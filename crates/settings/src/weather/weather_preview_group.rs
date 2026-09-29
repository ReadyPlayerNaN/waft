//! Weather preview group -- dumb widget.
//!
//! Displays current temperature, condition text, and weather icon.
//! Hidden until `apply_props` is called with data.

use adw::prelude::*;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;

/// Presentational widget showing current weather conditions.
pub struct WeatherPreviewGroup {
    pub root: adw::PreferencesGroup,
    icon: IconWidget,
    temperature_row: adw::ActionRow,
    condition_row: adw::ActionRow,
}

impl WeatherPreviewGroup {
    pub fn new() -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/weather-preview-group.ui");
        let group: adw::PreferencesGroup = builder
            .object("root")
            .expect("weather-preview-group.ui must contain root");
        let icon_slot: gtk::Box = builder
            .object("icon_slot")
            .expect("weather-preview-group.ui must contain icon_slot");
        let temperature_row: adw::ActionRow = builder
            .object("temperature_row")
            .expect("weather-preview-group.ui must contain temperature_row");
        let condition_row: adw::ActionRow = builder
            .object("condition_row")
            .expect("weather-preview-group.ui must contain condition_row");
        group.set_title(&t("weather-current"));
        temperature_row.set_title(&t("weather-temperature"));
        condition_row.set_title(&t("weather-condition"));
        let icon = IconWidget::from_name("weather-clear-symbolic", 32);
        icon_slot.append(icon.widget());

        Self {
            root: group,
            icon,
            temperature_row,
            condition_row,
        }
    }

    /// Update the preview with current weather data.
    pub fn apply_props(&self, temperature: f64, condition: &str, icon_name: &str) {
        self.root.set_visible(true);
        self.temperature_row
            .set_subtitle(&format!("{temperature:.1}\u{00B0}"));
        self.condition_row.set_subtitle(condition);
        self.icon.set_icon(icon_name);
    }
}
