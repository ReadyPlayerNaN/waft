//! Location settings group -- dumb widget.
//!
//! Provides city search, units selection, and update interval configuration.
//! Emits output events for config changes and search requests.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Input properties for location settings.
pub struct LocationSettingsProps {
    pub location_name: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub units: String,
    pub update_interval: u64,
}

/// Output events from the location settings group.
pub enum LocationSettingsOutput {
    /// User changed configuration via UI controls.
    ConfigChanged {
        location_name: Option<String>,
        latitude: f64,
        longitude: f64,
        units: String,
        update_interval: u64,
    },
    /// User requested a city search.
    SearchRequested(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(LocationSettingsOutput)>>>>;

/// Dumb widget for weather location configuration.
pub struct LocationSettingsGroup {
    pub root: gtk::Box,
    output_cb: OutputCallback,
    location_row: adw::ActionRow,
    search_entry: adw::EntryRow,
    results_group: adw::PreferencesGroup,
    units_row: adw::ComboRow,
    interval_row: adw::ComboRow,
    current_lat: Rc<Cell<f64>>,
    current_lon: Rc<Cell<f64>>,
    current_location_name: Rc<RefCell<Option<String>>>,
}

fn units_options() -> Vec<String> {
    vec![t("weather-celsius"), t("weather-fahrenheit")]
}

const INTERVAL_VALUES: &[u64] = &[300, 600, 1800, 3600];

fn interval_options() -> Vec<(String, u64)> {
    vec![
        (t("weather-interval-5m"), 300),
        (t("weather-interval-10m"), 600),
        (t("weather-interval-30m"), 1800),
        (t("weather-interval-1h"), 3600),
    ]
}

impl LocationSettingsGroup {
    pub fn new(props: &LocationSettingsProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let current_lat = Rc::new(Cell::new(props.latitude));
        let current_lon = Rc::new(Cell::new(props.longitude));
        let current_location_name: Rc<RefCell<Option<String>>> =
            Rc::new(RefCell::new(props.location_name.clone()));

        // -- Location group --
        let location_group = adw::PreferencesGroup::builder().title(t("weather-location")).build();

        let location_label = match &props.location_name {
            Some(name) => name.clone(),
            None => format!("{:.4}, {:.4}", props.latitude, props.longitude),
        };
        let location_row = adw::ActionRow::builder()
            .title(t("weather-current-location"))
            .subtitle(&location_label)
            .build();
        location_group.add(&location_row);

        let search_entry = adw::EntryRow::builder().title(t("weather-search-city")).build();
        location_group.add(&search_entry);

        // Wire search on activate (Enter key)
        {
            let cb = output_cb.clone();
            search_entry.connect_apply(move |entry| {
                let text = entry.text().to_string();
                if text.len() >= 2
                    && let Some(ref callback) = *cb.borrow()
                {
                    callback(LocationSettingsOutput::SearchRequested(text));
                }
            });
        }

        root.append(&location_group);

        // -- Search results group (hidden until results arrive) --
        let results_group = adw::PreferencesGroup::builder()
            .title(t("weather-search-results"))
            .visible(false)
            .build();
        root.append(&results_group);

        // -- Settings group --
        let settings_group = adw::PreferencesGroup::builder().title(t("weather-settings")).build();

        // Units combo
        let units_labels = units_options();
        let units_refs: Vec<&str> = units_labels.iter().map(std::string::String::as_str).collect();
        let units_model = gtk::StringList::new(&units_refs);
        let units_row = adw::ComboRow::builder()
            .title(t("weather-temp-unit"))
            .model(&units_model)
            .build();
        let units_idx = match props.units.as_str() {
            "fahrenheit" => 1,
            _ => 0,
        };
        units_row.set_selected(units_idx);
        settings_group.add(&units_row);

        // Interval combo
        let intervals = interval_options();
        let interval_label_refs: Vec<&str> = intervals.iter().map(|(l, _)| l.as_str()).collect();
        let interval_model = gtk::StringList::new(&interval_label_refs);
        let interval_row = adw::ComboRow::builder()
            .title(t("weather-update-interval"))
            .model(&interval_model)
            .build();
        let interval_idx = INTERVAL_VALUES
            .iter()
            .position(|v| *v == props.update_interval)
            .unwrap_or(1) as u32; // default to "10 minutes"
        interval_row.set_selected(interval_idx);
        settings_group.add(&interval_row);

        // Wire units change
        {
            let cb = output_cb.clone();
            let lat = current_lat.clone();
            let lon = current_lon.clone();
            let name = current_location_name.clone();
            let interval_ref = interval_row.clone();
            units_row.connect_selected_notify(move |row| {
                let units = match row.selected() {
                    1 => "fahrenheit",
                    _ => "celsius",
                };
                let interval = INTERVAL_VALUES
                    .get(interval_ref.selected() as usize)
                    .copied()
                    .unwrap_or(600);
                if let Some(ref callback) = *cb.borrow() {
                    callback(LocationSettingsOutput::ConfigChanged {
                        location_name: name.borrow().clone(),
                        latitude: lat.get(),
                        longitude: lon.get(),
                        units: units.to_string(),
                        update_interval: interval,
                    });
                }
            });
        }

        // Wire interval change
        {
            let cb = output_cb.clone();
            let lat = current_lat.clone();
            let lon = current_lon.clone();
            let name = current_location_name.clone();
            let units_ref = units_row.clone();
            interval_row.connect_selected_notify(move |row| {
                let interval = INTERVAL_VALUES
                    .get(row.selected() as usize)
                    .copied()
                    .unwrap_or(600);
                let units = match units_ref.selected() {
                    1 => "fahrenheit",
                    _ => "celsius",
                };
                if let Some(ref callback) = *cb.borrow() {
                    callback(LocationSettingsOutput::ConfigChanged {
                        location_name: name.borrow().clone(),
                        latitude: lat.get(),
                        longitude: lon.get(),
                        units: units.to_string(),
                        update_interval: interval,
                    });
                }
            });
        }

        root.append(&settings_group);

        Self {
            root,
            output_cb,
            location_row,
            search_entry,
            results_group,
            units_row,
            interval_row,
            current_lat,
            current_lon,
            current_location_name,
        }
    }

    /// Display search results. Each tuple is (display_name, latitude, longitude).
    pub fn show_search_results(&self, results: &[(String, f64, f64)]) {
        // Clear previous results
        while let Some(child) = self.results_group.first_child() {
            self.results_group.remove(&child);
        }

        if results.is_empty() {
            let empty_row = adw::ActionRow::builder()
                .title(t("weather-no-locations"))
                .build();
            self.results_group.add(&empty_row);
        } else {
            for (name, lat, lon) in results {
                let row = adw::ActionRow::builder()
                    .title(name)
                    .subtitle(format!("{lat:.4}, {lon:.4}"))
                    .activatable(true)
                    .build();

                let cb = self.output_cb.clone();
                let result_name = name.clone();
                let result_lat = *lat;
                let result_lon = *lon;
                let location_row_ref = self.location_row.clone();
                let lat_cell = self.current_lat.clone();
                let lon_cell = self.current_lon.clone();
                let name_cell = self.current_location_name.clone();
                let results_group_ref = self.results_group.clone();
                let search_entry_ref = self.search_entry.clone();
                let units_ref = self.units_row.clone();
                let interval_ref = self.interval_row.clone();

                row.connect_activated(move |_| {
                    // Update current state
                    lat_cell.set(result_lat);
                    lon_cell.set(result_lon);
                    *name_cell.borrow_mut() = Some(result_name.clone());
                    location_row_ref.set_subtitle(&result_name);
                    search_entry_ref.set_text("");
                    results_group_ref.set_visible(false);

                    // Emit config changed
                    let units = match units_ref.selected() {
                        1 => "fahrenheit",
                        _ => "celsius",
                    };
                    let interval = INTERVAL_VALUES
                        .get(interval_ref.selected() as usize)
                        .copied()
                        .unwrap_or(600);
                    if let Some(ref callback) = *cb.borrow() {
                        callback(LocationSettingsOutput::ConfigChanged {
                            location_name: Some(result_name.clone()),
                            latitude: result_lat,
                            longitude: result_lon,
                            units: units.to_string(),
                            update_interval: interval,
                        });
                    }
                });

                self.results_group.add(&row);
            }
        }

        self.results_group.set_visible(true);
    }

    /// Display a search error message.
    pub fn show_search_error(&self, error: &str) {
        while let Some(child) = self.results_group.first_child() {
            self.results_group.remove(&child);
        }

        let error_row = adw::ActionRow::builder().title(error).build();
        self.results_group.add(&error_row);
        self.results_group.set_visible(true);
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(LocationSettingsOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
