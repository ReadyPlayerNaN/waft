//! Wired (Ethernet) adapter toggles.
//!
//! Subscribes to `network-adapter` (filtered to `AdapterKind::Wired`) and
//! `ethernet-connection` entity types. Creates one FeatureToggleWidget per wired adapter.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::menu_state::{menu_id_for_widget, toggle_menu};
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

use super::{NetworkRow, ToggleEntry, adapter_icon, adapter_title};
use crate::components::toggles::settings_app_tracker::SettingsAppTracker;
use crate::layout::types::WidgetFeatureToggle;
use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;
use crate::ui::feature_toggles::menu_info_row::{
    FeatureToggleMenuInfoRow, FeatureToggleMenuInfoRowProps,
};
use crate::ui::feature_toggles::menu_settings::FeatureToggleMenuSettingsButtonProps;
use waft_ui_gtk::icons::IconWidget;

/// Dynamic set of toggles for wired network adapters.
pub struct WiredToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    #[allow(dead_code)]
    store: Rc<EntityStore>,
    #[allow(dead_code)]
    action_callback: EntityActionCallback,
    #[allow(dead_code)]
    menu_store: Rc<waft_core::menu_state::MenuStore>,
    #[allow(dead_code)]
    settings_tracker: Rc<SettingsAppTracker>,
}

impl WiredToggles {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let settings_available: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // Track waft-settings app availability for settings button visibility.
        let settings_tracker: Rc<SettingsAppTracker> = {
            let entries_ref = entries.clone();
            let settings_available_ref = settings_available.clone();
            Rc::new(SettingsAppTracker::new(store, move |is_available| {
                settings_available_ref.set(is_available);
                let entries = entries_ref.borrow();
                for entry in entries.iter() {
                    if let Some(ref btn) = entry.settings_button {
                        if let Some(ref label) = entry.settings_button_label {
                            btn.update(&FeatureToggleMenuSettingsButtonProps {
                                label: label.clone(),
                                visible: is_available,
                            });
                        }
                        let has_info = !entry.info_rows.borrow().is_empty();
                        let has_children = !entry.network_rows.borrow().is_empty();
                        entry
                            .toggle
                            .set_expandable(has_info || has_children || is_available);
                    }
                }
            }))
        };

        // Subscribe to network adapter changes (wired only)
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();
            let settings_available_ref = settings_available.clone();
            let settings_tracker_for_adapter = settings_tracker.clone();

            store.subscribe_type(entity::network::ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, entity::network::NetworkAdapter)> =
                    store_ref.get_entities_typed(entity::network::ADAPTER_ENTITY_TYPE);

                // Filter to wired adapters only
                let adapters: Vec<_> = adapters
                    .into_iter()
                    .filter(|(_, a)| matches!(a.kind, entity::network::AdapterKind::Wired))
                    .collect();

                let mut entries_mut = entries_ref.borrow_mut();
                let mut changed = false;

                // Current adapter URN strings
                let current_urns: Vec<String> = adapters
                    .iter()
                    .map(|(urn, _)| urn.as_str().to_string())
                    .collect();

                // Remove adapter toggles that no longer exist
                let before_len = entries_mut.len();
                entries_mut.retain(|entry| current_urns.contains(&entry.urn_str));
                if entries_mut.len() != before_len {
                    changed = true;
                }

