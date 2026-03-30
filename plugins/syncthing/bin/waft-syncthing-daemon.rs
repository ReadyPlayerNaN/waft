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

use std::sync::LazyLock;

use anyhow::Result;
use std::sync::{Arc, Mutex as StdMutex};

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/syncthing.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/syncthing.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

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
            .map_err(|e| anyhow::anyhow!("failed to run systemctl: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("systemctl --user {action} syncthing failed: {stderr}");
        }

        log::info!("Syncthing service {action}ed successfully");
        Ok(())
    }

    fn shared_state(&self) -> Arc<StdMutex<SyncthingState>> {
        self.state.clone()
    }
}

#[async_trait::async_trait]
impl Plugin for SyncthingPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.state.lock_or_recover();
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
    ) -> anyhow::Result<serde_json::Value> {
        match action.as_str() {
            "toggle" => {
                let was_enabled = self.state.lock_or_recover().enabled;
                let result = SyncthingPlugin::set_service_enabled(!was_enabled).await;

                match result {
                    Ok(()) => {
                        self.state.lock_or_recover().enabled = !was_enabled;
                        log::debug!("Syncthing toggled to: {}", !was_enabled);
                    }
                    Err(e) => {
                        log::error!("Failed to toggle syncthing: {e}");
                        return Err(e);
                    }
                }
            }
            "enable" => {
                if !self.state.lock_or_recover().enabled {
                    match SyncthingPlugin::set_service_enabled(true).await {
                        Ok(()) => {
                            self.state.lock_or_recover().enabled = true;
                            log::debug!("Syncthing enabled");
                        }
                        Err(e) => {
                            log::error!("Failed to enable syncthing: {e}");
                            return Err(e);
                        }
                    }
                }
            }
            "disable" => {
                if self.state.lock_or_recover().enabled {
                    match SyncthingPlugin::set_service_enabled(false).await {
                        Ok(()) => {
                            self.state.lock_or_recover().enabled = false;
                            log::debug!("Syncthing disabled");
                        }
                        Err(e) => {
                            log::error!("Failed to disable syncthing: {e}");
                            return Err(e);
                        }
                    }
                }
            }
            other => {
                log::debug!("[syncthing] Unknown action: {other}");
            }
        }
        Ok(serde_json::Value::Null)
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
            let mut guard = state.lock_or_recover();
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
    PluginRunner::new("syncthing", &[entity::storage::BACKUP_METHOD_ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async move {
            let plugin = SyncthingPlugin::new().await?;
            let shared_state = plugin.shared_state();

            // Monitor service state for external changes
            tokio::spawn(async move {
                monitor_service_state(shared_state, notifier).await;
                log::debug!("[syncthing] monitor task stopped");
            });

            Ok(plugin)
        })
}
