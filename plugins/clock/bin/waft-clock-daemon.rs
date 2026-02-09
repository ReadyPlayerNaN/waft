//! Clock daemon - displays current date and time.
//!
//! This daemon provides a clock widget that shows the current date and time.
//! The time is updated whenever the overview requests widgets.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "waft::clock-daemon"
//! on_click = "gnome-calendar"  # Optional: command to run when clock is clicked
//! ```

use anyhow::{Context, Result};
use chrono::Local;
use serde::Deserialize;
use waft_plugin_sdk::*;

/// Clock configuration from config file
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct ClockConfig {
    /// Command to run when the clock is clicked. Empty means no action.
    #[serde(default)]
    on_click: String,
}

/// Clock daemon state
struct ClockDaemon {
    config: ClockConfig,
}

impl ClockDaemon {
    fn new() -> Result<Self> {
        let config = Self::load_config().unwrap_or_default();
        log::debug!("Clock daemon config: {:?}", config);
        Ok(Self { config })
    }

    fn load_config() -> Result<ClockConfig> {
        // Load config from ~/.config/waft/config.toml
        let config_path = dirs::config_dir()
            .context("No config directory")?
            .join("waft/config.toml");

        if !config_path.exists() {
            log::debug!("Config file not found, using defaults");
            return Ok(ClockConfig::default());
        }

        let content = std::fs::read_to_string(&config_path)
            .context("Failed to read config file")?;

        let root: toml::Table = toml::from_str(&content)
            .context("Failed to parse config file")?;

        // Find clock-daemon plugin config
        if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
            for plugin in plugins {
                if let Some(table) = plugin.as_table() {
                    if let Some(id) = table.get("id").and_then(|v| v.as_str()) {
                        if id == "waft::clock-daemon" || id == "clock-daemon" {
                            return toml::Value::Table(table.clone())
                                .try_into()
                                .context("Failed to parse clock config");
                        }
                    }
                }
            }
        }

        Ok(ClockConfig::default())
    }

    fn format_date() -> String {
        Local::now().format("%a, %d %b %Y").to_string()
    }

    fn format_time() -> String {
        Local::now().format("%H:%M").to_string()
    }

    fn build_clock_widget(&self) -> Widget {
        let date_label = Widget::Label {
            text: Self::format_date(),
            css_classes: vec![
                "title-3".to_string(),
                "dim-label".to_string(),
                "clock-date".to_string(),
            ],
        };

        let time_label = Widget::Label {
            text: Self::format_time(),
            css_classes: vec![
                "title-1".to_string(),
                "clock-time".to_string(),
            ],
        };

        Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 2,
            css_classes: vec!["clock-container".to_string()],
            children: vec![date_label, time_label],
        }
    }
}

#[async_trait::async_trait]
impl PluginDaemon for ClockDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![NamedWidget {
            id: "clock:main".to_string(),
            slot: Slot::Header,
            weight: 10,
            widget: self.build_clock_widget(),
        }]
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Handle click action
        if action.id == "click" && !self.config.on_click.is_empty() {
            log::debug!("Clock clicked, running command: {}", self.config.on_click);

            let on_click_cmd = self.config.on_click.clone();
            tokio::task::spawn_blocking(move || {
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&on_click_cmd)
                    .spawn()
                {
                    Ok(mut child) => {
                        // Wait for child to complete
                        if let Err(e) = child.wait() {
                            log::error!("Clock on_click command failed: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to spawn clock on_click command: {}", e);
                    }
                }
            });
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting clock daemon...");

    // Create daemon
    let daemon = ClockDaemon::new()?;

    // Create and run server
    let server = PluginServer::new("clock-daemon", daemon);

    // Run server (widget updates happen on each GetWidgets call)
    server.run().await?;

    Ok(())
}
