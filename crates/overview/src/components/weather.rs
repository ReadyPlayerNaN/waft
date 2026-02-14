//! Weather header component.
//!
//! Subscribes to weather entity type and renders temperature, condition
//! icon, and condition label. Hides when no weather entity exists.

use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::entity;
use waft_protocol::entity::weather::WeatherCondition;
use waft_ui_gtk::widgets::info_card::InfoCardWidget;

use waft_client::EntityStore;

/// Displays temperature as title, weather condition as description.
///
/// Automatically hides when no weather entity exists.
pub struct WeatherComponent {
    container: gtk::Box,
    _widget: Rc<InfoCardWidget>,
}

impl WeatherComponent {
    pub fn new(store: &Rc<EntityStore>) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .visible(false)
            .build();

        let widget = Rc::new(InfoCardWidget::new("weather-clear-symbolic", "", None));
        container.append(&widget.widget());

        let store_ref = store.clone();
        let widget_ref = widget.clone();
        let container_ref = container.clone();
        store.subscribe_type(entity::weather::ENTITY_TYPE, move || {
            let entities = store_ref
                .get_entities_typed::<entity::weather::Weather>(entity::weather::ENTITY_TYPE);
            match entities.first() {
                Some((_urn, weather)) => {
                    widget_ref.set_icon(&weather_icon(weather));
                    widget_ref.set_title(&format!("{:.0}\u{00B0}C", weather.temperature));
                    widget_ref.set_description(Some(weather_condition_label(weather.condition)));
                    container_ref.set_visible(true);
                }
                None => {
                    container_ref.set_visible(false);
                }
            }
        });

        Self {
            container,
            _widget: widget,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}

fn weather_icon(weather: &entity::weather::Weather) -> String {
    match (weather.condition, weather.day) {
        (WeatherCondition::Clear, true) => "weather-clear-symbolic",
        (WeatherCondition::Clear, false) => "weather-clear-night-symbolic",
        (WeatherCondition::PartlyCloudy, true) => "weather-few-clouds-symbolic",
        (WeatherCondition::PartlyCloudy, false) => "weather-few-clouds-night-symbolic",
        (WeatherCondition::Cloudy, _) => "weather-overcast-symbolic",
        (WeatherCondition::Fog, _) => "weather-fog-symbolic",
        (WeatherCondition::Drizzle, _) => "weather-showers-scattered-symbolic",
        (WeatherCondition::Rain, _) => "weather-showers-symbolic",
        (WeatherCondition::FreezingRain, _) => "weather-freezing-rain-symbolic",
        (WeatherCondition::Snow, _) => "weather-snow-symbolic",
        (WeatherCondition::Thunderstorm, _) => "weather-storm-symbolic",
    }
    .to_string()
}

fn weather_condition_label(condition: WeatherCondition) -> &'static str {
    match condition {
        WeatherCondition::Clear => "Clear",
        WeatherCondition::PartlyCloudy => "Partly cloudy",
        WeatherCondition::Cloudy => "Cloudy",
        WeatherCondition::Fog => "Fog",
        WeatherCondition::Drizzle => "Drizzle",
        WeatherCondition::Rain => "Rain",
        WeatherCondition::FreezingRain => "Freezing rain",
        WeatherCondition::Snow => "Snow",
        WeatherCondition::Thunderstorm => "Thunderstorm",
    }
}
