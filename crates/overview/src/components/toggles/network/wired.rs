//! Wired/Ethernet connection profile and IP info rows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::icons::IconWidget;

use super::{NetworkRow, ToggleEntry};
use super::build_info_row;

/// Update Ethernet connection profile menus for wired adapters.
pub(super) fn update_ethernet_menus(
    entries: &Rc<RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    settings_available: &Rc<Cell<bool>>,
) {
    let connections: Vec<(Urn, entity::network::EthernetConnection)> =
        store.get_entities_typed(entity::network::ETHERNET_CONNECTION_ENTITY_TYPE);

    let entries_mut = entries.borrow();

    for entry in entries_mut.iter() {
        // Only process wired adapter toggles
        if !entry.urn_str.contains("/network-adapter/") {
            continue;
        }

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
                row.remove_from(&entry.menu.root());
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
                row.remove_from(&entry.menu.root());
                false
            }
        });

        for (conn_urn, conn) in &adapter_connections {
            let conn_urn_str = conn_urn.as_str().to_string();

            // Remove stale row if it exists (always recreate to reflect fresh conn.active state)
            if let Some(existing) = network_rows.iter().find(|r| r.urn_str() == conn_urn_str) {
                existing.remove_from(&entry.menu.root());
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
pub(super) fn update_wired_info_rows(
    entry: &ToggleEntry,
    adapter: &entity::network::NetworkAdapter,
    settings_available: &Rc<Cell<bool>>,
) {
    let mut info_rows = entry.info_rows.borrow_mut();

    // Remove old info rows
    for row in info_rows.drain(..) {
        entry.menu.remove(&row);
    }

    // Only show info when connected with IP data
    let ip = match &adapter.ip {
        Some(ip) if adapter.connected => ip,
        _ => {
            // Don't unconditionally set expandable to false — ethernet profiles
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
    let local_row = build_info_row("Local IP", &local_label);
    entry.menu.append(&local_row);
    info_rows.push(local_row);

    // Gateway row
    if let Some(ref gateway) = ip.gateway {
        let gw_row = build_info_row("Gateway", gateway);
        entry.menu.append(&gw_row);
        info_rows.push(gw_row);
    }

    // Public IP row
    let public_text = adapter.public_ip.as_deref().unwrap_or("Unavailable");
    let public_row = build_info_row("Public IP", public_text);
    entry.menu.append(&public_row);
    info_rows.push(public_row);

    // Re-append settings button to keep it last in the menu
    if let Some(ref btn_container) = entry.settings_button {
        entry
            .menu
            .reorder_child_after(&btn_container.widget(), entry.menu.last_child().as_ref());
    }
}
