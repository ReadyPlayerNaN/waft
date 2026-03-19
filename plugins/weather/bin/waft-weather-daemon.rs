//! Weather daemon - displays current weather conditions.
//!
//! Provides a `weather` entity with temperature and condition data, updated
//! periodically via the Open-Meteo API. Connects to the waft daemon via the
//! entity-based protocol.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "weather"
//! location_name = "Prague, Czechia"
//! latitude = 50.0755
//! longitude = 14.4378
//! units = "celsius"
//! update_interval = 600
//! ```

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::Result;
use waft_plugin::*;

use waft_plugin_weather::{TemperatureUnit, WeatherConfig, WeatherData, fetch_weather};

/// Convert local WeatherCondition to protocol WeatherCondition.
fn to_protocol_condition(
    condition: waft_plugin_weather::WeatherCondition,
) -> entity::weather::WeatherCondition {
    match condition {
        waft_plugin_weather::WeatherCondition::Clear => entity::weather::WeatherCondition::Clear,
        waft_plugin_weather::WeatherCondition::PartlyCloudy => {
            entity::weather::WeatherCondition::PartlyCloudy
        }
        waft_plugin_weather::WeatherCondition::Cloudy => entity::weather::WeatherCondition::Cloudy,
        waft_plugin_weather::WeatherCondition::Fog => entity::weather::WeatherCondition::Fog,
        waft_plugin_weather::WeatherCondition::Drizzle => {
            entity::weather::WeatherCondition::Drizzle
        }
        waft_plugin_weather::WeatherCondition::Rain => entity::weather::WeatherCondition::Rain,
        waft_plugin_weather::WeatherCondition::FreezingRain => {
            entity::weather::WeatherCondition::FreezingRain
        }
        waft_plugin_weather::WeatherCondition::Snow => entity::weather::WeatherCondition::Snow,
        waft_plugin_weather::WeatherCondition::Thunderstorm => {
            entity::weather::WeatherCondition::Thunderstorm
        }
    }
}

/// Weather plugin state.
struct WeatherPlugin {
    state: Arc<StdMutex<Option<Result<WeatherData, String>>>>,
    config: Arc<StdMutex<WeatherConfig>>,
    wake: Arc<tokio::sync::Notify>,
}

impl WeatherPlugin {
    fn new(config: WeatherConfig) -> Self {
        Self {
            state: Arc::new(StdMutex::new(None)),
            config: Arc::new(StdMutex::new(config)),
            wake: Arc::new(tokio::sync::Notify::new()),
        }
    }

    fn current_data(&self) -> Option<Result<WeatherData, String>> {
        self.state.lock_or_recover().clone()
    }
}

#[async_trait::async_trait]
impl Plugin for WeatherPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let data = self.current_data();

        match data {
            Some(Ok(weather_data)) => {
                let weather = entity::weather::Weather {
                    temperature: weather_data.temperature,
                    condition: to_protocol_condition(weather_data.condition),
                    day: weather_data.is_day,
                };
                vec![Entity::new(
                    Urn::new("weather", entity::weather::ENTITY_TYPE, "default"),
                    entity::weather::ENTITY_TYPE,
                    &weather,
                )]
            }
            Some(Err(_)) | None => {
                // No data yet or error — return empty so the daemon knows we exist
                // but don't have data to show yet
                vec![]
            }
        }
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "update-config" => {
                let new_config: WeatherConfig = serde_json::from_value(params)?;

                // Validate
                if new_config.latitude < -90.0 || new_config.latitude > 90.0 {
                    return Err("Invalid latitude (must be -90 to 90)".into());
                }
                if new_config.longitude < -180.0 || new_config.longitude > 180.0 {
                    return Err("Invalid longitude (must be -180 to 180)".into());
                }

                log::info!(
                    "[weather] Config update: lat={}, lon={}, units={}, interval={}s",
                    new_config.latitude,
                    new_config.longitude,
                    new_config.units,
                    new_config.update_interval,
                );

                // Update in-memory config
                *self.config.lock_or_recover() = new_config;

                // Wake the fetch task to use new config immediately
                self.wake.notify_one();

                Ok(serde_json::Value::Null)
            }
            _ => Err(format!("Unknown action: {action}").into()),
        }
    }
}

fn main() -> Result<()> {
    PluginRunner::new("weather", &[entity::weather::ENTITY_TYPE])
        .i18n(
            waft_plugin_weather::i18n::i18n(),
            "plugin-name",
            "plugin-description",
        )
        .run(|notifier| async move {
            let config: WeatherConfig =
                waft_plugin::config::load_plugin_config("weather").unwrap_or_default();

            log::debug!(
                "Weather config: lat={}, lon={}, units={}, interval={}s",
                config.latitude,
                config.longitude,
                config.units,
                config.update_interval,
            );

            let plugin = WeatherPlugin::new(config);
            let state = plugin.state.clone();
            let config_ref = plugin.config.clone();
            let wake = plugin.wake.clone();

            // Spawn periodic weather fetch task
            tokio::spawn(async move {
                loop {
                    let cfg = config_ref.lock_or_recover().clone();

                    let units = TemperatureUnit::parse(&cfg.units);
                    log::debug!(
                        "Fetching weather update for ({}, {})",
                        cfg.latitude,
                        cfg.longitude
                    );

                    match fetch_weather(cfg.latitude, cfg.longitude, units).await {
                        Ok(data) => {
                            *state.lock_or_recover() = Some(Ok(data));
                        }
                        Err(e) => {
                            log::error!("Failed to fetch weather: {e:?}");
                            let mut guard = state.lock_or_recover();
                            // Only set error if we have no previous data
                            if guard.is_none() {
                                *guard = Some(Err(format!("Failed to load weather: {e}")));
                            }
                        }
                    }
                    notifier.notify();

                    // Sleep for the configured interval, but wake early on config changes
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(cfg.update_interval)) => {},
                        _ = wake.notified() => {
                            log::debug!("[weather] Config changed, fetching immediately");
                        }
                    }
                }
            });

            Ok(plugin)
        })
}
