//! Weather plugin - displays current weather conditions.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Fetches weather data from Open-Meteo API.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use log::{debug, error};
use serde::Deserialize;

use waft_core::menu_state::MenuStore;
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, Widget, WidgetRegistrar, Slot};

use self::api::fetch_weather;
use self::ui::{WeatherState, WeatherWidget};
use self::values::TemperatureUnit;

mod api;
mod ui;
mod values;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("waft::weather", "Weather", "0.1.0");
waft_plugin_api::export_overview_plugin!(WeatherPlugin::new());

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
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for WeatherPlugin {
    fn default() -> Self {
        Self {
            widget: Rc::new(RefCell::new(None)),
            config: WeatherConfig::default(),
            tokio_handle: None,
        }
    }
}

impl WeatherPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    fn units(&self) -> TemperatureUnit {
        TemperatureUnit::from_str(&self.config.units)
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for WeatherPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("waft::weather")
    }

    fn configure(&mut self, settings: &toml::Table) -> Result<()> {
        self.config = settings.clone().try_into()?;
        debug!("[weather] Configured weather plugin: {:?}", self.config);
        Ok(())
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[weather] init() called");

        // Save the tokio handle and enter runtime context for this plugin's copy of tokio
        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();
        self.tokio_handle = Some(tokio_handle.clone());

        debug!("[weather] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());
        let units = self.units();
        let weather_widget = WeatherWidget::new(units);

        // Register the widget
        registrar.register_widget(Rc::new(Widget {
            id: "weather:main".to_string(),
            slot: Slot::Header,
            el: weather_widget.root.clone().upcast::<gtk::Widget>(),
            weight: 20,
        }));

        // Store the widget
        *self.widget.borrow_mut() = Some(weather_widget);

        // Get tokio handle
        let tokio_handle = self
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?
            .clone();

        // Initial fetch
        let widget_ref = self.widget.clone();
        let lat = self.config.latitude;
        let lon = self.config.longitude;

        // Fetch weather in background using glib spawn
        {
            let widget_ref = widget_ref.clone();
            let tokio_handle = tokio_handle.clone();
            glib::spawn_future_local(async move {
                match fetch_weather(lat, lon, units, &tokio_handle).await {
                    Ok(data) => {
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&WeatherState::Loaded(data));
                        }
                    }
                    Err(e) => {
                        error!("[weather] Failed to fetch weather: {:?}", e);
                        if let Some(ref widget) = *widget_ref.borrow() {
                            widget.update(&WeatherState::Error(waft_plugin_api::i18n::t("weather-failed-to-load")));
                        }
                    }
                }
            });
        }

        // Schedule periodic updates
        let update_interval = self.config.update_interval;
        glib::timeout_add_local(Duration::from_secs(update_interval), move || {
            let widget_ref = widget_ref.clone();
            let tokio_handle = tokio_handle.clone();
            glib::spawn_future_local(async move {
                debug!("[weather] Fetching weather update");
                match fetch_weather(lat, lon, units, &tokio_handle).await {
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
}
