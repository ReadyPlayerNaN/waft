//! Weather settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `weather` entity type.
//! Composes location settings and weather preview groups.
//! Runs geocoding searches in background threads and updates results.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::weather::{self, Weather, WeatherCondition};

use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::weather::geocoding;
use crate::weather::location_settings_group::{
    LocationSettingsGroup, LocationSettingsOutput, LocationSettingsProps,
};
use crate::weather::weather_preview_group::WeatherPreviewGroup;

/// Smart container for the Weather settings page.
pub struct WeatherPage {
    pub root: gtk::Box,
}

impl WeatherPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Weather preview (hidden until entity arrives)
        let preview = Rc::new(WeatherPreviewGroup::new());
        root.append(&preview.root);

        // Location settings
        let default_props = LocationSettingsProps {
            location_name: None,
            latitude: 50.0755,
            longitude: 14.4378,
            units: "celsius".to_string(),
            update_interval: 600,
        };
        let location_group = LocationSettingsGroup::new(&default_props);
        root.append(&location_group.root);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-weather");
            idx.add_section("weather", &page_title, &t("weather-current"), "weather-current", &preview.root);
            idx.add_section("weather", &page_title, &t("weather-settings"), "weather-settings", &location_group.root);
            idx.add_input("weather", &page_title, &t("weather-settings"), &t("weather-temp-unit"), "weather-temp-unit", &location_group.root);
            idx.add_input("weather", &page_title, &t("weather-settings"), &t("weather-update-interval"), "weather-update-interval", &location_group.root);
        }

        // Wrap in Rc for sharing between closures
        let location_group = Rc::new(location_group);

        // Current URN (set on first entity arrival)
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Wire location settings output
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let lg_for_search = location_group.clone();

            location_group.connect_output(move |output| match output {
                LocationSettingsOutput::ConfigChanged {
                    location_name,
                    latitude,
                    longitude,
                    units,
                    update_interval,
                } => {
                    if let Some(ref urn) = *urn_ref.borrow() {
                        cb(
                            urn.clone(),
                            "update-config".to_string(),
                            serde_json::json!({
                                "location_name": location_name,
                                "latitude": latitude,
                                "longitude": longitude,
                                "units": units,
                                "update_interval": update_interval,
                            }),
                        );
                    }
                }
                LocationSettingsOutput::SearchRequested(query) => {
                    // Run geocoding in background thread
                    let (tx, rx) = flume::bounded::<Result<Vec<(String, f64, f64)>, String>>(1);
                    let lg_ref = lg_for_search.clone();

                    std::thread::spawn(move || {
                        let rt = match tokio::runtime::Runtime::new() {
                            Ok(rt) => rt,
                            Err(e) => {
                                if tx.send(Err(format!("Runtime error: {e}"))).is_err() {
                                    log::debug!("[weather-page] search result receiver dropped");
                                }
                                return;
                            }
                        };
                        let result = rt.block_on(geocoding::search_cities(&query));
                        let mapped = result.map(|results| {
                            results
                                .into_iter()
                                .map(|r| (r.display_name, r.latitude, r.longitude))
                                .collect()
                        });
                        if tx.send(mapped).is_err() {
                            log::debug!("[weather-page] search result receiver dropped");
                        }
                    });

                    // Receive results in glib context
                    gtk::glib::spawn_future_local(async move {
                        match rx.recv_async().await {
                            Ok(Ok(results)) => {
                                lg_ref.show_search_results(&results);
                            }
                            Ok(Err(error)) => {
                                lg_ref.show_search_error(&error);
                            }
                            Err(e) => {
                                log::warn!("[weather-page] search channel error: {e}");
                            }
                        }
                    });
                }
            });
        }

        // Subscribe to weather entities
        {
            let store = entity_store.clone();
            let preview_ref = preview.clone();
            let urn_ref = current_urn;

            entity_store.subscribe_type(weather::ENTITY_TYPE, move || {
                let entities: Vec<(Urn, Weather)> = store.get_entities_typed(weather::ENTITY_TYPE);

                if let Some((urn, w)) = entities.first() {
                    *urn_ref.borrow_mut() = Some(urn.clone());

                    let icon_name = condition_icon_name(&w.condition, w.day);
                    let condition_text = condition_description(&w.condition);

                    preview_ref.apply_props(w.temperature, &condition_text, icon_name);
                }
            });
        }

        // Initial reconciliation with cached data
        {
            let store = entity_store.clone();
            let preview_ref = preview;
            let urn_ref: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));
            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, Weather)> = store.get_entities_typed(weather::ENTITY_TYPE);

                if let Some((urn, w)) = entities.first() {
                    log::debug!(
                        "[weather-page] Initial reconciliation: {} weather entities",
                        entities.len()
                    );
                    *urn_ref.borrow_mut() = Some(urn.clone());

                    let icon_name = condition_icon_name(&w.condition, w.day);
                    let condition_text = condition_description(&w.condition);
                    preview_ref.apply_props(w.temperature, &condition_text, icon_name);
                }
            });
        }

        // Prevent location_group from being dropped
        std::mem::forget(location_group);

        Self { root }
    }
}

/// Map WeatherCondition enum to icon name string.
fn condition_icon_name(condition: &WeatherCondition, day: bool) -> &'static str {
    match (condition, day) {
        (WeatherCondition::Clear, true) => "weather-clear-symbolic",
        (WeatherCondition::Clear, false) => "weather-clear-night-symbolic",
        (WeatherCondition::PartlyCloudy, true) => "weather-few-clouds-symbolic",
        (WeatherCondition::PartlyCloudy, false) => "weather-few-clouds-night-symbolic",
        (WeatherCondition::Cloudy, _) => "weather-overcast-symbolic",
        (WeatherCondition::Fog, _) => "weather-fog-symbolic",
        (WeatherCondition::Drizzle, _) => "weather-showers-scattered-symbolic",
        (WeatherCondition::Rain, _) => "weather-showers-symbolic",
        (WeatherCondition::FreezingRain, _) => "weather-showers-symbolic",
        (WeatherCondition::Snow, _) => "weather-snow-symbolic",
        (WeatherCondition::Thunderstorm, _) => "weather-storm-symbolic",
    }
}

/// Map WeatherCondition enum to human-readable description.
fn condition_description(condition: &WeatherCondition) -> String {
    match condition {
        WeatherCondition::Clear => t("weather-clear"),
        WeatherCondition::PartlyCloudy => t("weather-partly-cloudy"),
        WeatherCondition::Cloudy => t("weather-cloudy"),
        WeatherCondition::Fog => t("weather-fog"),
        WeatherCondition::Drizzle => t("weather-drizzle"),
        WeatherCondition::Rain => t("weather-rain"),
        WeatherCondition::FreezingRain => t("weather-freezing-rain"),
        WeatherCondition::Snow => t("weather-snow"),
        WeatherCondition::Thunderstorm => t("weather-thunderstorm"),
    }
}
