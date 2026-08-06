//! VPN operations: profile discovery, activation, deactivation, state refresh.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use nmrs::models::DeviceState as NmDeviceState;
use zbus::Connection;

use crate::state::{NmState, VpnConnectionInfo, VpnState};

/// Returns true if the NM connection type should be treated as a VPN.
pub fn is_vpn_type(conn_type: &str) -> bool {
    conn_type == "vpn" || conn_type == "wireguard"
}

/// A saved VPN connection profile.
#[derive(Debug, Clone)]
pub struct VpnProfileInfo {
    pub path: String,
    pub uuid: String,
    pub name: String,
    /// NM connection type: "vpn" or "wireguard".
    pub conn_type: String,
}

/// List all saved VPN connection profiles from NetworkManager.
pub async fn get_vpn_profiles(nm: &nmrs::NetworkManager) -> Result<Vec<VpnProfileInfo>> {
    let saved = nm.list_saved_connections().await?;
    Ok(saved
        .into_iter()
        .filter(|conn| is_vpn_type(&conn.connection_type))
        .map(|conn| VpnProfileInfo {
            path: conn.path.to_string(),
            uuid: conn.uuid,
            name: conn.id,
            conn_type: conn.connection_type,
        })
        .collect())
}

fn nmrs_vpn_state_to_plugin_state(state: NmDeviceState, active: bool) -> VpnState {
    match state {
        NmDeviceState::Prepare
        | NmDeviceState::Config
        | NmDeviceState::NeedAuth
        | NmDeviceState::IpConfig
        | NmDeviceState::IpCheck
        | NmDeviceState::Secondaries => VpnState::Connecting,
        NmDeviceState::Activated => VpnState::Connected,
        NmDeviceState::Deactivating => VpnState::Disconnecting,
        // nmrs currently exposes NM ActiveConnection.State through the DeviceState field
        // for VPNs, so active connections often arrive as Other(1..=4) instead of the
        // usual device-state domain. Interpret those codes using the plugin's
        // ActiveConnection-state mapping to avoid getting stuck at Disconnected.
        NmDeviceState::Other(code) => VpnState::from_active_state(code),
        _ if active => VpnState::Connected,
        _ => VpnState::Disconnected,
    }
}

/// Get active VPN states from NetworkManager keyed by UUID.
pub async fn get_active_vpn_connections(
    nm: &nmrs::NetworkManager,
) -> Result<HashMap<String, VpnState>> {
    Ok(nm
        .list_vpn_connections()
        .await?
        .into_iter()
        .map(|vpn| {
            let state = nmrs_vpn_state_to_plugin_state(vpn.state, vpn.active);
            (vpn.uuid, state)
        })
        .collect())
}

/// Refresh VPN connection states from NetworkManager.
pub async fn refresh_vpn_states(
    _conn: &Connection,
    nm: &nmrs::NetworkManager,
    state: &Arc<StdMutex<NmState>>,
) -> Result<()> {
    let profiles = get_vpn_profiles(nm).await?;
    let active_vpns = get_active_vpn_connections(nm).await.unwrap_or_default();

    let mut new_connections = Vec::new();

    for profile in profiles {
        let vpn_state = active_vpns
            .get(&profile.uuid)
            .cloned()
            .unwrap_or(VpnState::Disconnected);

        new_connections.push(VpnConnectionInfo {
            path: profile.path,
            uuid: profile.uuid,
            name: profile.name,
            conn_type: profile.conn_type,
            state: vpn_state.clone(),
            active_path: None,
        });
    }

    let mut st = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("[nm] Mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };
    st.vpn_connections = new_connections;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_nm_device_activated_to_connected() {
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(NmDeviceState::Activated, true),
            VpnState::Connected
        );
    }

    #[test]
    fn maps_nmrs_active_connection_other_codes() {
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(NmDeviceState::Other(1), true),
            VpnState::Connecting
        );
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(NmDeviceState::Other(2), true),
            VpnState::Connected
        );
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(NmDeviceState::Other(3), true),
            VpnState::Disconnecting
        );
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(NmDeviceState::Other(4), false),
            VpnState::Disconnected
        );
    }

    #[test]
    fn active_flag_keeps_state_connected_when_nmrs_state_is_unhelpful() {
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(NmDeviceState::Disconnected, true),
            VpnState::Connected
        );
    }
}
