//! Pure GTK4 Weather widget.
//!
//! Displays current weather with icon, temperature, and condition.

use gtk::prelude::*;

use crate::features::weather::values::{TemperatureUnit, WeatherData};

/// State of the weather widget.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Loading state is part of the API but weather is loaded eagerly
pub enum WeatherState {
    Loading,
    Loaded(WeatherData),
    Error(String),
}

/// Pure GTK4 weather widget - displays weather icon, temperature, and condition.
pub struct WeatherWidget {
    pub root: gtk::Box,
    icon: gtk::Image,
    temp_label: gtk::Label,
    condition_label: gtk::Label,
    spinner: gtk::Spinner,
    error_label: gtk::Label,
    content_box: gtk::Box,
    units: TemperatureUnit,
}

impl WeatherWidget {
    /// Create a new weather widget.
    pub fn new(units: TemperatureUnit) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["weather-container"])
            .build();

        // Weather icon
        let icon = gtk::Image::builder()
            .icon_name("weather-clear-symbolic")
            .pixel_size(32)
            .css_classes(["weather-icon"])
            .build();

        // Temperature and condition labels
        let labels_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .valign(gtk::Align::Center)
            .build();

        let temp_label = gtk::Label::builder()
            .label(&crate::i18n::t("weather-placeholder"))
            .xalign(0.0)
            .css_classes(["title-3", "weather-temp"])
            .build();

        let condition_label = gtk::Label::builder()
            .label(&crate::i18n::t("weather-loading"))
            .xalign(0.0)
            .css_classes(["dim-label", "weather-condition"])
            .build();

        labels_box.append(&temp_label);
        labels_box.append(&condition_label);

        // Content box (icon + labels)
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        content_box.append(&icon);
        content_box.append(&labels_box);

        // Loading spinner
        let spinner = gtk::Spinner::builder().spinning(true).build();

        // Error label
        let error_label = gtk::Label::builder()
            .label("")
            .css_classes(["error", "weather-error"])
            .visible(false)
            .build();

        // Initially show loading state
        root.append(&spinner);
        root.append(&content_box);
        root.append(&error_label);

        content_box.set_visible(false);

        Self {
            root,
            icon,
            temp_label,
            condition_label,
            spinner,
            error_label,
            content_box,
            units,
        }
    }

    /// Update the widget with new weather state.
    pub fn update(&self, state: &WeatherState) {
        match state {
            WeatherState::Loading => {
                self.spinner.set_visible(true);
                self.spinner.set_spinning(true);
                self.content_box.set_visible(false);
                self.error_label.set_visible(false);
            }
            WeatherState::Loaded(data) => {
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.content_box.set_visible(true);
                self.error_label.set_visible(false);

                // Update icon
                self.icon
                    .set_icon_name(Some(data.condition.icon_name(data.is_day)));

                // Update temperature
                let temp_text = format!("{:.0}\u{00B0}{}", data.temperature, self.units.symbol());
                self.temp_label.set_label(&temp_text);

                // Update condition
                self.condition_label
                    .set_label(&data.condition.description());
            }
            WeatherState::Error(msg) => {
                self.spinner.set_visible(false);
                self.spinner.set_spinning(false);
                self.content_box.set_visible(false);
                self.error_label.set_visible(true);
                self.error_label.set_label(msg);
            }
        }
    }
}
