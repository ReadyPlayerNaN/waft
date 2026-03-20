//! Clock plugin — displays current date and time.
//!
//! Provides a `clock` entity with time and date strings, updated on minute
//! boundaries. Connects to the waft daemon via the entity-based protocol.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "clock"
//! on_click = "gnome-calendar"  # Optional: command to run when clock is clicked
//! ```

use std::sync::LazyLock;
use std::time::Duration;

use anyhow::Result;
use chrono::{Local, Locale, Timelike};
use serde::Deserialize;
use waft_plugin::*;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/clock.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/clock.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

/// Clock configuration from config file.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct ClockConfig {
    /// Command to run when the clock is clicked. Empty means no action.
    #[serde(default)]
    on_click: String,
}

fn detect_chrono_locale() -> Locale {
    let bcp47 = waft_i18n::system_locale();
    let posix = bcp47.replace('-', "_");
    posix.parse::<Locale>().unwrap_or(Locale::en_US)
}

/// Clock plugin state.
struct ClockPlugin {
    config: ClockConfig,
    locale: Locale,
}

impl ClockPlugin {
    fn new() -> Result<Self> {
        let config: ClockConfig =
            waft_plugin::config::load_plugin_config("clock").unwrap_or_default();
        let locale = detect_chrono_locale();
        log::debug!("Clock config: {config:?}");
        log::debug!("Clock locale: {locale:?}");
        Ok(Self { config, locale })
    }

    fn format_date(&self) -> String {
        Local::now()
            .format_localized("%a, %d %b %Y", self.locale)
            .to_string()
    }

    fn format_time() -> String {
        Local::now().format("%H:%M").to_string()
    }
}

#[async_trait::async_trait]
impl Plugin for ClockPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let clock = entity::clock::Clock {
            time: Self::format_time(),
            date: self.format_date(),
        };
        vec![Entity::new(
            Urn::new("clock", entity::clock::ENTITY_TYPE, "default"),
            entity::clock::ENTITY_TYPE,
            &clock,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if action == "click" && !self.config.on_click.is_empty() {
            log::debug!("Clock clicked, running command: {}", self.config.on_click);

            let on_click_cmd = self.config.on_click.clone();
            tokio::task::spawn_blocking(move || {
                match std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&on_click_cmd)
                    .spawn()
                {
                    Ok(mut child) => {
                        if let Err(e) = child.wait() {
                            log::error!("Clock on_click command failed: {e}");
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to spawn clock on_click command: {e}");
                    }
                }
            });
        }
        Ok(serde_json::Value::Null)
    }
}

fn main() -> Result<()> {
    PluginRunner::new("clock", &[entity::clock::ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async move {
            let plugin = ClockPlugin::new()?;

            // Clock updates on minute boundaries (display is HH:MM)
            tokio::spawn(async move {
                loop {
                    let now = Local::now();
                    let secs_to_next = 60 - now.second() as u64;
                    tokio::time::sleep(Duration::from_secs(secs_to_next)).await;
                    notifier.notify();
                }
            });

            Ok(plugin)
        })
}
