//! Night light configuration settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `night-light-config` entity type.
//! Provides grouped configuration UI with mode-aware field enabling.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::i18n::t;
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::display::{
    FieldState, NIGHT_LIGHT_CONFIG_ENTITY_TYPE, NightLightConfig,
};

/// Smart container for night light configuration settings.
pub struct NightLightConfigSection {
    pub root: gtk::Box,
}

fn apply_field_state(widget: &impl gtk::prelude::WidgetExt, state: Option<&FieldState>) {
    match state {
        Some(FieldState::Editable) => widget.set_sensitive(true),
        Some(FieldState::ReadOnly) | Some(FieldState::Disabled) => widget.set_sensitive(false),
        None => widget.set_sensitive(true),
    }
}

fn subtitle_for_state(state: Option<&FieldState>) -> String {
    match state {
        Some(FieldState::ReadOnly) => t("nlc-calculated-auto"),
        Some(FieldState::Disabled) => t("nlc-not-used"),
        _ => String::new(),
    }
}

impl NightLightConfigSection {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-appearance");
        // Colors group
        let colors_title = t("nlc-colors");
        idx.add_section_deferred("appearance", &page_title, &colors_title, "nlc-colors");
        idx.add_input_deferred("appearance", &page_title, &colors_title, &t("nlc-night-temp"), "nlc-night-temp");
        idx.add_input_deferred("appearance", &page_title, &colors_title, &t("nlc-night-gamma"), "nlc-night-gamma");
        idx.add_input_deferred("appearance", &page_title, &colors_title, &t("nlc-day-temp"), "nlc-day-temp");
        idx.add_input_deferred("appearance", &page_title, &colors_title, &t("nlc-day-gamma"), "nlc-day-gamma");
        idx.add_input_deferred("appearance", &page_title, &colors_title, &t("nlc-static-temp"), "nlc-static-temp");
        idx.add_input_deferred("appearance", &page_title, &colors_title, &t("nlc-static-gamma"), "nlc-static-gamma");
        // Timing group
        let timing_title = t("nlc-timing");
        idx.add_section_deferred("appearance", &page_title, &timing_title, "nlc-timing");
        idx.add_input_deferred("appearance", &page_title, &timing_title, &t("nlc-transition-mode"), "nlc-transition-mode");
        idx.add_input_deferred("appearance", &page_title, &timing_title, &t("nlc-sunrise"), "nlc-sunrise");
        idx.add_input_deferred("appearance", &page_title, &timing_title, &t("nlc-sunset"), "nlc-sunset");
        idx.add_input_deferred("appearance", &page_title, &timing_title, &t("nlc-transition-duration"), "nlc-transition-duration");
        // Location group
        let location_title = t("nlc-location");
        idx.add_section_deferred("appearance", &page_title, &location_title, "nlc-location");
        idx.add_input_deferred("appearance", &page_title, &location_title, &t("nlc-latitude"), "nlc-latitude");
        idx.add_input_deferred("appearance", &page_title, &location_title, &t("nlc-longitude"), "nlc-longitude");
        // Advanced group
        let advanced_title = t("nlc-advanced");
        idx.add_section_deferred("appearance", &page_title, &advanced_title, "nlc-advanced");
        idx.add_input_deferred("appearance", &page_title, &advanced_title, &t("nlc-backend"), "nlc-backend");
        idx.add_input_deferred("appearance", &page_title, &advanced_title, &t("nlc-smoothing"), "nlc-smoothing");
        idx.add_input_deferred("appearance", &page_title, &advanced_title, &t("nlc-startup-duration"), "nlc-startup-duration");
        idx.add_input_deferred("appearance", &page_title, &advanced_title, &t("nlc-shutdown-duration"), "nlc-shutdown-duration");
        idx.add_input_deferred("appearance", &page_title, &advanced_title, &t("nlc-adaptive-interval"), "nlc-adaptive-interval");
        idx.add_input_deferred("appearance", &page_title, &advanced_title, &t("nlc-update-interval"), "nlc-update-interval");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .visible(false)
            .build();

        // ---- Colors Group ----
        let colors_group = adw::PreferencesGroup::builder()
            .title(t("nlc-colors"))
            .build();
        root.append(&colors_group);

        let night_temp_row = adw::SpinRow::builder()
            .title(t("nlc-night-temp"))
            .subtitle(t("nlc-night-temp-sub"))
            .adjustment(&gtk::Adjustment::new(3500.0, 1000.0, 10000.0, 100.0, 500.0, 0.0))
            .digits(0)
            .build();
        colors_group.add(&night_temp_row);

