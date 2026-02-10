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
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::{Arc, Mutex as StdMutex};
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

/// Darkman daemon state.
///
/// The `mode` field is shared with the D-Bus signal monitoring task via
/// `Arc<StdMutex>` so external changes (e.g. `darkman toggle`) update
/// the daemon's state immediately.
struct DarkmanDaemon {
    #[allow(dead_code)]
    config: DarkmanConfig,
    mode: Arc<StdMutex<DarkmanMode>>,
    busy: bool,
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
            mode: Arc::new(StdMutex::new(mode)),
            busy: false,
            conn,
        })
    }

    fn load_config() -> Result<DarkmanConfig> {
        // Load config from ~/.config/waft/config.toml
        let config_path = dirs::config_dir()
            .context("No config directory")?
            .join("waft/config.toml");

        if !config_path.exists() {
            log::debug!("Config file not found, using defaults");
            return Ok(DarkmanConfig::default());
        }

        let content =
            std::fs::read_to_string(&config_path).context("Failed to read config file")?;

        let root: toml::Table = toml::from_str(&content).context("Failed to parse config file")?;

        // Find darkman-daemon plugin config
        if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
            for plugin in plugins {
                if let Some(table) = plugin.as_table() {
                    if let Some(id) = table.get("id").and_then(|v| v.as_str()) {
                        if id == "waft::darkman-daemon" || id == "darkman-daemon" {
                            return toml::Value::Table(table.clone())
                                .try_into()
                                .context("Failed to parse darkman config");
                        }
                    }
                }
            }
        }

        Ok(DarkmanConfig::default())
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
        *self.mode.lock().unwrap()
    }

    fn shared_mode(&self) -> Arc<StdMutex<DarkmanMode>> {
        self.mode.clone()
    }

    fn build_toggle_widget(&self) -> Widget {
        FeatureToggleBuilder::new("Dark Mode")
            .icon("weather-clear-night-symbolic")
            .active(self.current_mode().is_active())
            .busy(self.busy)
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
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action.id == "toggle" {
            log::debug!("Toggle action received");
            self.busy = true;

            // Toggle the mode
            let current = self.current_mode();
            let new_mode = match current {
                DarkmanMode::Dark => DarkmanMode::Light,
                DarkmanMode::Light => DarkmanMode::Dark,
            };

            // Set mode via D-Bus
            if let Err(e) = self.set_mode(new_mode).await {
                log::error!("Failed to set darkman mode: {}", e);
                self.busy = false;
                return Err(e.into());
            }

            // Update shared state
            *self.mode.lock().unwrap() = new_mode;
            self.busy = false;
            log::debug!("Mode toggled to: {:?}", new_mode);
        }
        Ok(())
    }
}

/// Listen for `ModeChanged` D-Bus signals from darkman and update shared state.
async fn monitor_mode_signals(
    conn: Connection,
    mode: Arc<StdMutex<DarkmanMode>>,
    notifier: WidgetNotifier,
) -> Result<()> {
    // Subscribe to ModeChanged signal
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(DARKMAN_DESTINATION)?
        .path(DARKMAN_PATH)?
        .interface(DARKMAN_INTERFACE)?
        .member("ModeChanged")?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add match rule")?;

    log::info!("Listening for darkman ModeChanged signals");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("D-Bus stream error: {}", e);
                continue;
            }
        };

        let header = msg.header();
        if header.member().map(|m| m.as_str()) == Some("ModeChanged")
            && header.interface().map(|i| i.as_str()) == Some(DARKMAN_INTERFACE)
        {
            if let Ok(new_mode_str) = msg.body().deserialize::<String>() {
                let new_mode = DarkmanMode::from_str(&new_mode_str).unwrap_or_default();
                log::info!("Darkman mode changed externally: {:?}", new_mode);
                *mode.lock().unwrap() = new_mode;
                notifier.notify();
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting darkman daemon...");

    // Create daemon
    let daemon = DarkmanDaemon::new().await?;

    // Grab shared handles before daemon is moved into the server
    let shared_mode = daemon.shared_mode();
    let monitor_conn = daemon.conn.clone();

    // Create server and notifier
    let (server, notifier) = PluginServer::new("darkman-daemon", daemon);

    // Listen for D-Bus ModeChanged signals (instant, no polling)
    tokio::spawn(async move {
        if let Err(e) = monitor_mode_signals(monitor_conn, shared_mode, notifier).await {
            log::error!("D-Bus signal monitoring failed: {}", e);
        }
    });

    // Run server
    server.run().await?;

    Ok(())
}
