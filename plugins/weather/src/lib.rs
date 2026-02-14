pub mod api;
pub mod i18n;
pub mod values;

pub use api::fetch_weather;
pub use values::{TemperatureUnit, WeatherCondition, WeatherData};

use serde::{Deserialize, Serialize};

/// Configuration for the weather plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WeatherConfig {
    /// Human-readable location name (e.g., "Prague, Czechia").
    pub location_name: Option<String>,
    /// Latitude for weather location.
    pub latitude: f64,
    /// Longitude for weather location.
    pub longitude: f64,
    /// Temperature units: "celsius" or "fahrenheit".
    pub units: String,
    /// Update interval in seconds (default: 600 = 10 minutes).
    pub update_interval: u64,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            location_name: Some("Prague, Czechia".to_string()),
            latitude: 50.0755,
            longitude: 14.4378,
            units: "celsius".to_string(),
            update_interval: 600,
        }
    }
}
