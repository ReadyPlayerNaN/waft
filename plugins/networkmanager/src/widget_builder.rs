//! Widget building: converts NmState into NamedWidget descriptors.

use waft_plugin_sdk::*;

use crate::state::{
    EthernetAdapterState, NmState, VpnConnectionInfo, VpnState, WiFiAdapterState,
};

/// Build all widgets for the current network state.
pub fn build_widgets(state: &NmState) -> Vec<NamedWidget> {
    let mut widgets = Vec::new();

    for adapter in &state.wifi_adapters {
        widgets.push(build_wifi_widget(adapter));
    }

    for adapter in &state.ethernet_adapters {
        widgets.push(build_wired_widget(adapter));
    }

    if !state.vpn_connections.is_empty() {
        widgets.push(build_vpn_widget(&state.vpn_connections));
    }

    widgets
}

fn build_wifi_widget(adapter: &WiFiAdapterState) -> NamedWidget {
    let connected = adapter.active_ssid.is_some();
    let signal_strength = if connected {
        adapter
            .access_points
            .iter()
            .find(|ap| Some(&ap.ssid) == adapter.active_ssid.as_ref())
            .map(|ap| ap.strength)
    } else {
        None
    };

    let icon = crate::get_wifi_icon(signal_strength, adapter.enabled, connected);

    let details = if !adapter.enabled {
        Some("Disabled".to_string())
    } else if let Some(ref ssid) = adapter.active_ssid {
        Some(ssid.clone())
    } else if !adapter.access_points.is_empty() {
        let count = adapter.access_points.len();
        Some(format!(
            "{} network{} available",
            count,
            if count == 1 { "" } else { "s" }
        ))
    } else {
        None
    };

    // Build network list as expanded content
    let expanded_content = if !adapter.access_points.is_empty() || connected {
        let mut container = ColBuilder::new().spacing(4);

        // Show available networks sorted by signal strength
        for ap in &adapter.access_points {
            let is_active = adapter.active_ssid.as_deref() == Some(&ap.ssid);
            let ap_icon = crate::get_wifi_icon(Some(ap.strength), true, true);

            let mut row = MenuRowBuilder::new(&ap.ssid).icon(ap_icon);

            if is_active {
                row = row.trailing(Widget::Checkmark { visible: true });
            }

            row = row.on_click(format!("connect_wifi:{}", ap.ssid));
            container = container.child(row.build());
        }

        // If connected, add disconnect option
        if connected {
            let disconnect_row = MenuRowBuilder::new("Disconnect")
                .icon("network-offline-symbolic")
                .on_click(format!("disconnect_wifi:{}", adapter.path))
                .build();
            container = container.child(disconnect_row);
        }

        Some(container.build())
    } else {
        None
    };

    let mut toggle = FeatureToggleBuilder::new(format!("Wi-Fi ({})", adapter.interface_name))
        .icon(icon)
        .active(adapter.enabled)
        .busy(adapter.busy)
        .on_toggle("toggle_wifi");

    if let Some(d) = &details {
        toggle = toggle.details(d);
    }

    if let Some(content) = expanded_content {
        toggle = toggle.expanded_content(content);
    } else {
        toggle = toggle.expandable(true);
    }

    NamedWidget {
        id: format!("networkmanager:wifi:{}", adapter.path),
        weight: 100,
        widget: toggle.build(),
    }
}

fn build_wired_widget(adapter: &EthernetAdapterState) -> NamedWidget {
    let icon = wired_icon(adapter);
    let details = wired_details(adapter);

    let toggle =
        FeatureToggleBuilder::new(format!("Wired ({})", adapter.interface_name))
            .icon(icon)
            .active(adapter.is_connected())
            .details(details)
            .on_toggle(format!("toggle_wired:{}", adapter.path));

    NamedWidget {
        id: format!("networkmanager:wired:{}", adapter.path),
        weight: 101,
        widget: toggle.build(),
    }
}

fn build_vpn_widget(vpn_connections: &[VpnConnectionInfo]) -> NamedWidget {
    let (connected_name, overall_state) = derive_vpn_state(vpn_connections);
    let any_active = overall_state != VpnState::Disconnected;

    let icon = if any_active {
        "network-vpn-symbolic"
    } else {
        "network-vpn-disconnected-symbolic"
    };

    let details = match &overall_state {
        VpnState::Connected => connected_name.clone(),
        VpnState::Connecting => Some("Connecting...".to_string()),
        VpnState::Disconnecting => Some("Disconnecting...".to_string()),
        VpnState::Disconnected => None,
    };

    // Build VPN connection list as expanded content
    let mut container = ColBuilder::new().spacing(4);

    for vpn in vpn_connections {
        let is_busy = matches!(vpn.state, VpnState::Connecting | VpnState::Disconnecting);
        let is_connected = vpn.state == VpnState::Connected;

        let action_id = if is_connected {
            format!("disconnect_vpn:{}", vpn.path)
        } else {
            format!("connect_vpn:{}", vpn.path)
        };

        let trailing = SwitchBuilder::new()
            .active(is_connected)
            .on_toggle(action_id)
            .build();

        let mut row = MenuRowBuilder::new(&vpn.name)
            .icon("network-vpn-symbolic")
            .trailing(trailing)
            .busy(is_busy);

        // Click action: toggle connection
        let click_action = if vpn.state == VpnState::Connected {
            format!("disconnect_vpn:{}", vpn.path)
        } else if vpn.state == VpnState::Disconnected {
            format!("connect_vpn:{}", vpn.path)
        } else {
            String::new()
        };

        if !click_action.is_empty() {
            row = row.on_click(click_action);
        }

        container = container.child(row.build());
    }

    let mut toggle = FeatureToggleBuilder::new("VPN")
        .icon(icon)
        .active(any_active)
        .on_toggle("toggle_vpn")
        .expanded_content(container.build());

    if let Some(d) = &details {
        toggle = toggle.details(d);
    }

    NamedWidget {
        id: "networkmanager:vpn".to_string(),
        weight: 103,
        widget: toggle.build(),
    }
}

