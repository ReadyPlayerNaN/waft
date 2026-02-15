//! Dark mode automation settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `dark-mode-automation-config` entity type.
//! Provides schema-driven configuration UI with constraint-aware widgets.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::display::{
    DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE, DarkModeAutomationConfig, FieldState,
};

/// Smart container for dark mode automation settings.
pub struct DarkModeAutomationSection {
    pub root: adw::PreferencesGroup,
}

struct Widgets {
    latitude_row: adw::SpinRow,
    longitude_row: adw::SpinRow,
    auto_location_row: adw::SwitchRow,
    dbus_api_row: adw::SwitchRow,
    portal_api_row: adw::SwitchRow,
}

fn reconcile_field_spin(
    row: &adw::SpinRow,
    field_name: &str,
    value: Option<f64>,
    config: &DarkModeAutomationConfig,
) {
    if let Some(schema) = config.schema.fields.get(field_name) {
        if schema.available {
            row.set_visible(true);
            if let Some(v) = value {
                row.set_value(v);
            }
            row.set_sensitive(schema.state == FieldState::Editable);
            if let Some(help) = &schema.help_text {
                row.set_subtitle(help);
            }
        } else {
            row.set_visible(false);
        }
    }
}

fn reconcile_field_switch(
    row: &adw::SwitchRow,
    field_name: &str,
    value: Option<bool>,
    config: &DarkModeAutomationConfig,
) {
    if let Some(schema) = config.schema.fields.get(field_name) {
        if schema.available {
            row.set_visible(true);
            if let Some(v) = value {
                row.set_active(v);
            }
            row.set_sensitive(schema.state == FieldState::Editable);
            if let Some(help) = &schema.help_text {
                row.set_subtitle(help);
            }
        } else {
            row.set_visible(false);
        }
    }
}

fn reconcile(config: &DarkModeAutomationConfig, widgets: &Widgets) {
    reconcile_field_spin(&widgets.latitude_row, "latitude", config.latitude, config);
    reconcile_field_spin(
        &widgets.longitude_row,
        "longitude",
        config.longitude,
        config,
    );
    reconcile_field_switch(
        &widgets.auto_location_row,
        "auto_location",
        config.auto_location,
        config,
    );
    reconcile_field_switch(&widgets.dbus_api_row, "dbus_api", config.dbus_api, config);
    reconcile_field_switch(
        &widgets.portal_api_row,
        "portal_api",
        config.portal_api,
        config,
    );
}

impl DarkModeAutomationSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Dark Mode Automation")
            .visible(false)
            .build();

        let latitude_row = adw::SpinRow::builder()
            .title("Latitude")
            .adjustment(&gtk::Adjustment::new(0.0, -90.0, 90.0, 0.01, 1.0, 0.0))
            .digits(2)
            .visible(false)
            .build();
        group.add(&latitude_row);

        let longitude_row = adw::SpinRow::builder()
            .title("Longitude")
            .adjustment(&gtk::Adjustment::new(0.0, -180.0, 180.0, 0.01, 1.0, 0.0))
            .digits(2)
            .visible(false)
            .build();
        group.add(&longitude_row);

        let auto_location_row = adw::SwitchRow::builder()
            .title("Auto-detect location")
            .visible(false)
            .build();
        group.add(&auto_location_row);

        let dbus_api_row = adw::SwitchRow::builder()
            .title("Enable D-Bus API")
            .visible(false)
            .build();
        group.add(&dbus_api_row);

        let portal_api_row = adw::SwitchRow::builder()
            .title("Enable XDG Portal")
            .visible(false)
            .build();
        group.add(&portal_api_row);

        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Wire spin row changes
        for (row, field_name) in [
            (&latitude_row, "latitude"),
            (&longitude_row, "longitude"),
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
                    cb(
                        urn.clone(),
                        "update_field".to_string(),
                        serde_json::json!({ "field": field, "value": row.value() }),
                    );
                }
            });
        }

        // Wire switch row changes
        for (row, field_name) in [
            (&auto_location_row, "auto_location"),
            (&dbus_api_row, "dbus_api"),
            (&portal_api_row, "portal_api"),
        ] {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            let field = field_name.to_string();
            row.connect_active_notify(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    cb(
                        urn.clone(),
                        "update_field".to_string(),
                        serde_json::json!({ "field": field, "value": row.is_active() }),
                    );
                }
            });
        }

        let widgets = Rc::new(Widgets {
            latitude_row,
            longitude_row,
            auto_location_row,
            dbus_api_row,
            portal_api_row,
        });

        // Subscribe to entity updates
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            let w = widgets.clone();

            entity_store.subscribe_type(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE, move || {
                let configs: Vec<(Urn, DarkModeAutomationConfig)> =
                    store.get_entities_typed(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    reconcile(config, &w);
                    guard.set(false);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        // Initial reconciliation with cached data
        {
            let store_clone = entity_store.clone();
            let group_ref = group.clone();
            let urn_ref = current_urn;
            let guard = updating;
            let w = widgets;

            gtk::glib::idle_add_local_once(move || {
                let configs: Vec<(Urn, DarkModeAutomationConfig)> =
                    store_clone.get_entities_typed(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    log::debug!(
                        "[dark-mode-automation] Initial reconciliation with cached data"
                    );
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    reconcile(config, &w);
                    guard.set(false);
                }
            });
        }

        Self { root: group }
    }
}