                // Update existing or create new adapter toggles
                for (urn, adapter) in &adapters {
                    let urn_str = urn.as_str().to_string();
                    let icon = adapter_icon(adapter);
                    let title = adapter_title(adapter);

                    if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == urn_str) {
                        // Update existing toggle
                        entry.toggle.set_active(adapter.connected);
                        entry.toggle.set_busy(false);
                        entry.toggle.set_icon(&icon);
                        entry.connected.set(adapter.connected);

                        // Update IP info rows
                        update_wired_info_rows(entry, adapter, &settings_available_ref);
                    } else {
                        // Create new toggle for this adapter
                        let widget_id = format!("wired-toggle-{}", urn_str);
                        let menu_id = menu_id_for_widget(&widget_id);

                        let menu = FeatureToggleMenuWidget::new();
                        let connected = Rc::new(Cell::new(adapter.connected));

                        let toggle = Rc::new(FeatureToggleWidget::new(
                            FeatureToggleProps {
                                active: adapter.connected,
                                busy: false,
                                details: None,
                                expandable: false,
                                icon,
                                title,
                                menu_id: Some(menu_id.clone()),
                                expanded: false,
                            },
                            Some(menu_store_ref.clone()),
                        ));

                        let action_cb = cb.clone();
                        let action_urn = urn.clone();
                        let menu_id_for_expand = menu_id.clone();
                        let menu_store_for_expand = menu_store_ref.clone();
                        toggle.connect_output(move |output| {
                            match output {
                                FeatureToggleOutput::Activate | FeatureToggleOutput::Deactivate => {
                                    action_cb(
                                        action_urn.clone(),
                                        "activate".to_string(),
                                        serde_json::Value::Null,
                                    );
                                }
                                FeatureToggleOutput::ExpandToggle(_) => {
                                    toggle_menu(&menu_store_for_expand, &menu_id_for_expand);
                                }
                            }
                        });

                        // Create settings button
                        let has_settings = settings_tracker_for_adapter.is_available();
                        let label = crate::i18n::t("wired-settings-button");
                        let btn = settings_tracker_for_adapter.build_settings_button(
                            &cb,
                            "wired",
                            label.clone(),
                            has_settings,
                        );
                        menu.append(&btn.widget());

                        let entry = ToggleEntry {
                            urn_str,
                            toggle,
                            menu,
                            network_rows: RefCell::new(Vec::new()),
                            info_rows: RefCell::new(Vec::new()),
                            weight: 150,
                            connected,
                            settings_button: Some(btn),
                            settings_button_label: Some(label),
                        };

                        // Initialize IP info rows
                        update_wired_info_rows(&entry, adapter, &settings_available_ref);

                        entries_mut.push(entry);
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    rebuild();
                }
            });
        }

        // Subscribe to Ethernet connection profile changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            let settings_available_ref = settings_available.clone();
            store.subscribe_type(
                entity::network::ETHERNET_CONNECTION_ENTITY_TYPE,
                move || {
                    update_ethernet_menus(
                        &entries_ref,
                        &store_ref,
                        &cb,
                        &settings_available_ref,
                    );
                },
            );
        }

        Self {
            entries,
            store: store.clone(),
            action_callback: action_callback.clone(),
            menu_store: menu_store.clone(),
            settings_tracker,
        }
    }

    /// Return all current toggles as feature toggle widgets for the grid.
    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        self.entries
            .borrow()
            .iter()
            .map(|entry| {
                Rc::new(WidgetFeatureToggle {
                    id: format!("wired-toggle-{}", entry.urn_str),
                    weight: entry.weight,
                    toggle: (*entry.toggle).clone(),
                    menu: Some(entry.menu.widget().clone()),
                })
            })
            .collect()
    }
}

