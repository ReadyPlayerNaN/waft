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
//! latitude = 50.0755
//! longitude = 14.4378
//! units = "celsius"
//! update_interval = 600
//! ```

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::{Context, Result};
use waft_plugin::*;

use waft_plugin_weather::{WeatherConfig, WeatherData, TemperatureUnit, fetch_weather};

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
}

impl WeatherPlugin {
    fn new() -> Self {
        Self {
            state: Arc::new(StdMutex::new(None)),
        }
    }

    fn current_data(&self) -> Option<Result<WeatherData, String>> {
        match self.state.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::warn!("[weather] mutex poisoned, recovering: {e}");
                e.into_inner().clone()
            }
        }
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
        _action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Display-only, no actions
        Ok(())
    }
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[entity::weather::ENTITY_TYPE]) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting weather plugin...");

    let config: WeatherConfig =
        waft_plugin::config::load_plugin_config("weather").unwrap_or_default();
    let lat = config.latitude;
    let lon = config.longitude;
    let units = TemperatureUnit::from_str(&config.units);
    let interval = config.update_interval;

    log::debug!("Weather config: lat={lat}, lon={lon}, units={units:?}, interval={interval}s");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = WeatherPlugin::new();
        let state = plugin.state.clone();

        let (runtime, notifier) = PluginRuntime::new("weather", plugin);

        // Spawn periodic weather fetch task
        tokio::spawn(async move {
            loop {
                log::debug!("Fetching weather update");
                match fetch_weather(lat, lon, units).await {
                    Ok(data) => {
                        match state.lock() {
                            Ok(mut guard) => *guard = Some(Ok(data)),
                            Err(e) => {
                                log::warn!("[weather] mutex poisoned, recovering: {e}");
                                *e.into_inner() = Some(Ok(data));
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch weather: {e:?}");
                        match state.lock() {
                            Ok(mut guard) => {
                                // Only set error if we have no previous data
                                if guard.is_none() {
                                    *guard = Some(Err(format!("Failed to load weather: {e}")));
                                }
                            }
                            Err(poison) => {
                                log::warn!("[weather] mutex poisoned, recovering: {poison}");
                                let inner = poison.into_inner();
                                if inner.is_none() {
                                    // Don't overwrite existing data
                                }
                            }
                        }
                    }
                }
                notifier.notify();
                tokio::time::sleep(Duration::from_secs(interval)).await;
            }
        });

        runtime.run().await?;
        Ok(())
    })
}
