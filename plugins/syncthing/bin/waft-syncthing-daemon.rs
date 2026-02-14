//! Syncthing plugin -- backup method toggle.
//!
//! Provides a `backup-method` entity for syncthing. Detects syncthing
//! availability by checking if the binary exists in PATH and whether
//! the syncthing user service is running. Toggles the service via
//! `systemctl --user start/stop syncthing`.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "syncthing"
//! ```

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use waft_plugin::*;

/// Shared daemon state.
struct SyncthingState {
    enabled: bool,
    available: bool,
}

/// Syncthing plugin.
struct SyncthingPlugin {
    state: Arc<StdMutex<SyncthingState>>,
}

impl SyncthingPlugin {
    async fn new() -> Result<Self> {
        let available = Self::detect_syncthing().await;
        let enabled = if available {
            Self::check_service_active().await
        } else {
            false
        };

        log::info!("Syncthing plugin: available={available}, enabled={enabled}");

        Ok(Self {
            state: Arc::new(StdMutex::new(SyncthingState { enabled, available })),
        })
    }

    /// Check if syncthing binary exists in PATH.
    async fn detect_syncthing() -> bool {
        match tokio::process::Command::new("which")
            .arg("syncthing")
            .output()
            .await
        {
            Ok(output) => output.status.success(),
            Err(e) => {
                log::debug!("Failed to check for syncthing binary: {e}");
                false
            }
        }
    }

    /// Check if the syncthing user service is currently active.
    async fn check_service_active() -> bool {
        match tokio::process::Command::new("systemctl")
            .args(["--user", "is-active", "syncthing"])
            .output()
            .await
        {
            Ok(output) => output.status.success(),
            Err(e) => {
                log::debug!("Failed to check syncthing service status: {e}");
                false
            }
        }
    }

    /// Start or stop the syncthing user service.
    async fn set_service_enabled(enable: bool) -> Result<()> {
        let action = if enable { "start" } else { "stop" };
        let output = tokio::process::Command::new("systemctl")
            .args(["--user", action, "syncthing"])
            .output()
            .await
            .context("failed to run systemctl")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("systemctl --user {action} syncthing failed: {stderr}");
        }

        log::info!("Syncthing service {action}ed successfully");
        Ok(())
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, SyncthingState> {
        match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[syncthing] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
    }

    fn shared_state(&self) -> Arc<StdMutex<SyncthingState>> {
        self.state.clone()
    }
}

#[async_trait::async_trait]
impl Plugin for SyncthingPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        if !state.available {
            return Vec::new();
        }
        let method = entity::storage::BackupMethod {
            name: "Syncthing".to_string(),
            enabled: state.enabled,
            icon: "drive-harddisk-symbolic".to_string(),
        };
        vec![Entity::new(
            Urn::new(
                "syncthing",
                entity::storage::BACKUP_METHOD_ENTITY_TYPE,
                "syncthing",
            ),
            entity::storage::BACKUP_METHOD_ENTITY_TYPE,
            &method,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "toggle" => {
                let was_enabled = self.lock_state().enabled;
                let result = SyncthingPlugin::set_service_enabled(!was_enabled).await;

                match result {
                    Ok(()) => {
                        self.lock_state().enabled = !was_enabled;
                        log::debug!("Syncthing toggled to: {}", !was_enabled);
                    }
                    Err(e) => {
                        log::error!("Failed to toggle syncthing: {e}");
                        return Err(e.into());
                    }
                }
            }
            "enable" => {
                if !self.lock_state().enabled {
                    match SyncthingPlugin::set_service_enabled(true).await {
                        Ok(()) => {
                            self.lock_state().enabled = true;
                            log::debug!("Syncthing enabled");
                        }
                        Err(e) => {
                            log::error!("Failed to enable syncthing: {e}");
                            return Err(e.into());
                        }
                    }
                }
            }
            "disable" => {
                if self.lock_state().enabled {
                    match SyncthingPlugin::set_service_enabled(false).await {
                        Ok(()) => {
                            self.lock_state().enabled = false;
                            log::debug!("Syncthing disabled");
                        }
                        Err(e) => {
                            log::error!("Failed to disable syncthing: {e}");
                            return Err(e.into());
                        }
                    }
                }
            }
            other => {
                log::debug!("[syncthing] Unknown action: {other}");
            }
        }
        Ok(())
    }

    fn can_stop(&self) -> bool {
        true
    }
}

/// Periodically poll the syncthing service status to detect external changes.
///
/// Checks every 30 seconds whether the service state has changed (e.g. user
/// started/stopped it via systemctl directly).
async fn monitor_service_state(state: Arc<StdMutex<SyncthingState>>, notifier: EntityNotifier) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let active = SyncthingPlugin::check_service_active().await;
        let changed = {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::warn!("[syncthing] mutex poisoned in monitor, recovering: {e}");
                    e.into_inner()
                }
            };
            if guard.enabled != active {
                guard.enabled = active;
                true
            } else {
                false
            }
        };

        if changed {
            log::info!("Syncthing service state changed externally: enabled={active}");
            notifier.notify();
        }
    }
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[entity::storage::BACKUP_METHOD_ENTITY_TYPE]) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting syncthing plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = SyncthingPlugin::new().await?;
        let shared_state = plugin.shared_state();

        let (runtime, notifier) = PluginRuntime::new("syncthing", plugin);

        // Monitor service state for external changes
        tokio::spawn(async move {
            monitor_service_state(shared_state, notifier).await;
            log::debug!("[syncthing] monitor task stopped");
        });

        runtime.run().await?;
        Ok(())
    })
}
