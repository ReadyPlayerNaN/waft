//! VPN operations: profile discovery, activation, deactivation, state refresh.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};
use zbus::Connection;

use crate::dbus_property::{
    get_property, NM_CONNECTION_ACTIVE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE,
    NM_SETTINGS_INTERFACE, NM_SETTINGS_PATH,
};
use crate::state::{NmState, VpnConnectionInfo, VpnState};

/// A saved VPN connection profile.
#[derive(Debug, Clone)]
pub struct VpnProfileInfo {
    pub path: String,
    pub uuid: String,
    pub name: String,
}

/// List all saved VPN connection profiles from NM Settings.
pub async fn get_vpn_profiles(conn: &Connection) -> Result<Vec<VpnProfileInfo>> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_INTERFACE)
        .await
        .context("Failed to create Settings proxy")?;

    let (settings_paths,): (Vec<OwnedObjectPath>,) = proxy
        .call("ListConnections", &())
        .await
        .context("Failed to list connections")?;

    let mut vpn_profiles = Vec::new();

    for settings_path in settings_paths {
        let path_str = settings_path.as_str();

        let conn_proxy = zbus::Proxy::new(
            conn,
            NM_SERVICE,
            path_str,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        let (settings,): (HashMap<String, HashMap<String, OwnedValue>>,) =
            conn_proxy.call("GetSettings", &()).await?;

        if let Some(connection) = settings.get("connection") {
            if let Some(conn_type) = connection.get("type") {
                if let Ok(type_str) = String::try_from(conn_type.clone()) {
                    if type_str == "vpn" {
                        let name = connection
                            .get("id")
                            .and_then(|v| String::try_from(v.clone()).ok())
                            .unwrap_or_else(|| "Unknown VPN".to_string());
                        let uuid = connection
                            .get("uuid")
                            .and_then(|v| String::try_from(v.clone()).ok())
                            .unwrap_or_default();

                        vpn_profiles.push(VpnProfileInfo {
                            path: path_str.to_string(),
                            uuid,
                            name,
                        });
                    }
                }
            }
        }
    }

    Ok(vpn_profiles)
}

/// Get active VPN connections: (active_path, connection_path, uuid, state_code).
pub async fn get_active_vpn_connections(
    conn: &Connection,
) -> Result<Vec<(String, String, String, u32)>> {
    let active_connections: Vec<OwnedObjectPath> = match get_property(
        conn,
        NM_PATH,
        NM_INTERFACE,
        "ActiveConnections",
    )
    .await
    {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };

    let mut vpn_active = Vec::new();

    for active_conn_path in active_connections {
        let path_str = active_conn_path.as_str();

        let conn_type: String =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "Type").await {
                Ok(t) => t,
                Err(_) => continue,
            };

        if conn_type != "vpn" {
            continue;
        }

        let connection_path: OwnedObjectPath = match get_property(
            conn,
            path_str,
            NM_CONNECTION_ACTIVE_INTERFACE,
            "Connection",
        )
        .await
        {
            Ok(p) => p,
            Err(_) => continue,
        };

        let uuid: String =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "Uuid").await {
                Ok(u) => u,
                Err(_) => continue,
            };

        let state: u32 =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "State").await {
                Ok(s) => s,
                Err(_) => 0,
            };

        vpn_active.push((
            path_str.to_string(),
            connection_path.to_string(),
            uuid,
            state,
        ));
    }

    Ok(vpn_active)
}

/// Activate a VPN connection.
pub async fn activate_vpn(conn: &Connection, connection_path: &str) -> Result<String> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let conn_obj = ObjectPath::try_from(connection_path)?;
    let device_obj = ObjectPath::try_from("/")?;
    let specific_obj = ObjectPath::try_from("/")?;

    let (active_conn_path,): (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &(conn_obj, device_obj, specific_obj))
        .await
        .context("Failed to activate VPN connection")?;

    Ok(active_conn_path.to_string())
}

/// Deactivate a VPN connection.
pub async fn deactivate_vpn(conn: &Connection, active_connection_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let active_obj = ObjectPath::try_from(active_connection_path)?;
    let _: () = proxy
        .call("DeactivateConnection", &(active_obj,))
        .await
        .context("Failed to deactivate VPN connection")?;

    Ok(())
}

/// Refresh VPN connection states from D-Bus.
pub async fn refresh_vpn_states(conn: &Connection, state: &Arc<StdMutex<NmState>>) -> Result<()> {
    let profiles = get_vpn_profiles(conn).await?;
    let active_vpns = get_active_vpn_connections(conn).await.unwrap_or_default();

    let mut new_connections = Vec::new();

    for profile in profiles {
        let active_info = active_vpns
            .iter()
            .find(|(_, _, uuid, _)| *uuid == profile.uuid);

        let vpn_state = active_info
            .map(|(_, _, _, state_code)| VpnState::from_active_state(*state_code))
            .unwrap_or(VpnState::Disconnected);

        let active_path = active_info.map(|(ap, _, _, _)| ap.clone());

        new_connections.push(VpnConnectionInfo {
            path: profile.path,
            uuid: profile.uuid,
            name: profile.name,
            state: vpn_state,
            active_path,
        });
    }

    let mut st = state.lock().unwrap();
    st.vpn_connections = new_connections;

    Ok(())
}
