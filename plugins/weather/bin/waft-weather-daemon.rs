//! Weather daemon - displays current weather conditions.
//!
//! This daemon fetches weather data from the Open-Meteo API and displays
//! temperature and conditions via an InfoCard widget.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "waft::weather-daemon"
//! latitude = 50.0755
//! longitude = 14.4378
//! units = "celsius"
//! update_interval = 600
//! ```

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use waft_plugin_sdk::*;

use waft_plugin_weather::{WeatherConfig, WeatherData, TemperatureUnit, fetch_weather, i18n::i18n};

/// Weather daemon state
struct WeatherDaemon {
    config: WeatherConfig,
    units: TemperatureUnit,
    state: Arc<Mutex<Option<Result<WeatherData, String>>>>,
}

impl WeatherDaemon {
    fn new() -> Self {
        let config = Self::load_config().unwrap_or_default();
        let units = TemperatureUnit::from_str(&config.units);
        log::debug!("Weather daemon config: {:?}", config);
        Self {
            config,
            units,
            state: Arc::new(Mutex::new(None)),
        }
    }

    fn load_config() -> Result<WeatherConfig> {
        let config_path = dirs::config_dir()
            .context("No config directory")?
            .join("waft/config.toml");

        if !config_path.exists() {
            log::debug!("Config file not found, using defaults");
            return Ok(WeatherConfig::default());
        }

        let content = std::fs::read_to_string(&config_path)
            .context("Failed to read config file")?;

        let root: toml::Table = toml::from_str(&content)
            .context("Failed to parse config file")?;

        if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
            for plugin in plugins {
                if let Some(table) = plugin.as_table() {
                    if let Some(id) = table.get("id").and_then(|v| v.as_str()) {
                        if id == "waft::weather-daemon" || id == "weather-daemon" {
                            return toml::Value::Table(table.clone())
                                .try_into()
                                .context("Failed to parse weather config");
                        }
                    }
                }
            }
        }

        Ok(WeatherConfig::default())
    }
}

#[async_trait::async_trait]
impl PluginDaemon for WeatherDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = self.state.lock().unwrap();
        let widget = match state.as_ref() {
            None => InfoCardBuilder::new(i18n().t("weather-placeholder"))
                .icon("weather-clear-symbolic")
                .description(i18n().t("weather-loading"))
                .build(),
            Some(Ok(data)) => {
                let temp_text = format!("{:.0}\u{00B0}{}", data.temperature, self.units.symbol());
                InfoCardBuilder::new(&temp_text)
                    .icon(data.condition.icon_name(data.is_day))
                    .description(data.condition.description())
                    .build()
            }
            Some(Err(msg)) => InfoCardBuilder::new("Weather")
                .icon("dialog-warning-symbolic")
                .description(msg)
                .build(),
        };

        vec![NamedWidget {
            id: "weather:main".to_string(),
            weight: 20,
            widget,
        }]
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        _action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Display-only, no actions
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    waft_plugin_sdk::init_daemon_logger("info");

    log::info!("Starting weather daemon...");

    let daemon = WeatherDaemon::new();
    let state = daemon.state.clone();
    let lat = daemon.config.latitude;
    let lon = daemon.config.longitude;
    let units = daemon.units;
    let interval = daemon.config.update_interval;

    let (server, notifier) = PluginServer::new("weather-daemon", daemon);

    // Spawn periodic weather fetch task
    tokio::spawn(async move {
        loop {
            log::debug!("Fetching weather update");
            match fetch_weather(lat, lon, units).await {
                Ok(data) => {
                    *state.lock().unwrap() = Some(Ok(data));
                }
                Err(e) => {
                    log::error!("Failed to fetch weather: {:?}", e);
                    let mut guard = state.lock().unwrap();
                    // Only set error if we have no previous data
                    if guard.is_none() {
                        *guard = Some(Err(i18n().t("weather-failed-to-load")));
                    }
                }
            }
            notifier.notify();
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });

    server.run().await?;

    Ok(())
}
