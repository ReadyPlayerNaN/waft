//! Darkman plugin — dark mode toggle.
//!
//! Provides a `dark-mode` entity via the darkman D-Bus service.
//! Monitors D-Bus signals for external mode changes and updates
//! the entity accordingly.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "darkman"
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::dbus_monitor::{SignalMonitorConfig, monitor_signal};
use waft_plugin::*;
use zbus::Connection;

const DARKMAN_DESTINATION: &str = "nl.whynothugo.darkman";
const DARKMAN_PATH: &str = "/nl/whynothugo/darkman";
const DARKMAN_INTERFACE: &str = "nl.whynothugo.darkman";

/// Darkman mode enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DarkmanMode {
    Dark,
    #[default]
    Light,
}

impl DarkmanMode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    fn active(self) -> bool {
        matches!(self, Self::Dark)
    }
}

/// Darkman configuration from config file.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct DarkmanConfig {}

/// Shared daemon state.
struct DarkmanState {
    mode: DarkmanMode,
}

/// Darkman plugin.
struct DarkmanPlugin {
    #[allow(dead_code)]
    config: DarkmanConfig,
    state: Arc<StdMutex<DarkmanState>>,
    conn: Connection,
}

impl DarkmanPlugin {
    async fn new() -> Result<Self> {
        let config: DarkmanConfig =
            waft_plugin::config::load_plugin_config("darkman").unwrap_or_default();
        log::debug!("Darkman config: {config:?}");

        let conn = Connection::session()
            .await
            .context("failed to connect to session bus")?;

        let mode = Self::get_mode(&conn).await.unwrap_or_default();
        log::info!("Initial darkman mode: {mode:?}");

        Ok(Self {
            config,
            state: Arc::new(StdMutex::new(DarkmanState { mode })),
            conn,
        })
    }

    /// Get darkman mode via D-Bus property.
    async fn get_mode(conn: &Connection) -> Result<DarkmanMode> {
        let proxy = zbus::Proxy::new(
            conn,
            DARKMAN_DESTINATION,
            DARKMAN_PATH,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("failed to create D-Bus proxy")?;

        let (value,): (zbus::zvariant::OwnedValue,) = proxy
            .call("Get", &(DARKMAN_INTERFACE, "Mode"))
            .await
            .context("failed to get Mode property")?;

        let val: zbus::zvariant::Value = value.into();
        let mode_str = if let zbus::zvariant::Value::Str(s) = val {
            s.to_string()
        } else {
            "light".to_string()
        };

        Ok(DarkmanMode::from_str(&mode_str).unwrap_or(DarkmanMode::Light))
    }

    /// Set darkman mode via D-Bus property.
    async fn set_mode(&self, mode: DarkmanMode) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            DARKMAN_DESTINATION,
            DARKMAN_PATH,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("failed to create D-Bus proxy")?;

        let value = zbus::zvariant::Value::from(mode.as_str().to_string());
        let _: () = proxy
            .call("Set", &(DARKMAN_INTERFACE, "Mode", value))
            .await
            .context("failed to set Mode property")?;

        log::info!("Set darkman mode to: {mode:?}");
        Ok(())
    }

    fn current_mode(&self) -> DarkmanMode {
        match self.state.lock() {
            Ok(g) => g.mode,
            Err(e) => {
                log::warn!("Mutex poisoned, recovering: {e}");
                e.into_inner().mode
            }
        }
    }

    fn shared_state(&self) -> Arc<StdMutex<DarkmanState>> {
        self.state.clone()
    }
}

#[async_trait::async_trait]
impl Plugin for DarkmanPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let mode = self.current_mode();
        let dark_mode = entity::display::DarkMode {
            active: mode.active(),
        };
        vec![Entity::new(
            Urn::new("darkman", entity::display::DARK_MODE_ENTITY_TYPE, "default"),
            entity::display::DARK_MODE_ENTITY_TYPE,
            &dark_mode,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action == "toggle" {
            log::debug!("Toggle action received");

            let current = self.current_mode();
            let new_mode = match current {
                DarkmanMode::Dark => DarkmanMode::Light,
                DarkmanMode::Light => DarkmanMode::Dark,
            };

            if let Err(e) = self.set_mode(new_mode).await {
                log::error!("Failed to set darkman mode: {e}");
                return Err(e.into());
            }

            // Update shared state
            match self.state.lock() {
                Ok(mut guard) => guard.mode = new_mode,
                Err(e) => {
                    log::warn!("Mutex poisoned, recovering: {e}");
                    e.into_inner().mode = new_mode;
                }
            }
            log::debug!("Mode toggled to: {new_mode:?}");
        }
        Ok(())
    }
}

/// Listen for `ModeChanged` D-Bus signals from darkman and update shared state.
async fn monitor_mode_signals(
    conn: Connection,
    state: Arc<StdMutex<DarkmanState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let config = SignalMonitorConfig::builder()
        .sender(DARKMAN_DESTINATION)
        .path(DARKMAN_PATH)
        .interface(DARKMAN_INTERFACE)
        .member("ModeChanged")
        .build()?;

    monitor_signal(conn, config, state, notifier, |msg, darkman_state| {
        let new_mode_str: String = msg.body().deserialize()?;
        let new_mode = DarkmanMode::from_str(&new_mode_str).unwrap_or_default();
        log::info!("Darkman mode changed externally: {new_mode:?}");
        darkman_state.mode = new_mode;
        Ok(true)
    })
    .await
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[entity::display::DARK_MODE_ENTITY_TYPE]) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting darkman plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = DarkmanPlugin::new().await?;

        // Grab shared handles before plugin is moved into the runtime
        let shared_state = plugin.shared_state();
        let monitor_conn = plugin.conn.clone();

        let (runtime, notifier) = PluginRuntime::new("darkman", plugin);

        // Listen for D-Bus ModeChanged signals (instant, no polling)
        tokio::spawn(async move {
            if let Err(e) = monitor_mode_signals(monitor_conn, shared_state, notifier).await {
                log::error!("D-Bus signal monitoring failed: {e}");
            }
        });

        runtime.run().await?;
        Ok(())
    })
}
