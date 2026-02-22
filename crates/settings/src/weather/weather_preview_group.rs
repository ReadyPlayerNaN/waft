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
        let group = adw::PreferencesGroup::builder()
            .title(t("weather-current"))
            .visible(false)
            .build();

        let icon = IconWidget::from_name("weather-clear-symbolic", 32);

        let temperature_row = adw::ActionRow::builder().title(t("weather-temperature")).build();
        temperature_row.add_prefix(icon.widget());
        group.add(&temperature_row);

        let condition_row = adw::ActionRow::builder().title(t("weather-condition")).build();
        group.add(&condition_row);

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
