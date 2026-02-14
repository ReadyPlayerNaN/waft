//! Ethernet connection profile discovery and management.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Connection;

use crate::dbus_property::{
    get_property, NM_CONNECTION_ACTIVE_INTERFACE, NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH,
    NM_SERVICE, NM_SETTINGS_INTERFACE, NM_SETTINGS_PATH,
};
use crate::state::{EthernetProfileInfo, NmState};

/// List all saved 802-3-ethernet connection profiles from NM Settings.
pub async fn get_ethernet_profiles(conn: &Connection) -> Result<Vec<EthernetProfileInfo>> {
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
                    && type_str == "802-3-ethernet" {
                        let name = connection
                            .get("id")
                            .and_then(|v| String::try_from(v.clone()).ok())
                            .unwrap_or_else(|| "Wired Connection".to_string());
                        let uuid = connection
                            .get("uuid")
                            .and_then(|v| String::try_from(v.clone()).ok())
                            .unwrap_or_default();

                        profiles.push(EthernetProfileInfo {
                            path: path_str.to_string(),
                            uuid,
                            name,
                        });
                    }
    }

    Ok(profiles)
}

/// Get the active connection UUID for an ethernet device.
pub async fn get_active_connection_uuid(
    conn: &Connection,
    device_path: &str,
) -> Result<Option<String>> {
    // Read ActiveConnection property from device
    let active_conn_path: OwnedObjectPath =
        match get_property(conn, device_path, NM_DEVICE_INTERFACE, "ActiveConnection").await {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

    let path_str = active_conn_path.as_str();
    if path_str == "/" {
        return Ok(None);
    }

    // Read UUID from ActiveConnection
    match get_property::<String>(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "Uuid").await {
        Ok(uuid) if !uuid.is_empty() => Ok(Some(uuid)),
        _ => Ok(None),
    }
}

/// Activate a specific ethernet connection profile on a device.
pub async fn activate_ethernet_connection(
    conn: &Connection,
    connection_path: &str,
    device_path: &str,
) -> Result<String> {
    use zbus::zvariant::ObjectPath;

    let conn_obj = ObjectPath::try_from(connection_path)?;
    let device_obj = ObjectPath::try_from(device_path)?;
    let no_specific = ObjectPath::from_static_str_unchecked("/");

    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let (active_conn_path,): (OwnedObjectPath,) = proxy
        .call(
            "ActivateConnection",
            &(&conn_obj, &device_obj, &no_specific),
        )
        .await
        .context("Failed to activate ethernet connection")?;

    Ok(active_conn_path.to_string())
}

/// Deactivate the active connection on a device.
pub async fn deactivate_ethernet_connection(
    conn: &Connection,
    device_path: &str,
) -> Result<()> {
    use zbus::zvariant::ObjectPath;

    // Get the active connection path
    let active_conn_path: OwnedObjectPath =
        get_property(conn, device_path, NM_DEVICE_INTERFACE, "ActiveConnection")
            .await
            .context("Device has no active connection")?;

    let path_str = active_conn_path.as_str();
    if path_str == "/" {
        return Ok(());
    }

    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let active_obj = ObjectPath::try_from(path_str)?;
    let _: () = proxy
        .call("DeactivateConnection", &(active_obj,))
        .await
        .context("Failed to deactivate connection")?;

    Ok(())
}

/// Refresh ethernet profiles and active connection state for all adapters.
pub async fn refresh_ethernet_state(conn: &Connection, state: &Arc<StdMutex<NmState>>) -> Result<()> {
    let profiles = get_ethernet_profiles(conn).await?;

    // Get device paths for active connection UUID lookup
    let adapter_paths: Vec<(String, String)> = {
        let st = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[nm] Mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };
        st.ethernet_adapters
            .iter()
            .map(|a| (a.path.clone(), a.interface_name.clone()))
            .collect()
    };

    for (device_path, _iface) in &adapter_paths {
        let active_uuid = get_active_connection_uuid(conn, device_path)
            .await
            .unwrap_or(None);

        let mut st = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[nm] Mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };
        if let Some(adapter) = st.ethernet_adapters.iter_mut().find(|a| a.path == *device_path) {
            adapter.active_connection_uuid = active_uuid;
            adapter.profiles = profiles.clone();
        }
    }

    debug!("[nm] Refreshed ethernet profiles: {} profiles", profiles.len());

    Ok(())
}