        let night_gamma_row = adw::SpinRow::builder()
            .title(t("nlc-night-gamma"))
            .subtitle(t("nlc-night-gamma-sub"))
            .adjustment(&gtk::Adjustment::new(100.0, 10.0, 200.0, 1.0, 10.0, 0.0))
            .digits(0)
            .build();
        colors_group.add(&night_gamma_row);

        let day_temp_row = adw::SpinRow::builder()
            .title(t("nlc-day-temp"))
            .subtitle(t("nlc-day-temp-sub"))
            .adjustment(&gtk::Adjustment::new(6500.0, 1000.0, 10000.0, 100.0, 500.0, 0.0))
            .digits(0)
            .build();
        colors_group.add(&day_temp_row);

        let day_gamma_row = adw::SpinRow::builder()
            .title(t("nlc-day-gamma"))
            .subtitle(t("nlc-day-gamma-sub"))
            .adjustment(&gtk::Adjustment::new(100.0, 10.0, 200.0, 1.0, 10.0, 0.0))
            .digits(0)
            .build();
        colors_group.add(&day_gamma_row);

        let static_temp_row = adw::SpinRow::builder()
            .title(t("nlc-static-temp"))
            .subtitle(t("nlc-static-temp-sub"))
            .adjustment(&gtk::Adjustment::new(4500.0, 1000.0, 10000.0, 100.0, 500.0, 0.0))
            .digits(0)
            .build();
        colors_group.add(&static_temp_row);

        let static_gamma_row = adw::SpinRow::builder()
            .title(t("nlc-static-gamma"))
            .subtitle(t("nlc-static-gamma-sub"))
            .adjustment(&gtk::Adjustment::new(100.0, 10.0, 200.0, 1.0, 10.0, 0.0))
            .digits(0)
            .build();
        colors_group.add(&static_gamma_row);

        // ---- Timing Group ----
        let timing_group = adw::PreferencesGroup::builder()
            .title(t("nlc-timing"))
            .build();
        root.append(&timing_group);

        let mode_model = gtk::StringList::new(&["geo", "static", "center", "finish_by", "start_at"]);
        let mode_row = adw::ComboRow::builder()
            .title(t("nlc-transition-mode"))
            .subtitle(t("nlc-transition-mode-sub"))
            .model(&mode_model)
            .build();
        timing_group.add(&mode_row);

        let sunrise_row = adw::EntryRow::builder()
            .title(t("nlc-sunrise"))
            .show_apply_button(true)
            .build();
        timing_group.add(&sunrise_row);

        let sunset_row = adw::EntryRow::builder()
            .title(t("nlc-sunset"))
            .show_apply_button(true)
            .build();
        timing_group.add(&sunset_row);

        let transition_duration_row = adw::SpinRow::builder()
            .title(t("nlc-transition-duration"))
            .subtitle(t("nlc-transition-duration-sub"))
            .adjustment(&gtk::Adjustment::new(30.0, 1.0, 180.0, 1.0, 10.0, 0.0))
            .digits(0)
            .build();
        timing_group.add(&transition_duration_row);

        // ---- Location Group ----
        let location_group = adw::PreferencesGroup::builder()
            .title(t("nlc-location"))
            .build();
        root.append(&location_group);

        let latitude_row = adw::EntryRow::builder()
            .title(t("nlc-latitude"))
            .show_apply_button(true)
            .build();
        location_group.add(&latitude_row);

        let longitude_row = adw::EntryRow::builder()
            .title(t("nlc-longitude"))
            .show_apply_button(true)
            .build();
        location_group.add(&longitude_row);

        // ---- Advanced Group ----
        let advanced_group = adw::PreferencesGroup::builder()
            .title(t("nlc-advanced"))
            .build();
        root.append(&advanced_group);

        let backend_model = gtk::StringList::new(&["auto", "hyprland", "wayland"]);
        let backend_row = adw::ComboRow::builder()
            .title(t("nlc-backend"))
            .model(&backend_model)
            .build();
        advanced_group.add(&backend_row);

        let smoothing_row = adw::SwitchRow::builder()
            .title(t("nlc-smoothing"))
            .subtitle(t("nlc-smoothing-sub"))
            .build();
        advanced_group.add(&smoothing_row);

