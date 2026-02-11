//! Darkman daemon - dark mode toggle.
//!
//! This daemon provides a toggle to switch between light and dark mode via the darkman service.
//! It monitors the darkman D-Bus service for mode changes and updates the UI accordingly.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "waft::darkman-daemon"
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin_sdk::dbus_monitor::{monitor_signal, SignalMonitorConfig};
use waft_plugin_sdk::*;
use zbus::Connection;

const DARKMAN_DESTINATION: &str = "nl.whynothugo.darkman";
const DARKMAN_PATH: &str = "/nl/whynothugo/darkman";
const DARKMAN_INTERFACE: &str = "nl.whynothugo.darkman";

/// Darkman mode enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DarkmanMode {
    Dark,
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

    fn is_active(self) -> bool {
        matches!(self, Self::Dark)
    }
}

impl Default for DarkmanMode {
    fn default() -> Self {
        Self::Light
    }
}

/// Darkman configuration from config file
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct DarkmanConfig {}

/// Shared daemon state behind interior mutability.
struct DarkmanState {
    mode: DarkmanMode,
    busy: bool,
}

/// Darkman daemon state.
///
/// The state is shared with the D-Bus signal monitoring task via
/// `Arc<StdMutex>` so external changes (e.g. `darkman toggle`) update
/// the daemon's state immediately.
struct DarkmanDaemon {
    #[allow(dead_code)]
    config: DarkmanConfig,
    state: Arc<StdMutex<DarkmanState>>,
    conn: Connection,
}

impl DarkmanDaemon {
    async fn new() -> Result<Self> {
        let config = Self::load_config().unwrap_or_default();
        log::debug!("Darkman daemon config: {:?}", config);

        // Connect to session bus
        let conn = Connection::session()
            .await
            .context("Failed to connect to session bus")?;

        // Get initial mode
        let mode = Self::get_mode(&conn).await.unwrap_or_default();
        log::info!("Initial darkman mode: {:?}", mode);

        Ok(Self {
            config,
            state: Arc::new(StdMutex::new(DarkmanState { mode, busy: false })),
            conn,
        })
    }

    fn load_config() -> Result<DarkmanConfig> {
        waft_plugin_sdk::config::load_plugin_config("darkman-daemon")
            .context("Failed to load darkman config")
    }

    /// Get darkman mode via D-Bus property
    async fn get_mode(conn: &Connection) -> Result<DarkmanMode> {
        let proxy = zbus::Proxy::new(
            conn,
            DARKMAN_DESTINATION,
            DARKMAN_PATH,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create D-Bus proxy")?;

        // Call org.freedesktop.DBus.Properties.Get
        let (value,): (zbus::zvariant::OwnedValue,) = proxy
            .call("Get", &(DARKMAN_INTERFACE, "Mode"))
            .await
            .context("Failed to get Mode property")?;

        // Extract string from variant
        let val: zbus::zvariant::Value = value.into();
        let mode_str = if let zbus::zvariant::Value::Str(s) = val {
            s.to_string()
        } else {
            "light".to_string()
        };

        Ok(DarkmanMode::from_str(&mode_str).unwrap_or(DarkmanMode::Light))
    }

    /// Set darkman mode via D-Bus property
    async fn set_mode(&self, mode: DarkmanMode) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            DARKMAN_DESTINATION,
            DARKMAN_PATH,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create D-Bus proxy")?;

        // Call org.freedesktop.DBus.Properties.Set
        let value = zbus::zvariant::Value::from(mode.as_str().to_string());
        let _: () = proxy
            .call("Set", &(DARKMAN_INTERFACE, "Mode", value))
            .await
            .context("Failed to set Mode property")?;

        log::info!("Set darkman mode to: {:?}", mode);
        Ok(())
    }

    fn current_mode(&self) -> DarkmanMode {
        self.state.lock().unwrap().mode
    }

    fn shared_state(&self) -> Arc<StdMutex<DarkmanState>> {
        self.state.clone()
    }

    fn build_toggle_widget(&self) -> Widget {
        let state = self.state.lock().unwrap();
        FeatureToggleBuilder::new("Dark Mode")
            .icon("weather-clear-night-symbolic")
            .active(state.mode.is_active())
            .busy(state.busy)
            .on_toggle("toggle")
            .build()
    }
}

#[async_trait::async_trait]
impl PluginDaemon for DarkmanDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![NamedWidget {
            id: "darkman:toggle".to_string(),
            weight: 190,
            widget: self.build_toggle_widget(),
        }]
    }

    async fn handle_action(
        &self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action.id == "toggle" {
            log::debug!("Toggle action received");
            self.state.lock().unwrap().busy = true;

            // Toggle the mode
            let current = self.current_mode();
            let new_mode = match current {
                DarkmanMode::Dark => DarkmanMode::Light,
                DarkmanMode::Light => DarkmanMode::Dark,
            };

            // Set mode via D-Bus
            if let Err(e) = self.set_mode(new_mode).await {
                log::error!("Failed to set darkman mode: {}", e);
                self.state.lock().unwrap().busy = false;
                return Err(e.into());
            }

            // Update shared state
            {
                let mut state = self.state.lock().unwrap();
                state.mode = new_mode;
                state.busy = false;
            }
            log::debug!("Mode toggled to: {:?}", new_mode);
        }
        Ok(())
    }
}

/// Listen for `ModeChanged` D-Bus signals from darkman and update shared state.
async fn monitor_mode_signals(
    conn: Connection,
    state: Arc<StdMutex<DarkmanState>>,
    notifier: WidgetNotifier,
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
        log::info!("Darkman mode changed externally: {:?}", new_mode);
        darkman_state.mode = new_mode;
        Ok(true)
    })
    .await
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    waft_plugin_sdk::init_daemon_logger("info");

    log::info!("Starting darkman daemon...");

    // Create daemon
    let daemon = DarkmanDaemon::new().await?;

    // Grab shared handles before daemon is moved into the server
    let shared_state = daemon.shared_state();
    let monitor_conn = daemon.conn.clone();

    // Create server and notifier
    let (server, notifier) = PluginServer::new("darkman-daemon", daemon);

    // Listen for D-Bus ModeChanged signals (instant, no polling)
    tokio::spawn(async move {
        if let Err(e) = monitor_mode_signals(monitor_conn, shared_state, notifier).await {
            log::error!("D-Bus signal monitoring failed: {}", e);
        }
    });

    // Run server
    server.run().await?;

    Ok(())
}
