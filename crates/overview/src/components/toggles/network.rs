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
    info_rows: RefCell<Vec<gtk::Box>>,
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

                        // Update IP info rows for wired adapters
                        if matches!(adapter.kind, entity::network::AdapterKind::Wired) {
                            update_wired_info_rows(entry, adapter);
                        }
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

                        let entry = ToggleEntry {
                            urn_str,
                            toggle,
                            menu,
                            network_rows: RefCell::new(Vec::new()),
                            info_rows: RefCell::new(Vec::new()),
                            weight: 150,
                        };

                        // Initialize IP info rows for wired adapters
                        if matches!(adapter.kind, entity::network::AdapterKind::Wired) {
                            update_wired_info_rows(&entry, adapter);
                        }

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

        // Subscribe to WiFi network changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            store.subscribe_type(entity::network::WIFI_NETWORK_ENTITY_TYPE, move || {
                Self::update_wifi_menus(&entries_ref, &store_ref, &cb);
            });
        }

        // Subscribe to Ethernet connection profile changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            store.subscribe_type(
                entity::network::ETHERNET_CONNECTION_ENTITY_TYPE,
                move || {
                    Self::update_ethernet_menus(&entries_ref, &store_ref, &cb);
                },
            );
        }

        // Subscribe to VPN changes - single consolidated toggle
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();

            // Track VPN URNs + states for the click handler
            let vpn_states: Rc<RefCell<Vec<(Urn, entity::network::VpnState)>>> =
                Rc::new(RefCell::new(Vec::new()));

            store.subscribe_type(entity::network::VPN_ENTITY_TYPE, move || {
                let vpns: Vec<(Urn, entity::network::Vpn)> =
                    store_ref.get_entities_typed(entity::network::VPN_ENTITY_TYPE);

                // Update tracked VPN states
                {
                    let mut states = vpn_states.borrow_mut();
                    states.clear();
                    for (urn, vpn) in &vpns {
                        states.push((urn.clone(), vpn.state));
                    }
                }

                let mut entries_mut = entries_ref.borrow_mut();

                if vpns.is_empty() {
                    // Remove consolidated VPN toggle if no VPNs exist
                    let before_len = entries_mut.len();
                    entries_mut.retain(|entry| entry.urn_str != "vpn-consolidated");
                    if entries_mut.len() != before_len {
                        drop(entries_mut);
                        rebuild();
                    }
                    return;
                }

                // Compute consolidated state
                let any_active = vpns.iter().any(|(_urn, vpn)| {
                    matches!(
                        vpn.state,
                        entity::network::VpnState::Connected
                            | entity::network::VpnState::Connecting
                    )
                });
                let any_busy = vpns.iter().any(|(_urn, vpn)| {
                    matches!(
                        vpn.state,
                        entity::network::VpnState::Connecting
                            | entity::network::VpnState::Disconnecting
                    )
                });
                let details = vpns
                    .iter()
                    .find(|(_, vpn)| vpn.state == entity::network::VpnState::Connected)
                    .map(|(_, vpn)| vpn.name.clone())
                    .unwrap_or_else(|| "Disconnected".to_string());

                if let Some(entry) = entries_mut
                    .iter()
                    .find(|e| e.urn_str == "vpn-consolidated")
                {
                    // Update existing consolidated toggle
                    entry.toggle.set_active(any_active);
                    entry.toggle.set_busy(any_busy);
                    entry.toggle.set_details(Some(details));
                    entry.toggle.set_expandable(vpns.len() >= 2);

                    // Update VPN menu rows
                    Self::update_vpn_menu_rows(entry, &vpns, &cb);
                } else {
                    // Create consolidated VPN toggle
                    let widget_id = "network-toggle-vpn-consolidated";
                    let menu_id = menu_id_for_widget(widget_id);

                    let menu = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(0)
                        .css_classes(["menu-content"])
                        .build();

                    let toggle = Rc::new(FeatureToggleWidget::new(
                        FeatureToggleProps {
                            active: any_active,
                            busy: any_busy,
                            details: Some(details),
                            expandable: vpns.len() >= 2,
                            icon: "network-vpn-symbolic".to_string(),
                            title: "VPN".to_string(),
                            menu_id: Some(menu_id.clone()),
                        },
                        Some(menu_store_ref.clone()),
                    ));

                    // Toggle click: disconnect ALL connected VPNs
                    let action_cb = cb.clone();
                    let vpn_states_for_click = vpn_states.clone();
                    toggle.connect_output(move |_output| {
                        let states = vpn_states_for_click.borrow();
                        for (urn, state) in states.iter() {
                            if matches!(
                                state,
                                entity::network::VpnState::Connected
                                    | entity::network::VpnState::Connecting
                            ) {
                                action_cb(
                                    urn.clone(),
                                    "disconnect".to_string(),
                                    serde_json::Value::Null,
                                );
                            }
                        }
                    });

                    let entry = ToggleEntry {
                        urn_str: "vpn-consolidated".to_string(),
                        toggle,
                        menu,
                        network_rows: RefCell::new(Vec::new()),
                        info_rows: RefCell::new(Vec::new()),
                        weight: 160,
                    };

                    // Populate VPN menu rows
                    Self::update_vpn_menu_rows(&entry, &vpns, &cb);

                    entries_mut.push(entry);
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

    /// Update VPN menu rows inside the consolidated VPN toggle.
    fn update_vpn_menu_rows(
        entry: &ToggleEntry,
        vpns: &[(Urn, entity::network::Vpn)],
        action_callback: &EntityActionCallback,
    ) {
        let mut network_rows = entry.network_rows.borrow_mut();

        // Remove all existing rows and rebuild
        for row in network_rows.drain(..) {
            entry.menu.remove(&row.root);
        }

        for (vpn_urn, vpn) in vpns {
            let row_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .css_classes(["menu-row", "clickable"])
                .build();

            // VPN name label
            let name_label = gtk::Label::builder()
                .label(&vpn.name)
                .hexpand(true)
                .xalign(0.0)
                .build();
            row_box.append(&name_label);

            // Trailing indicator: spinner for transitional states, checkmark for connected
            match vpn.state {
                entity::network::VpnState::Connecting
                | entity::network::VpnState::Disconnecting => {
                    let spinner = gtk::Spinner::builder()
                        .spinning(true)
                        .width_request(24)
                        .height_request(24)
                        .build();
                    row_box.append(&spinner);
                }
                entity::network::VpnState::Connected => {
                    let check_icon = IconWidget::from_name("object-select-symbolic", 24);
                    row_box.append(check_icon.widget());
                }
                entity::network::VpnState::Disconnected => {}
            }

            // Make row clickable
            let gesture = gtk::GestureClick::new();
            let action_cb = action_callback.clone();
            let urn_for_click = vpn_urn.clone();
            let vpn_state = vpn.state;
            gesture.connect_released(move |_, _, _, _| {
                let action = match vpn_state {
                    entity::network::VpnState::Connected => "disconnect",
                    entity::network::VpnState::Disconnected => "connect",
                    // Don't send actions during transitions
                    _ => return,
                };
                action_cb(
                    urn_for_click.clone(),
                    action.to_string(),
                    serde_json::Value::Null,
                );
            });
            row_box.add_controller(gesture);

            entry.menu.append(&row_box);

            network_rows.push(NetworkRow {
                urn_str: vpn_urn.as_str().to_string(),
                root: row_box,
            });
        }
    }

    /// Update Ethernet connection profile menus for wired adapters.
    fn update_ethernet_menus(
        entries: &Rc<RefCell<Vec<ToggleEntry>>>,
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
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
                    entry.menu.remove(&row.root);
                }
                // Don't change expandable here - info rows may still warrant it
                return;
            }

            // Update expandable - IP info rows or profiles should show menu
            entry.toggle.set_expandable(true);

            // Remove rows for connections that no longer exist
            let current_conn_urns: Vec<String> = adapter_connections
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();

            // Remove stale rows from both the menu widget and our tracking
            network_rows.retain(|row| {
                if current_conn_urns.contains(&row.urn_str) {
                    true
                } else {
                    entry.menu.remove(&row.root);
                    false
                }
            });

            for (conn_urn, conn) in &adapter_connections {
                let conn_urn_str = conn_urn.as_str().to_string();

                if let Some(existing) = network_rows.iter().find(|r| r.urn_str == conn_urn_str) {
                    // Update existing row - rebuild checkmark state
                    // Remove old row and recreate (simple approach for state updates)
                    entry.menu.remove(&existing.root);
                    network_rows.retain(|r| r.urn_str != conn_urn_str);
                }

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

                network_rows.push(NetworkRow {
                    urn_str: conn_urn_str,
                    root: row_box,
                });
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
                    toggle: (*entry.toggle).clone(),
                    menu: Some(entry.menu.clone().upcast::<gtk::Widget>()),
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

/// Update IP info rows in a wired adapter's menu.
fn update_wired_info_rows(
    entry: &ToggleEntry,
    adapter: &entity::network::NetworkAdapter,
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
            entry.toggle.set_expandable(false);
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
    let public_text = adapter
        .public_ip
        .as_deref()
        .unwrap_or("Unavailable");
    let public_row = build_info_row("Public IP", public_text);
    entry.menu.append(&public_row);
    info_rows.push(public_row);
}

/// Build a non-interactive info label row for the menu.
fn build_info_row(label: &str, value: &str) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["menu-row"])
        .build();

    let label_widget = gtk::Label::builder()
        .label(label)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    row.append(&label_widget);

    let value_widget = gtk::Label::builder()
        .label(value)
        .hexpand(true)
        .xalign(1.0)
        .build();
    row.append(&value_widget);

    row
}
