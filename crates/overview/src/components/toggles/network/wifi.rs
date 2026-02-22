//! WiFi network menu rows for wireless adapters.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::icons::IconWidget;

use super::{NetworkRow, ToggleEntry};
use super::network_menu_logic::{details_text, should_be_expandable};

/// Update WiFi network menus for all wireless adapters based on current network entities.
pub(super) fn update_wifi_menus(
    entries: &Rc<RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    settings_available: &Rc<Cell<bool>>,
) {
    let networks: Vec<(Urn, entity::network::WiFiNetwork)> =
        store.get_entities_typed(entity::network::WIFI_NETWORK_ENTITY_TYPE);

    let entries_mut = entries.borrow();
    let has_settings = settings_available.get();

    for entry in entries_mut.iter() {
        // Only process wireless adapter toggles
        if !entry.urn_str.contains("/network-adapter/") {
            continue;
        }

        // Find networks for this adapter by checking URN prefix
        let adapter_urn_prefix = format!("{}/", entry.urn_str);
        let adapter_networks: Vec<_> = networks
            .iter()
            .filter(|(urn, _)| urn.as_str().starts_with(&adapter_urn_prefix))
            .collect();

        // Update toggle expandable state based on network count or settings availability
        let adapter_networks_owned: Vec<_> = adapter_networks
            .iter()
            .map(|(urn, net)| ((*urn).clone(), (*net).clone()))
            .collect();
        entry.toggle.set_expandable(should_be_expandable(
            adapter_networks_owned.len(),
            has_settings,
        ));

        // Update details text
        entry
            .toggle
            .set_details(details_text(&adapter_networks_owned));

        // Update network rows
        let mut network_rows = entry.network_rows.borrow_mut();

        // Remove rows for networks that no longer exist
        let current_network_urns: Vec<String> = adapter_networks
            .iter()
            .map(|(urn, _)| urn.as_str().to_string())
            .collect();
        network_rows.retain(|row| {
            if current_network_urns.iter().any(|u| u == row.urn_str()) {
                true
            } else {
                row.remove_from(entry.menu.root());
                false
            }
        });

        // Update or create rows for each network
        for (network_urn, network) in &adapter_networks {
            let network_urn_str = network_urn.as_str().to_string();

            if network_rows.iter().any(|r| r.urn_str() == network_urn_str) {
                // Network row already exists - no update needed for now
            } else {
                // Create new network row
                let row_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(12)
                    .css_classes(["menu-row", "clickable"])
                    .build();

                // Signal strength icon
                let icon_name = match network.strength {
                    s if s > 75 => "network-wireless-signal-excellent-symbolic",
                    s if s > 50 => "network-wireless-signal-good-symbolic",
                    s if s > 25 => "network-wireless-signal-ok-symbolic",
                    _ => "network-wireless-signal-weak-symbolic",
                };
                let icon_widget = IconWidget::from_name(icon_name, 24);
                row_box.append(icon_widget.widget());

                // SSID label
                let ssid_label = gtk::Label::builder()
                    .label(&network.ssid)
                    .hexpand(true)
                    .xalign(0.0)
                    .build();
                row_box.append(&ssid_label);

                // Security icon
                if network.secure {
                    let lock_icon = IconWidget::from_name("channel-secure-symbolic", 24);
                    row_box.append(lock_icon.widget());
                }

                // Connected indicator
                if network.connected {
                    let check_icon = IconWidget::from_name("object-select-symbolic", 24);
                    row_box.append(check_icon.widget());
                }

                // Make row clickable
                let gesture = gtk::GestureClick::new();
                let action_cb = action_callback.clone();
                let urn_for_click = network_urn.clone();
                let is_connected = network.connected;
                gesture.connect_released(move |_, _, _, _| {
                    let action = if is_connected {
                        "disconnect"
                    } else {
                        "connect"
                    };
                    action_cb(
                        urn_for_click.clone(),
                        action.to_string(),
                        serde_json::Value::Null,
                    );
                });
                row_box.add_controller(gesture);

                entry.menu.append(&row_box);

                network_rows.push(NetworkRow::Plain {
                    urn_str: network_urn_str,
                    root: row_box,
                });
            }
        }

        // Re-append settings button to keep it last in the menu
        if let Some(ref btn_container) = entry.settings_button {
            entry
                .menu
                .reorder_child_after(&btn_container.widget(), entry.menu.last_child().as_ref());
        }
    }
}