        let startup_duration_row = adw::SpinRow::builder()
            .title(t("nlc-startup-duration"))
            .subtitle(t("nlc-startup-duration-sub"))
            .adjustment(&gtk::Adjustment::new(1.0, 0.0, 30.0, 0.1, 1.0, 0.0))
            .digits(1)
            .build();
        advanced_group.add(&startup_duration_row);

        let shutdown_duration_row = adw::SpinRow::builder()
            .title(t("nlc-shutdown-duration"))
            .subtitle(t("nlc-shutdown-duration-sub"))
            .adjustment(&gtk::Adjustment::new(1.0, 0.0, 30.0, 0.1, 1.0, 0.0))
            .digits(1)
            .build();
        advanced_group.add(&shutdown_duration_row);

        let adaptive_interval_row = adw::SpinRow::builder()
            .title(t("nlc-adaptive-interval"))
            .subtitle(t("nlc-adaptive-interval-sub"))
            .adjustment(&gtk::Adjustment::new(100.0, 10.0, 10000.0, 10.0, 100.0, 0.0))
            .digits(0)
            .build();
        advanced_group.add(&adaptive_interval_row);

        let update_interval_row = adw::SpinRow::builder()
            .title(t("nlc-update-interval"))
            .subtitle(t("nlc-update-interval-sub"))
            .adjustment(&gtk::Adjustment::new(5.0, 1.0, 600.0, 1.0, 10.0, 0.0))
            .digits(0)
            .build();
        advanced_group.add(&update_interval_row);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-appearance");
            // Colors group
            let colors_title = t("nlc-colors");
            idx.add_section("appearance", &page_title, &colors_title, "nlc-colors", &colors_group);
            idx.add_input("appearance", &page_title, &colors_title, &t("nlc-night-temp"), "nlc-night-temp", &night_temp_row);
            idx.add_input("appearance", &page_title, &colors_title, &t("nlc-night-gamma"), "nlc-night-gamma", &night_gamma_row);
            idx.add_input("appearance", &page_title, &colors_title, &t("nlc-day-temp"), "nlc-day-temp", &day_temp_row);
            idx.add_input("appearance", &page_title, &colors_title, &t("nlc-day-gamma"), "nlc-day-gamma", &day_gamma_row);
            idx.add_input("appearance", &page_title, &colors_title, &t("nlc-static-temp"), "nlc-static-temp", &static_temp_row);
            idx.add_input("appearance", &page_title, &colors_title, &t("nlc-static-gamma"), "nlc-static-gamma", &static_gamma_row);
            // Timing group
            let timing_title = t("nlc-timing");
            idx.add_section("appearance", &page_title, &timing_title, "nlc-timing", &timing_group);
            idx.add_input("appearance", &page_title, &timing_title, &t("nlc-transition-mode"), "nlc-transition-mode", &mode_row);
            idx.add_input("appearance", &page_title, &timing_title, &t("nlc-sunrise"), "nlc-sunrise", &sunrise_row);
            idx.add_input("appearance", &page_title, &timing_title, &t("nlc-sunset"), "nlc-sunset", &sunset_row);
            idx.add_input("appearance", &page_title, &timing_title, &t("nlc-transition-duration"), "nlc-transition-duration", &transition_duration_row);
            // Location group
            let location_title = t("nlc-location");
            idx.add_section("appearance", &page_title, &location_title, "nlc-location", &location_group);
            idx.add_input("appearance", &page_title, &location_title, &t("nlc-latitude"), "nlc-latitude", &latitude_row);
            idx.add_input("appearance", &page_title, &location_title, &t("nlc-longitude"), "nlc-longitude", &longitude_row);
            // Advanced group
            let advanced_title = t("nlc-advanced");
            idx.add_section("appearance", &page_title, &advanced_title, "nlc-advanced", &advanced_group);
            idx.add_input("appearance", &page_title, &advanced_title, &t("nlc-backend"), "nlc-backend", &backend_row);
            idx.add_input("appearance", &page_title, &advanced_title, &t("nlc-smoothing"), "nlc-smoothing", &smoothing_row);
            idx.add_input("appearance", &page_title, &advanced_title, &t("nlc-startup-duration"), "nlc-startup-duration", &startup_duration_row);
            idx.add_input("appearance", &page_title, &advanced_title, &t("nlc-shutdown-duration"), "nlc-shutdown-duration", &shutdown_duration_row);
            idx.add_input("appearance", &page_title, &advanced_title, &t("nlc-adaptive-interval"), "nlc-adaptive-interval", &adaptive_interval_row);
            idx.add_input("appearance", &page_title, &advanced_title, &t("nlc-update-interval"), "nlc-update-interval", &update_interval_row);
        }

        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // --- Wire widget changes ---

        // Helper macro-like closures for SpinRow fields
        for (row, field_name) in [
            (&night_temp_row, "night_temp"),
            (&night_gamma_row, "night_gamma"),
            (&day_temp_row, "day_temp"),
            (&day_gamma_row, "day_gamma"),
            (&static_temp_row, "static_temp"),
            (&static_gamma_row, "static_gamma"),
            (&transition_duration_row, "transition_duration"),
            (&startup_duration_row, "startup_duration"),
            (&shutdown_duration_row, "shutdown_duration"),
            (&adaptive_interval_row, "adaptive_interval"),
            (&update_interval_row, "update_interval"),
        ] {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            let field = field_name.to_string();
            row.connect_changed(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let value = row.value().to_string();
                    cb(
                        urn.clone(),
                        "update_config".to_string(),
                        serde_json::json!({ "field": field, "value": value }),
                    );
                }
            });
        }

        // Wire EntryRow fields (with apply button)
        for (row, field_name) in [
            (&sunrise_row, "sunrise"),
            (&sunset_row, "sunset"),
            (&latitude_row, "latitude"),
            (&longitude_row, "longitude"),
        ] {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            let field = field_name.to_string();
            row.connect_apply(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let value = row.text().to_string();
                    cb(
                        urn.clone(),
                        "update_config".to_string(),
                        serde_json::json!({ "field": field, "value": value }),
                    );
                }
            });
        }

        // Wire transition mode ComboRow
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            mode_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let idx = row.selected() as usize;
                    let modes = ["geo", "static", "center", "finish_by", "start_at"];
                    if let Some(mode) = modes.get(idx) {
                        cb(
                            urn.clone(),
                            "update_config".to_string(),
                            serde_json::json!({ "field": "transition_mode", "value": *mode }),
                        );
                    }
                }
            });
        }

        // Wire backend ComboRow
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            backend_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let idx = row.selected() as usize;
                    let backends = ["auto", "hyprland", "wayland"];
                    if let Some(backend) = backends.get(idx) {
                        cb(
                            urn.clone(),
                            "update_config".to_string(),
                            serde_json::json!({ "field": "backend", "value": *backend }),
                        );
                    }
                }
            });
        }

        // Wire smoothing SwitchRow
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            smoothing_row.connect_active_notify(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let value = row.is_active().to_string();
                    cb(
                        urn.clone(),
                        "update_config".to_string(),
                        serde_json::json!({ "field": "smoothing", "value": value }),
                    );
                }
            });
        }

        // --- Reconciliation ---

        // Capture widget references for reconciliation
        let widgets = Rc::new(NightLightWidgets {
            night_temp_row,
            night_gamma_row,
            day_temp_row,
            day_gamma_row,
            static_temp_row,
            static_gamma_row,
            mode_row,
            sunrise_row,
            sunset_row,
            transition_duration_row,
            latitude_row,
            longitude_row,
            backend_row,
            smoothing_row,
            startup_duration_row,
            shutdown_duration_row,
            adaptive_interval_row,
            update_interval_row,
        });

        // Subscribe to entity updates
        {
            let store = entity_store.clone();
            let root_ref = root.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            let w = widgets.clone();

            entity_store.subscribe_type(NIGHT_LIGHT_CONFIG_ENTITY_TYPE, move || {
                let configs: Vec<(Urn, NightLightConfig)> =
                    store.get_entities_typed(NIGHT_LIGHT_CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    root_ref.set_visible(true);
                    reconcile(config, &w);
                    guard.set(false);
                } else {
                    root_ref.set_visible(false);
                }
            });
        }

        // Initial reconciliation with cached data
        {
            let store_clone = entity_store.clone();
            let root_ref = root.clone();
            let urn_ref = current_urn;
            let guard = updating;
            let w = widgets;

            gtk::glib::idle_add_local_once(move || {
                let configs: Vec<(Urn, NightLightConfig)> =
                    store_clone.get_entities_typed(NIGHT_LIGHT_CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    log::debug!(
                        "[night-light-config] Initial reconciliation with cached data"
                    );
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    root_ref.set_visible(true);
                    reconcile(config, &w);
                    guard.set(false);
                }
            });
        }

        Self { root }
    }
}