/// Update Ethernet connection profile menus for wired adapters.
fn update_ethernet_menus(
    entries: &Rc<RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    settings_available: &Rc<Cell<bool>>,
) {
    let connections: Vec<(Urn, entity::network::EthernetConnection)> =
        store.get_entities_typed(entity::network::ETHERNET_CONNECTION_ENTITY_TYPE);

    let entries_mut = entries.borrow();

    for entry in entries_mut.iter() {
        // Find connections for this adapter by checking URN prefix
        let adapter_urn_prefix = format!("{}/", entry.urn_str);
        let adapter_connections: Vec<_> = connections
            .iter()
            .filter(|(urn, _)| urn.as_str().starts_with(&adapter_urn_prefix))
            .collect();

        // Only show profile menu when 2+ profiles exist (1 profile = nothing to switch)
        let show_profiles = adapter_connections.len() >= 2;

        // Update network rows for profiles
        let mut network_rows = entry.network_rows.borrow_mut();

        if !show_profiles {
            // Remove any existing profile rows
            for row in network_rows.drain(..) {
                row.remove_from(entry.menu.root());
            }
            // Re-evaluate expandable: info rows or settings button may still warrant it
            let has_info = !entry.info_rows.borrow().is_empty();
            if !has_info && !settings_available.get() {
                entry.toggle.set_expandable(false);
            }
            continue;
        }

        // Update expandable - IP info rows, profiles, or settings button should show menu
        entry.toggle.set_expandable(true);

        // Remove rows for connections that no longer exist
        let current_conn_urns: Vec<String> = adapter_connections
            .iter()
            .map(|(urn, _)| urn.as_str().to_string())
            .collect();

        // Remove stale rows from both the menu widget and our tracking
        network_rows.retain(|row| {
            if current_conn_urns.iter().any(|u| u == row.urn_str()) {
                true
            } else {
                row.remove_from(entry.menu.root());
                false
            }
        });

        for (conn_urn, conn) in &adapter_connections {
            let conn_urn_str = conn_urn.as_str().to_string();

            // Remove stale row if it exists (always recreate to reflect fresh conn.active state)
            if let Some(existing) = network_rows.iter().find(|r| r.urn_str() == conn_urn_str) {
                existing.remove_from(entry.menu.root());
            }
            network_rows.retain(|r| r.urn_str() != conn_urn_str);

            // Create connection profile row
            let row_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .css_classes(["menu-row", "clickable"])
                .build();

            // Connection name label
            let name_label = gtk::Label::builder()
                .label(&conn.name)
                .hexpand(true)
                .xalign(0.0)
                .build();
            row_box.append(&name_label);

            // Active indicator (checkmark)
            if conn.active {
                let check_icon = IconWidget::from_name("object-select-symbolic", 24);
                row_box.append(check_icon.widget());
            }

            // Make row clickable
            let gesture = gtk::GestureClick::new();
            let action_cb = action_callback.clone();
            let urn_for_click = (*conn_urn).clone();
            let is_active = conn.active;
            gesture.connect_released(move |_, _, _, _| {
                let action = if is_active { "deactivate" } else { "activate" };
                action_cb(
                    urn_for_click.clone(),
                    action.to_string(),
                    serde_json::Value::Null,
                );
            });
            row_box.add_controller(gesture);

            entry.menu.append(&row_box);

            network_rows.push(NetworkRow::Plain {
                urn_str: conn_urn_str,
                root: row_box,
            });
        }

        // Re-append settings button to keep it last in the menu
        if let Some(ref btn_container) = entry.settings_button {
            entry
                .menu
                .reorder_child_after(&btn_container.widget(), entry.menu.last_child().as_ref());
        }
    }
}

/// Update IP info rows in a wired adapter's menu.
fn update_wired_info_rows(
    entry: &ToggleEntry,
    adapter: &entity::network::NetworkAdapter,
    settings_available: &Rc<Cell<bool>>,
) {
    let mut info_rows = entry.info_rows.borrow_mut();

    // Remove old info rows
    for row in info_rows.drain(..) {
        entry.menu.remove(&row.widget());
    }

    // Only show info when connected with IP data
    let ip = match &adapter.ip {
        Some(ip) if adapter.connected => ip,
        _ => {
            // Don't unconditionally set expandable to false -- ethernet profiles
            // or settings button may still require the menu to be expandable.
            let has_profiles = entry.network_rows.borrow().len() >= 2;
            if !has_profiles && !settings_available.get() {
                entry.toggle.set_expandable(false);
            }
            return;
        }
    };

    entry.toggle.set_expandable(true);

    // Local IP row
    let local_label = format!("{}/{}", ip.address, ip.prefix);
    let local_row = FeatureToggleMenuInfoRow::build(&FeatureToggleMenuInfoRowProps {
        label: "Local IP".to_string(),
        value: local_label,
    });
    entry.menu.append(&local_row.widget());
    info_rows.push(local_row);

    // Gateway row
    if let Some(ref gateway) = ip.gateway {
        let gw_row = FeatureToggleMenuInfoRow::build(&FeatureToggleMenuInfoRowProps {
            label: "Gateway".to_string(),
            value: gateway.clone(),
        });
        entry.menu.append(&gw_row.widget());
        info_rows.push(gw_row);
    }

    // Public IP row
    let public_text = adapter.public_ip.as_deref().unwrap_or("Unavailable");
    let public_row = FeatureToggleMenuInfoRow::build(&FeatureToggleMenuInfoRowProps {
        label: "Public IP".to_string(),
        value: public_text.to_string(),
    });
    entry.menu.append(&public_row.widget());
    info_rows.push(public_row);

    // Re-append settings button to keep it last in the menu
    if let Some(ref btn_container) = entry.settings_button {
        entry
            .menu
            .reorder_child_after(&btn_container.widget(), entry.menu.last_child().as_ref());
    }
}
