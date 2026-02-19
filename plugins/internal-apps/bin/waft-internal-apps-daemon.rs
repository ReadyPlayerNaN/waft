//! Internal apps plugin -- provides app entities for waft applications.
//!
//! Discovers internal waft binaries (currently waft-settings) and exposes
//! them as `app` entities. Supports "open" and "open-page" actions to
//! launch the application, optionally targeting a specific page.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use waft_plugin::*;

/// Internal apps plugin.
struct InternalAppsPlugin {
    settings_path: Option<PathBuf>,
}

impl InternalAppsPlugin {
    async fn new() -> Self {
        let settings_path = Self::resolve_binary("waft-settings").await;
        log::info!(
            "[internal-apps] waft-settings path: {:?}",
            settings_path
        );
        Self { settings_path }
    }

    /// Resolve a binary path using the same search order as the daemon's auto-detection:
    /// 1. `$WAFT_DAEMON_DIR` (explicit override)
    /// 2. `./target/debug` (dev builds without env var)
    /// 3. `./target/release` (release builds without env var)
    /// 4. `$PATH` via `which` (installed system binaries)
    async fn resolve_binary(name: &str) -> Option<PathBuf> {
        // 1. Check WAFT_DAEMON_DIR (explicit override)
        if let Ok(dir) = std::env::var("WAFT_DAEMON_DIR") {
            let candidate = PathBuf::from(&dir).join(name);
            if candidate.is_file() {
                log::debug!("[internal-apps] Found {name} in WAFT_DAEMON_DIR: {candidate:?}");
                return Some(candidate);
            }
        }

        // 2 & 3. Auto-detect from standard build output dirs (mirrors daemon logic)
        for dir in &["./target/debug", "./target/release"] {
            let candidate = PathBuf::from(dir).join(name);
            if candidate.is_file() {
                log::debug!("[internal-apps] Found {name} in {dir}: {candidate:?}");
                return Some(candidate);
            }
        }

        // 4. Fall back to PATH via `which`
        match tokio::process::Command::new("which")
            .arg(name)
            .output()
            .await
        {
            Ok(output) if output.status.success() => {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                Some(PathBuf::from(path))
            }
            Ok(_) => {
                log::debug!("[internal-apps] {name} not found in PATH");
                None
            }
            Err(e) => {
                log::debug!("[internal-apps] Failed to check for {name} binary: {e}");
                None
            }
        }
    }

    /// Spawn waft-settings with optional arguments, reaping the child process.
    fn spawn_settings(
        path: &PathBuf,
        args: &[&str],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut cmd = Command::new(path);
        for arg in args {
            cmd.arg(arg);
        }

        match cmd.spawn() {
            Ok(child) => {
                std::thread::spawn(move || {
                    let mut child = child;
                    match child.wait() {
                        Ok(status) => {
                            log::debug!("[internal-apps] waft-settings exited: {status}");
                        }
                        Err(e) => {
                            log::warn!("[internal-apps] Failed to wait on waft-settings: {e}");
                        }
                    }
                });
                Ok(())
            }
            Err(e) => {
                log::error!("[internal-apps] Failed to spawn waft-settings: {e}");
                Err(Box::new(e))
            }
        }
    }
}

#[async_trait::async_trait]
impl Plugin for InternalAppsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        if self.settings_path.is_none() {
            return Vec::new();
        }

        let app = entity::app::App {
            name: "Settings".to_string(),
            icon: "preferences-system-symbolic".to_string(),
            available: true,
        };

        vec![Entity::new(
            Urn::new("internal-apps", entity::app::ENTITY_TYPE, "waft-settings"),
            entity::app::ENTITY_TYPE,
            &app,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Some(ref path) = self.settings_path else {
            log::warn!("[internal-apps] action '{action}' ignored: waft-settings not available");
            return Ok(());
        };
        match action.as_str() {
            "open" => {
                Self::spawn_settings(path, &[])?;
            }
            "open-page" => {
                let page = params
                    .get("page")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bluetooth");
                Self::spawn_settings(path, &["--page", page])?;
            }
            other => {
                log::debug!("[internal-apps] Unknown action: {other}");
            }
        }
        Ok(())
    }

    fn can_stop(&self) -> bool {
        true
    }
}

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides_full(
        &[entity::app::ENTITY_TYPE],
        "Internal Apps",
        "Provides launchable application entities for internal waft apps",
    ) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");
    log::info!("Starting internal-apps plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = InternalAppsPlugin::new().await;
        let (runtime, _notifier) = PluginRuntime::new("internal-apps", plugin);
        runtime.run().await?;
        Ok(())
    })
}
