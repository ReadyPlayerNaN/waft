//! VPN operations: profile discovery, activation, deactivation, state refresh.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
#[cfg(test)]
use nmrs::models::DeviceState as NmDeviceState;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use crate::dbus_property::{NM_INTERFACE, NM_PATH, NM_SERVICE};

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

#[cfg(test)]
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
pub async fn get_active_vpn_connections(_conn: &Connection) -> Result<HashMap<String, VpnState>> {
    // Use a fresh system-bus connection here. Reusing the long-lived plugin
    // connection has proven too stale around rapid VPN transitions.
    let conn = Connection::system().await?;
    let nm = zbus::Proxy::new(
        &conn,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await?;

    let active_paths: Vec<OwnedObjectPath> = nm.get_property("ActiveConnections").await?;
    let mut states = HashMap::new();

    for path in active_paths {
        let active = match zbus::Proxy::new(
            &conn,
            "org.freedesktop.NetworkManager",
            path,
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await
        {
            Ok(proxy) => proxy,
            Err(_) => continue,
        };

        let conn_type: String = match active.get_property("Type").await {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !is_vpn_type(&conn_type) {
            continue;
        }

        let uuid: String = match active.get_property("Uuid").await {
            Ok(value) => value,
            Err(_) => continue,
        };
        let state_code: u32 = active.get_property("State").await.unwrap_or(0);
        states.insert(uuid, VpnState::from_active_state(state_code));
    }

    Ok(states)
}

pub async fn activate_vpn_by_uuid(conn: &Connection, uuid: &str) -> Result<()> {
    let settings = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await
    .context("Failed to create NM settings proxy")?;

    let (conn_path,): (OwnedObjectPath,) = settings
        .call("GetConnectionByUuid", &(uuid,))
        .await
        .with_context(|| format!("failed to look up VPN connection {uuid}"))?;

    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let conn_obj = ObjectPath::try_from(conn_path.as_str())?;
    let no_device = ObjectPath::from_static_str_unchecked("/");
    let no_specific = ObjectPath::from_static_str_unchecked("/");
    let _: (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &(&conn_obj, &no_device, &no_specific))
        .await
        .context("Failed to activate VPN connection")?;

    Ok(())
}

pub async fn deactivate_vpn_by_uuid(conn: &Connection, uuid: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let active_paths: Vec<OwnedObjectPath> = proxy
        .get_property("ActiveConnections")
        .await
        .context("Failed to read ActiveConnections")?;

    for path in active_paths {
        let active = match zbus::Proxy::new(
            conn,
            NM_SERVICE,
            path.clone(),
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await
        {
            Ok(proxy) => proxy,
            Err(_) => continue,
        };

        let active_uuid: String = match active.get_property("Uuid").await {
            Ok(value) => value,
            Err(_) => continue,
        };
        if active_uuid != uuid {
            continue;
        }

        let active_obj = ObjectPath::try_from(path.as_str())?;
        let _: () = proxy
            .call("DeactivateConnection", &(active_obj,))
            .await
            .context("Failed to deactivate VPN connection")?;
        return Ok(());
    }

    Ok(())
}

/// Refresh VPN connection states from NetworkManager.
pub async fn refresh_vpn_states(
    conn: &Connection,
    nm: &nmrs::NetworkManager,
    state: &Arc<StdMutex<NmState>>,
) -> Result<()> {
    let profiles = get_vpn_profiles(nm).await?;
    let active_vpns = get_active_vpn_connections(conn).await.unwrap_or_default();

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
