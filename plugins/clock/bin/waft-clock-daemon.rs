//! Clock daemon - displays current date and time.
//!
//! This daemon provides a clock widget that shows the current date and time.
//! Updates are pushed to connected clients on minute boundaries via WidgetNotifier.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "waft::clock-daemon"
//! on_click = "gnome-calendar"  # Optional: command to run when clock is clicked
//! ```

use std::time::Duration;
use anyhow::{Context, Result};
use chrono::{Local, Locale, Timelike};
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

fn detect_chrono_locale() -> Locale {
    let bcp47 = waft_i18n::system_locale();
    // chrono Locale uses POSIX underscores ("cs_CZ"), sys-locale returns BCP47 hyphens ("cs-CZ")
    let posix = bcp47.replace('-', "_");
    posix.parse::<Locale>().unwrap_or(Locale::en_US)
}

/// Clock daemon state
struct ClockDaemon {
    config: ClockConfig,
    locale: Locale,
}

impl ClockDaemon {
    fn new() -> Result<Self> {
        let config = Self::load_config().unwrap_or_default();
        let locale = detect_chrono_locale();
        log::debug!("Clock daemon config: {:?}", config);
        log::debug!("Clock daemon locale: {:?}", locale);
        Ok(Self { config, locale })
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

    fn format_date(locale: Locale) -> String {
        Local::now()
            .format_localized("%a, %d %b %Y", locale)
            .to_string()
    }

    fn format_time() -> String {
        Local::now().format("%H:%M").to_string()
    }

    fn build_clock_widget(&self) -> Widget {
        let time_text = Self::format_time();
        let date_text = Self::format_date(self.locale);

        // If on_click is configured, make it a clickable MenuRow
        if !self.config.on_click.is_empty() {
            Widget::MenuRow {
                icon: Some("appointment-symbolic".to_string()),
                label: time_text,
                sublabel: Some(date_text),
                trailing: None,
                sensitive: true,
                on_click: Some(Action {
                    id: "click".to_string(),
                    params: ActionParams::None,
                }),
            }
        } else {
            // Non-clickable version: just show as container
            let date_label = Widget::Label {
                text: date_text,
                css_classes: vec![
                    "title-3".to_string(),
                    "dim-label".to_string(),
                    "clock-date".to_string(),
                ],
            };

            let time_label = Widget::Label {
                text: time_text,
                css_classes: vec![
                    "title-1".to_string(),
                    "clock-time".to_string(),
                ],
            };

            Widget::Container {
                orientation: Orientation::Vertical,
                spacing: 2,
                css_classes: vec!["clock-container".to_string()],
                children: vec![date_label.into(), time_label.into()],
            }
        }
    }
}

#[async_trait::async_trait]
impl PluginDaemon for ClockDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![NamedWidget {
            id: "clock:main".to_string(),
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

    // Create server and notifier
    let (server, notifier) = PluginServer::new("clock-daemon", daemon);

    // Clock updates on minute boundaries (display is HH:MM)
    tokio::spawn(async move {
        loop {
            let now = Local::now();
            let secs_to_next = 60 - now.second() as u64;
            tokio::time::sleep(Duration::from_secs(secs_to_next)).await;
            notifier.notify();
        }
    });

    // Run server
    server.run().await?;

    Ok(())
}