struct NightLightWidgets {
    night_temp_row: adw::SpinRow,
    night_gamma_row: adw::SpinRow,
    day_temp_row: adw::SpinRow,
    day_gamma_row: adw::SpinRow,
    static_temp_row: adw::SpinRow,
    static_gamma_row: adw::SpinRow,
    mode_row: adw::ComboRow,
    sunrise_row: adw::EntryRow,
    sunset_row: adw::EntryRow,
    transition_duration_row: adw::SpinRow,
    latitude_row: adw::EntryRow,
    longitude_row: adw::EntryRow,
    backend_row: adw::ComboRow,
    smoothing_row: adw::SwitchRow,
    startup_duration_row: adw::SpinRow,
    shutdown_duration_row: adw::SpinRow,
    adaptive_interval_row: adw::SpinRow,
    update_interval_row: adw::SpinRow,
}

fn reconcile_spin(row: &adw::SpinRow, value: &str, state: Option<&FieldState>) {
    if let Ok(v) = value.parse::<f64>() {
        row.set_value(v);
    }
    apply_field_state(row, state);
    let sub = subtitle_for_state(state);
    if !sub.is_empty() {
        row.set_subtitle(&sub);
    }
}

fn reconcile_entry(row: &adw::EntryRow, value: &str, state: Option<&FieldState>) {
    row.set_text(value);
    apply_field_state(row, state);
}

