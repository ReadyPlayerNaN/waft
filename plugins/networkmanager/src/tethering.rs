//! Bluetooth tethering connection profile discovery and state management.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

use crate::dbus_property::{
    NM_CONNECTION_ACTIVE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE, NM_SETTINGS_INTERFACE,
    NM_SETTINGS_PATH, get_property,
};
use crate::state::{NmState, TetheringConnectionState, TetheringProfileInfo};

/// List all saved bluetooth connection profiles from NM Settings.
pub async fn get_tethering_profiles(conn: &Connection) -> Result<Vec<TetheringProfileInfo>> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_INTERFACE)
        .await
        .context("Failed to create Settings proxy")?;

    let (settings_paths,): (Vec<OwnedObjectPath>,) = proxy
        .call("ListConnections", &())
        .await
        .context("Failed to list connections")?;

    let mut profiles = Vec::new();

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

        if let Some(connection) = settings.get("connection")
            && let Some(conn_type) = connection.get("type")
            && let Ok(type_str) = String::try_from(conn_type.clone())
            && type_str == "bluetooth"
        {
            let name = connection
                .get("id")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_else(|| "Bluetooth Tethering".to_string());
            let uuid = connection
                .get("uuid")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            let bdaddr = settings
                .get("bluetooth")
                .and_then(|bt| bt.get("bdaddr"))
                .and_then(|v| {
                    // Try string first, then byte array (MAC bytes)
                    if let Ok(s) = String::try_from(v.clone()) {
                        return Some(s);
                    }
                    if let Ok(bytes) = <Vec<u8>>::try_from(v.clone())
                        && bytes.len() == 6
                    {
                        return Some(format!(
                            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
                        ));
                    }
                    None
                });
            if bdaddr.is_none() {
                warn!(
                    "[nm] Tethering profile {path_str} missing bdaddr, \
                                 cannot match to BlueZ device",
                );
            }

            profiles.push(TetheringProfileInfo {
                path: path_str.to_string(),
                uuid,
                name,
                bdaddr,
            });
        }
    }

    Ok(profiles)
}

/// Get active bluetooth tethering connections: (active_path, uuid).
pub async fn get_active_tethering_connections(conn: &Connection) -> Result<Vec<(String, String)>> {
    let active_connections: Vec<OwnedObjectPath> =
        match get_property(conn, NM_PATH, NM_INTERFACE, "ActiveConnections").await {
            Ok(v) => v,
            Err(_) => return Ok(Vec::new()),
        };

    let mut tethering_active = Vec::new();

    for active_conn_path in active_connections {
        let path_str = active_conn_path.as_str();

        let conn_type: String =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "Type").await {
                Ok(t) => t,
                Err(_) => continue,
            };

        if conn_type != "bluetooth" {
            continue;
        }

        let uuid: String =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "Uuid").await {
                Ok(u) => u,
                Err(_) => continue,
            };

        tethering_active.push((path_str.to_string(), uuid));
    }

    Ok(tethering_active)
}

/// Activate a tethering connection by its settings path.
pub async fn activate_tethering(conn: &Connection, connection_path: &str) -> Result<String> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let conn_obj = ObjectPath::try_from(connection_path)?;
    let device_obj = ObjectPath::try_from("/")?;
    let specific_obj = ObjectPath::try_from("/")?;

    let (active_conn_path,): (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &(conn_obj, device_obj, specific_obj))
        .await
        .context("Failed to activate tethering connection")?;

    Ok(active_conn_path.to_string())
}

/// Deactivate a tethering connection by its active connection path.
pub async fn deactivate_tethering(conn: &Connection, active_connection_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let active_obj = ObjectPath::try_from(active_connection_path)?;
    let _: () = proxy
        .call("DeactivateConnection", &(active_obj,))
        .await
        .context("Failed to deactivate tethering connection")?;

    Ok(())
}

/// Refresh tethering connection states from D-Bus.
pub async fn refresh_tethering_states(
    conn: &Connection,
    state: &Arc<StdMutex<NmState>>,
) -> Result<()> {
    let profiles = get_tethering_profiles(conn).await?;
    let active = get_active_tethering_connections(conn)
        .await
        .unwrap_or_default();

    let new_connections: Vec<TetheringConnectionState> = profiles
        .into_iter()
        .map(|profile| {
            let active_info = active.iter().find(|(_, uuid)| *uuid == profile.uuid);

            TetheringConnectionState {
                path: profile.path,
                uuid: profile.uuid,
                name: profile.name,
                active: active_info.is_some(),
                active_path: active_info.map(|(ap, _)| ap.clone()),
                bdaddr: profile.bdaddr,
            }
        })
        .collect();

    let mut st = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[nm] Mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };
    st.tethering_connections = new_connections;

    debug!(
        "[nm] Refreshed tethering state: {} profiles",
        st.tethering_connections.len()
    );

    Ok(())
}
