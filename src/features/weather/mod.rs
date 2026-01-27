//! Weather plugin - displays current weather conditions.

mod api;
pub mod values;

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error};
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::*;

use crate::plugin::{Plugin, PluginId, Slot, Widget};
use crate::ui::weather::{WeatherState, WeatherWidget};

use self::api::fetch_weather;
use self::values::TemperatureUnit;

/// Configuration for the weather plugin.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WeatherConfig {
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
            latitude: 50.0755,
            longitude: 14.4378,
            units: "celsius".to_string(),
            update_interval: 600,
        }
    }
}

pub struct WeatherPlugin {
    widget: Rc<RefCell<Option<WeatherWidget>>>,
    config: WeatherConfig,
}

impl WeatherPlugin {
    pub fn new() -> Self {
        Self {
            widget: Rc::new(RefCell::new(None)),
            config: WeatherConfig::default(),
        }
    }

    fn units(&self) -> TemperatureUnit {
        TemperatureUnit::from_str(&self.config.units)
    }
}

#[async_trait(?Send)]
impl Plugin for WeatherPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::weather")
    }

    fn configure(&mut self, settings: &toml::Table) -> Result<()> {
        self.config = settings.clone().try_into()?;
        debug!("Configured weather plugin: {:?}", self.config);
        Ok(())
    }

    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    async fn create_elements(&mut self, _app: &gtk::Application) -> Result<()> {
        let units = self.units();
        let weather_widget = WeatherWidget::new(units);

        // Store the widget
        *self.widget.borrow_mut() = Some(weather_widget);

        // Initial fetch
        let widget_ref = self.widget.clone();
        let lat = self.config.latitude;
        let lon = self.config.longitude;

        // Fetch weather in background using glib spawn
        {
            let widget_ref = widget_ref.clone();
            glib::spawn_future_local(async move {
                match fetch_weather(lat, lon, units).await {
                    Ok(data) => {
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&WeatherState::Loaded(data));
                        }
                    }
                    Err(e) => {
                        error!("[weather] Failed to fetch weather: {:?}", e);
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&WeatherState::Error("Failed to load".to_string()));
                        }
                    }
                }
            });
        }

        // Schedule periodic updates
        let update_interval = self.config.update_interval;
        glib::timeout_add_local(Duration::from_secs(update_interval), move || {
            let widget_ref = widget_ref.clone();
            glib::spawn_future_local(async move {
                debug!("[weather] Fetching weather update");
                match fetch_weather(lat, lon, units).await {
                    Ok(data) => {
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&WeatherState::Loaded(data));
                        }
                    }
                    Err(e) => {
                        error!("[weather] Failed to fetch weather: {:?}", e);
                        // Don't update to error state on refresh failures,
                        // keep showing the last known good data
                    }
                }
            });
            glib::ControlFlow::Continue
        });

        Ok(())
    }

    fn get_widgets(&self) -> Vec<Arc<Widget>> {
        match *self.widget.borrow() {
            Some(ref weather) => {
                vec![Arc::new(Widget {
                    slot: Slot::Header,
                    el: weather.root.clone().upcast::<gtk::Widget>(),
                    weight: 20,
                })]
            }
            None => vec![],
        }
    }
}