fn reconcile(config: &NightLightConfig, w: &NightLightWidgets) {
    // Colors
    reconcile_spin(
        &w.night_temp_row,
        &config.night_temp,
        config.field_state.get("night_temp"),
    );
    reconcile_spin(
        &w.night_gamma_row,
        &config.night_gamma,
        config.field_state.get("night_gamma"),
    );
    reconcile_spin(
        &w.day_temp_row,
        &config.day_temp,
        config.field_state.get("day_temp"),
    );
    reconcile_spin(
        &w.day_gamma_row,
        &config.day_gamma,
        config.field_state.get("day_gamma"),
    );
    reconcile_spin(
        &w.static_temp_row,
        &config.static_temp,
        config.field_state.get("static_temp"),
    );
    reconcile_spin(
        &w.static_gamma_row,
        &config.static_gamma,
        config.field_state.get("static_gamma"),
    );

    // Transition mode
    let mode_idx = match config.transition_mode.as_str() {
        "geo" => 0u32,
        "static" => 1,
        "center" => 2,
        "finish_by" => 3,
        "start_at" => 4,
        _ => 0,
    };
    w.mode_row.set_selected(mode_idx);
    apply_field_state(&w.mode_row, config.field_state.get("transition_mode"));

    // Timing
    reconcile_entry(
        &w.sunrise_row,
        &config.sunrise,
        config.field_state.get("sunrise"),
    );
    reconcile_entry(
        &w.sunset_row,
        &config.sunset,
        config.field_state.get("sunset"),
    );
    reconcile_spin(
        &w.transition_duration_row,
        &config.transition_duration,
        config.field_state.get("transition_duration"),
    );

    // Location
    reconcile_entry(
        &w.latitude_row,
        &config.latitude,
        config.field_state.get("latitude"),
    );
    reconcile_entry(
        &w.longitude_row,
        &config.longitude,
        config.field_state.get("longitude"),
    );

    // Backend
    let backend_idx = match config.backend.as_str() {
        "auto" => 0u32,
        "hyprland" => 1,
        "wayland" => 2,
        _ => 0,
    };
    w.backend_row.set_selected(backend_idx);
    apply_field_state(&w.backend_row, config.field_state.get("backend"));

    // Smoothing
    w.smoothing_row
        .set_active(config.smoothing == "true");
    apply_field_state(&w.smoothing_row, config.field_state.get("smoothing"));

    // Advanced durations
    reconcile_spin(
        &w.startup_duration_row,
        &config.startup_duration,
        config.field_state.get("startup_duration"),
    );
    reconcile_spin(
        &w.shutdown_duration_row,
        &config.shutdown_duration,
        config.field_state.get("shutdown_duration"),
    );
    reconcile_spin(
        &w.adaptive_interval_row,
        &config.adaptive_interval,
        config.field_state.get("adaptive_interval"),
    );
    reconcile_spin(
        &w.update_interval_row,
        &config.update_interval,
        config.field_state.get("update_interval"),
    );
}
