//! VPN toggle menu rows.

use std::rc::Rc;

use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::connection_row::{
    ConnectionRow, ConnectionRowOutput, ConnectionRowProps,
};

use waft_client::EntityActionCallback;

use super::{NetworkRow, ToggleEntry};

/// Update VPN menu rows inside the consolidated VPN toggle.
///
/// Uses ConnectionRow widgets with incremental updates instead of
/// full drain+recreate.
pub(super) fn update_vpn_menu_rows(
    entry: &ToggleEntry,
    vpns: &[(Urn, entity::network::Vpn)],
    action_callback: &EntityActionCallback,
) {
    let mut network_rows = entry.network_rows.borrow_mut();

    // Remove rows for VPNs that no longer exist
    let current_vpn_urns: Vec<String> = vpns
        .iter()
        .map(|(urn, _)| urn.as_str().to_string())
        .collect();
    network_rows.retain(|row| {
        if current_vpn_urns.iter().any(|u| u == row.urn_str()) {
            true
        } else {
            row.remove_from(&entry.menu.root());
            false
        }
    });

    // Update existing or create new rows
    for (vpn_urn, vpn) in vpns {
        let vpn_urn_str = vpn_urn.as_str().to_string();
        let active = vpn.state == entity::network::VpnState::Connected;
        let transitioning = matches!(
            vpn.state,
            entity::network::VpnState::Connecting | entity::network::VpnState::Disconnecting
        );

        if let Some(existing) = network_rows.iter().find(|r| r.urn_str() == vpn_urn_str) {
            // Update existing ConnectionRow
            if let NetworkRow::Connection { row, .. } = existing {
                row.update(&ConnectionRowProps {
                    name: vpn.name.clone(),
                    active,
                    transitioning,
                    icon: Some(vpn_icon_name(&vpn.vpn_type)),
                });
            }
        } else {
            // Create new ConnectionRow
            let conn_row = Rc::new(ConnectionRow::build(&ConnectionRowProps {
                name: vpn.name.clone(),
                active,
                transitioning,
                icon: Some(vpn_icon_name(&vpn.vpn_type)),
            }));

            let action_cb = action_callback.clone();
            let urn_for_click = vpn_urn.clone();
            let vpn_state = vpn.state;
            conn_row.connect_output(move |ConnectionRowOutput::Toggle| {
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

            entry.menu.append(&conn_row.widget());

            network_rows.push(NetworkRow::Connection {
                urn_str: vpn_urn_str,
                row: conn_row,
            });
        }
    }
}

/// Determine the icon name for a VPN connection based on its type.
fn vpn_icon_name(vpn_type: &entity::network::VpnType) -> String {
    match vpn_type {
        entity::network::VpnType::Wireguard => "network-vpn-symbolic".to_string(),
        entity::network::VpnType::Vpn => "network-vpn-symbolic".to_string(),
    }
}