fn wired_icon(state: &EthernetAdapterState) -> &'static str {
    if !state.is_enabled() {
        "network-wired-offline-symbolic"
    } else if state.is_connected() {
        "network-wired-symbolic"
    } else {
        "network-wired-disconnected-symbolic"
    }
}

fn wired_details(state: &EthernetAdapterState) -> &'static str {
    if !state.is_enabled() {
        "Disabled"
    } else if state.is_connected() {
        "Connected"
    } else {
        "Disconnected"
    }
}

fn derive_vpn_state(connections: &[VpnConnectionInfo]) -> (Option<String>, VpnState) {
    for conn in connections {
        match conn.state {
            VpnState::Connected => return (Some(conn.name.clone()), VpnState::Connected),
            VpnState::Connecting => return (Some(conn.name.clone()), VpnState::Connecting),
            VpnState::Disconnecting => {
                return (Some(conn.name.clone()), VpnState::Disconnecting)
            }
            VpnState::Disconnected => {}
        }
    }
    (None, VpnState::Disconnected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_plugin_sdk::Widget;

    fn make_ethernet_adapter(path: &str, interface_name: &str, device_state: u32) -> EthernetAdapterState {
        EthernetAdapterState {
            path: path.to_string(),
            interface_name: interface_name.to_string(),
            device_state,
        }
    }

    #[test]
    fn wired_widget_toggle_action_includes_device_path() {
        let adapter = make_ethernet_adapter("/org/freedesktop/NetworkManager/Devices/3", "enp0s31f6", 100);
        let named = build_wired_widget(&adapter);

        match &named.widget {
            Widget::FeatureToggle { on_toggle, .. } => {
                assert_eq!(
                    on_toggle.id,
                    "toggle_wired:/org/freedesktop/NetworkManager/Devices/3"
                );
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }

    #[test]
    fn wired_widget_not_expandable() {
        let adapter = make_ethernet_adapter("/org/freedesktop/NetworkManager/Devices/3", "enp0s31f6", 100);
        let named = build_wired_widget(&adapter);

        match &named.widget {
            Widget::FeatureToggle {
                expandable,
                expanded_content,
                ..
            } => {
                assert!(!expandable, "Wired widget should not be expandable");
                assert!(
                    expanded_content.is_none(),
                    "Wired widget should have no expanded content"
                );
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }

    #[test]
    fn wired_widget_connected_state() {
        let adapter = make_ethernet_adapter("/org/freedesktop/NetworkManager/Devices/3", "eth0", 100);
        let named = build_wired_widget(&adapter);

        match &named.widget {
            Widget::FeatureToggle {
                active,
                icon,
                details,
                ..
            } => {
                assert!(active, "Connected adapter should be active");
                assert_eq!(*icon, "network-wired-symbolic");
                assert_eq!(details.as_deref(), Some("Connected"));
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }

    #[test]
    fn wired_widget_disconnected_state() {
        // device_state 30 = disconnected but enabled
        let adapter = make_ethernet_adapter("/org/freedesktop/NetworkManager/Devices/3", "eth0", 30);
        let named = build_wired_widget(&adapter);

        match &named.widget {
            Widget::FeatureToggle {
                active,
                icon,
                details,
                ..
            } => {
                assert!(!active, "Disconnected adapter should not be active");
                assert_eq!(*icon, "network-wired-disconnected-symbolic");
                assert_eq!(details.as_deref(), Some("Disconnected"));
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }

    #[test]
    fn wired_widget_disabled_state() {
        // device_state 10 = unmanaged/disabled (below 20 threshold)
        let adapter = make_ethernet_adapter("/org/freedesktop/NetworkManager/Devices/3", "eth0", 10);
        let named = build_wired_widget(&adapter);

        match &named.widget {
            Widget::FeatureToggle {
                active,
                icon,
                details,
                ..
            } => {
                assert!(!active, "Disabled adapter should not be active");
                assert_eq!(*icon, "network-wired-offline-symbolic");
                assert_eq!(details.as_deref(), Some("Disabled"));
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }

    #[test]
    fn wired_widget_id_contains_device_path() {
        let adapter = make_ethernet_adapter("/org/freedesktop/NetworkManager/Devices/5", "enp0s31f6", 100);
        let named = build_wired_widget(&adapter);

        assert_eq!(named.id, "networkmanager:wired:/org/freedesktop/NetworkManager/Devices/5");
        assert_eq!(named.weight, 101);
    }

    #[test]
    fn wired_widget_title_includes_interface_name() {
        let adapter = make_ethernet_adapter("/dev/3", "enp0s31f6", 100);
        let named = build_wired_widget(&adapter);

        match &named.widget {
            Widget::FeatureToggle { title, .. } => {
                assert_eq!(*title, "Wired (enp0s31f6)");
            }
            other => panic!("Expected FeatureToggle, got {:?}", other),
        }
    }
}
