//! VPN operations: profile discovery, activation, deactivation, state refresh.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
#[cfg(test)]
use nmrs::models::DeviceState as NmDeviceState;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use crate::dbus_property::{NM_INTERFACE, NM_PATH, NM_SERVICE, NM_VPN_CONNECTION_INTERFACE};

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
fn nmrs_vpn_state_to_plugin_state(state: &NmDeviceState, active: bool) -> VpnState {
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
        NmDeviceState::Other(code) => VpnState::from_active_state(*code),
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
        let Ok(active) = zbus::Proxy::new(
            &conn,
            "org.freedesktop.NetworkManager",
            path.clone(),
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await
        else {
            continue;
        };

        let conn_type: String = match active.get_property("Type").await {
            Ok(value) => value,
            Err(error) => {
                log::debug!("[nm] Active connection {path:?} disappeared before Type: {error}");
                continue;
            }
        };
        if !is_vpn_type(&conn_type) {
            continue;
        }

        let uuid: String = match active.get_property("Uuid").await {
            Ok(value) => value,
            Err(error) => {
                log::debug!("[nm] VPN connection {path:?} disappeared before UUID: {error}");
                continue;
            }
        };
        let state_code: u32 = match active.get_property("State").await {
            Ok(value) => value,
            Err(error) => {
                log::debug!("[nm] VPN connection {uuid} disappeared before State: {error}");
                continue;
            }
        };

        // VPN.Connection.VpnState is more precise than the generic active
        // connection state, especially for VPN failures while the wrapper
        // remains activated.
        let vpn_state =
            match zbus::Proxy::new(&conn, NM_SERVICE, path.clone(), NM_VPN_CONNECTION_INTERFACE)
                .await
            {
                Ok(vpn) => vpn
                    .get_property::<u32>("VpnState")
                    .await
                    .ok()
                    .map(VpnState::from_vpn_state),
                Err(_) => None,
            };
        states.insert(
            uuid,
            vpn_state.unwrap_or_else(|| VpnState::from_active_state(state_code)),
        );
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
        let Ok(active) = zbus::Proxy::new(
            conn,
            NM_SERVICE,
            path.clone(),
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await
        else {
            continue;
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
    refresh_lock: &tokio::sync::Mutex<()>,
) -> Result<()> {
    // Signal handling and action reconciliation can overlap. Serialize the
    // read-modify-publish cycle so an older D-Bus snapshot cannot overwrite a
    // newer one after a rapid VPN transition.
    let _refresh_guard = refresh_lock.lock().await;

    for _ in 0..3 {
        let revision = {
            let st = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::warn!("[nm] Mutex poisoned, recovering: {e}");
                    e.into_inner()
                }
            };
            st.vpn_revision
        };
        let profiles = get_vpn_profiles(nm).await?;
        let active_vpns = get_active_vpn_connections(conn).await?;

        let new_connections = profiles
            .into_iter()
            .map(|profile| VpnConnectionInfo {
                state: active_vpns
                    .get(&profile.uuid)
                    .cloned()
                    .unwrap_or(VpnState::Disconnected),
                path: profile.path,
                uuid: profile.uuid,
                name: profile.name,
                conn_type: profile.conn_type,
                active_path: None,
            })
            .collect();

        let mut st = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[nm] Mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };
        if st.vpn_revision != revision {
            // An action changed the state while D-Bus reads were in flight.
            // Retry so the older snapshot cannot roll it back.
            continue;
        }
        st.vpn_connections = new_connections;
        st.vpn_revision = st.vpn_revision.wrapping_add(1);
        return Ok(());
    }

    log::debug!("[nm] VPN refresh superseded by a newer state change");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_nm_device_activated_to_connected() {
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(&NmDeviceState::Activated, true),
            VpnState::Connected
        );
    }

    #[test]
    fn maps_nmrs_active_connection_other_codes() {
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(&NmDeviceState::Other(1), true),
            VpnState::Connecting
        );
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(&NmDeviceState::Other(2), true),
            VpnState::Connected
        );
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(&NmDeviceState::Other(3), true),
            VpnState::Disconnecting
        );
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(&NmDeviceState::Other(4), false),
            VpnState::Disconnected
        );
    }

    #[test]
    fn active_flag_keeps_state_connected_when_nmrs_state_is_unhelpful() {
        assert_eq!(
            nmrs_vpn_state_to_plugin_state(&NmDeviceState::Disconnected, true),
            VpnState::Connected
        );
    }

    #[test]
    fn maps_vpn_connection_state_codes() {
        assert_eq!(VpnState::from_vpn_state(2), VpnState::Connecting);
        assert_eq!(VpnState::from_vpn_state(5), VpnState::Connected);
        assert_eq!(VpnState::from_vpn_state(6), VpnState::Disconnected);
        assert_eq!(VpnState::from_vpn_state(7), VpnState::Disconnected);
    }
}
