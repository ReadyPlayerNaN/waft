//! Network adapter and VPN toggle components.
//!
//! Subscribes to `network-adapter`, `wifi-network`, `ethernet-connection`, and `vpn`
//! entity types. Dynamically creates FeatureToggleWidget per adapter/VPN with expandable
//! menus showing child networks/connections.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::menu_state::menu_id_for_widget;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};
use waft_ui_gtk::widgets::icon::IconWidget;

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// A tracked toggle entry for a network adapter or VPN.
struct ToggleEntry {
    urn_str: String,
    toggle: Rc<FeatureToggleWidget>,
    menu: gtk::Box,
    network_rows: RefCell<Vec<NetworkRow>>,
    weight: i32,
}

/// A single network row in the menu.
struct NetworkRow {
    urn_str: String,
    root: gtk::Box,
}

/// Dynamic set of toggles for network adapters and VPN connections.
///
/// Maintains one FeatureToggleWidget per network-adapter entity and one per
/// VPN entity. Subscribes to both entity types and keeps the toggle set
/// in sync as entities appear, change, or are removed.
pub struct NetworkManagerToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    store: Rc<EntityStore>,
    action_callback: EntityActionCallback,
    menu_store: Rc<waft_core::menu_state::MenuStore>,
}

impl NetworkManagerToggles {
    /// Create a new NetworkManagerToggles that subscribes to the entity store.
    ///
    /// `rebuild_callback` is invoked whenever the set of toggles changes
    /// (adapter/VPN added or removed) so the parent grid can rebuild.
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));

        // Subscribe to network adapter changes
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();

            store.subscribe_type(entity::network::ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, entity::network::NetworkAdapter)> =
                    store_ref.get_entities_typed(entity::network::ADAPTER_ENTITY_TYPE);

                let mut entries_mut = entries_ref.borrow_mut();
                let mut changed = false;

                // Current adapter URN strings
                let current_urns: Vec<String> = adapters
                    .iter()
                    .map(|(urn, _)| urn.as_str().to_string())
                    .collect();

                // Remove adapter toggles that no longer exist
                let before_len = entries_mut.len();
                entries_mut.retain(|entry| {
                    // Keep VPN entries (not our responsibility) and current adapter entries
                    !entry.urn_str.contains("/network-adapter/") || current_urns.contains(&entry.urn_str)
                });
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
                    } else {
                        // Create new toggle for this adapter
                        let widget_id = format!("network-toggle-{}", urn_str);
                        let menu_id = menu_id_for_widget(&widget_id);

                        // Create menu container for networks/connections
                        let menu = gtk::Box::builder()
                            .orientation(gtk::Orientation::Vertical)
                            .spacing(0)
                            .css_classes(["menu-content"])
                            .build();

                        let toggle = Rc::new(FeatureToggleWidget::new(
                            FeatureToggleProps {
                                active: adapter.connected,
                                busy: false,
                                details: None,
                                expandable: false,  // Will be updated based on child count
                                icon,
                                title,
                                menu_id: Some(menu_id.clone()),
                            },
                            Some(menu_store_ref.clone()),
                        ));

                        let action_cb = cb.clone();
                        let action_urn = urn.clone();
                        let adapter_kind = adapter.kind.clone();
                        toggle.connect_output(move |_output| {
                            let action = match adapter_kind {
                                entity::network::AdapterKind::Wireless => "activate",
                                entity::network::AdapterKind::Wired => "activate",
                            };
                            action_cb(
                                action_urn.clone(),
                                action.to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        entries_mut.push(ToggleEntry {
                            urn_str,
                            toggle,
                            menu,
                            network_rows: RefCell::new(Vec::new()),
                            weight: 150,
                        });
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    rebuild();
                }
            });
        }

        // Subscribe to WiFi network changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            store.subscribe_type(entity::network::WIFI_NETWORK_ENTITY_TYPE, move || {
                Self::update_wifi_menus(&entries_ref, &store_ref, &cb);
            });
        }

        // Subscribe to VPN changes
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();

            store.subscribe_type(entity::network::VPN_ENTITY_TYPE, move || {
                let vpns: Vec<(Urn, entity::network::Vpn)> =
                    store_ref.get_entities_typed(entity::network::VPN_ENTITY_TYPE);

                let mut entries_mut = entries_ref.borrow_mut();
                let mut changed = false;

                // Current VPN URN strings
                let current_urns: Vec<String> = vpns
                    .iter()
                    .map(|(urn, _)| urn.as_str().to_string())
                    .collect();

                // Remove VPN toggles that no longer exist
                let before_len = entries_mut.len();
                entries_mut.retain(|entry| {
                    // Keep adapter entries (not our responsibility) and current VPN entries
                    !entry.urn_str.contains("/vpn/") || current_urns.contains(&entry.urn_str)
                });
                if entries_mut.len() != before_len {
                    changed = true;
                }

                // Update existing or create new VPN toggles
                for (urn, vpn) in &vpns {
                    let urn_str = urn.as_str().to_string();
                    let active = matches!(
                        vpn.state,
                        entity::network::VpnState::Connected | entity::network::VpnState::Connecting
                    );
                    let busy = matches!(
                        vpn.state,
                        entity::network::VpnState::Connecting | entity::network::VpnState::Disconnecting
                    );

                    if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == urn_str) {
                        // Update existing toggle
                        entry.toggle.set_active(active);
                        entry.toggle.set_busy(busy);
                        entry.toggle.set_details(Some(vpn.name.clone()));
                    } else {
                        // Create new toggle for this VPN
                        let toggle = Rc::new(FeatureToggleWidget::new(
                            FeatureToggleProps {
                                active,
                                busy,
                                details: Some(vpn.name.clone()),
                                expandable: false,
                                icon: "network-vpn-symbolic".to_string(),
                                title: "VPN".to_string(),
                                menu_id: None,
                            },
                            None,
                        ));

                        let action_cb = cb.clone();
                        let action_urn = urn.clone();
                        toggle.connect_output(move |_output| {
                            action_cb(
                                action_urn.clone(),
                                "toggle".to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        // VPN toggles don't have menus (for now)
                        let menu = gtk::Box::builder()
                            .orientation(gtk::Orientation::Vertical)
                            .spacing(0)
                            .build();

                        entries_mut.push(ToggleEntry {
                            urn_str,
                            toggle,
                            menu,
                            network_rows: RefCell::new(Vec::new()),
                            weight: 160,
                        });
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    rebuild();
                }
            });
        }

        Self {
            entries,
            store: store.clone(),
            action_callback: action_callback.clone(),
            menu_store: menu_store.clone(),
        }
    }

    /// Update WiFi network menus for all wireless adapters based on current network entities.
    fn update_wifi_menus(
        entries: &Rc<RefCell<Vec<ToggleEntry>>>,
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
    ) {
        let networks: Vec<(Urn, entity::network::WiFiNetwork)> =
            store.get_entities_typed(entity::network::WIFI_NETWORK_ENTITY_TYPE);

        let entries_mut = entries.borrow();

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

            // Update toggle expandable state based on network count
            entry.toggle.set_expandable(!adapter_networks.is_empty());

            // Update details text
            if let Some((_, connected_network)) = adapter_networks.iter().find(|(_, n)| n.connected) {
                entry.toggle.set_details(Some(connected_network.ssid.clone()));
            } else if !adapter_networks.is_empty() {
                entry.toggle.set_details(Some(format!("{} networks", adapter_networks.len())));
            } else {
                entry.toggle.set_details(None);
            }

            // Update network rows
            let mut network_rows = entry.network_rows.borrow_mut();

            // Remove rows for networks that no longer exist
            let current_network_urns: Vec<String> = adapter_networks
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();
            network_rows.retain(|row| current_network_urns.contains(&row.urn_str));

            // Update or create rows for each network
            for (network_urn, network) in &adapter_networks {
                let network_urn_str = network_urn.as_str().to_string();

                if network_rows.iter().any(|r| r.urn_str == network_urn_str) {
                    // Network row already exists - no update needed for now
                    // TODO: Update signal strength icon if needed
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
                        let action = if is_connected { "disconnect" } else { "connect" };
                        action_cb(
                            urn_for_click.clone(),
                            action.to_string(),
                            serde_json::Value::Null,
                        );
                    });
                    row_box.add_controller(gesture);

                    entry.menu.append(&row_box);

                    network_rows.push(NetworkRow {
                        urn_str: network_urn_str,
                        root: row_box,
                    });
                }
            }
        }
    }

    /// Return all current toggles as feature toggle widgets for the grid.
    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        self.entries
            .borrow()
            .iter()
            .map(|entry| {
                Rc::new(WidgetFeatureToggle {
                    id: format!("network-toggle-{}", entry.urn_str),
                    weight: entry.weight,
                    el: entry.toggle.widget(),
                    menu: Some(entry.menu.clone().upcast::<gtk::Widget>()),
                    on_expand_toggled: None,
                    menu_id: entry.toggle.menu_id.clone(),
                })
            })
            .collect()
    }
}

/// Determine the icon for a network adapter based on its kind and state.
fn adapter_icon(adapter: &entity::network::NetworkAdapter) -> String {
    match &adapter.kind {
        entity::network::AdapterKind::Wired => {
            if adapter.connected {
                "network-wired-symbolic"
            } else {
                "network-wired-disconnected-symbolic"
            }
        }
        entity::network::AdapterKind::Wireless => {
            if adapter.connected {
                "network-wireless-signal-good-symbolic" // Will be updated by child network data
            } else {
                "network-wireless-offline-symbolic"
            }
        }
    }
    .to_string()
}

/// Determine the title for a network adapter based on its kind.
fn adapter_title(adapter: &entity::network::NetworkAdapter) -> String {
    match &adapter.kind {
        entity::network::AdapterKind::Wired => "Wired".to_string(),
        entity::network::AdapterKind::Wireless => "Wi-Fi".to_string(),
    }
}
