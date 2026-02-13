//! WiFi operations: scanning, connecting, disconnecting.

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Connection;

use crate::dbus_property::{
    NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE, NM_SETTINGS_INTERFACE,
    NM_SETTINGS_PATH, NM_WIRELESS_INTERFACE,
};
use crate::state::AccessPointInfo;
use crate::AccessPoint;

/// Scan WiFi networks and list known ones via D-Bus.
///
/// Calls `RequestScan` on each adapter, waits for results, then reads access points
/// via `GetAllAccessPoints`. Only returns networks with saved connection profiles.
pub async fn scan_and_list_known_networks(
    conn: &Connection,
    adapter_paths: &[String],
) -> Result<Vec<AccessPointInfo>> {
    // Trigger scan on each WiFi adapter
    for adapter_path in adapter_paths {
        let proxy = match zbus::Proxy::new(
            conn,
            NM_SERVICE,
            adapter_path.as_str(),
            NM_WIRELESS_INTERFACE,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "[nm] Failed to create Wireless proxy for {}: {}",
                    adapter_path, e
                );
                continue;
            }
        };

        let options: HashMap<String, zbus::zvariant::Value<'_>> = HashMap::new();
        let scan_result: Result<(), _> = proxy.call("RequestScan", &(options,)).await;
        if let Err(e) = scan_result {
            warn!("[nm] Failed to trigger scan on {}: {}", adapter_path, e);
        }
    }

    // Wait for scan results
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Collect access points from all adapters
    let mut by_ssid: HashMap<String, AccessPointInfo> = HashMap::new();

    for adapter_path in adapter_paths {
        let proxy = match zbus::Proxy::new(
            conn,
            NM_SERVICE,
            adapter_path.as_str(),
            NM_WIRELESS_INTERFACE,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "[nm] Failed to create Wireless proxy for {}: {}",
                    adapter_path, e
                );
                continue;
            }
        };

        let ap_paths: (Vec<OwnedObjectPath>,) =
            match proxy.call("GetAllAccessPoints", &()).await {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        "[nm] Failed to get access points from {}: {}",
                        adapter_path, e
                    );
                    continue;
                }
            };

        for ap_path in &ap_paths.0 {
            let ap = match read_access_point(conn, ap_path.as_str()).await {
                Ok(ap) => ap,
                Err(e) => {
                    debug!("[nm] Failed to read AP {}: {}", ap_path, e);
                    continue;
                }
            };

            if ap.ssid.is_empty() {
                continue;
            }

            // Only include networks with saved connection profiles
            match get_connections_for_ssid(conn, &ap.ssid).await {
                Ok(connections) if !connections.is_empty() => {}
                _ => {
                    debug!("[nm] Skipping network {} (no saved profile)", ap.ssid);
                    continue;
                }
            }

            let secure = ap.is_secure();

            match by_ssid.get(&ap.ssid) {
                Some(existing) if existing.strength >= ap.strength => {}
                _ => {
                    by_ssid.insert(
                        ap.ssid.clone(),
                        AccessPointInfo {
                            ssid: ap.ssid,
                            strength: ap.strength,
                            secure,
                        },
                    );
                }
            }
        }
    }

    let mut result: Vec<AccessPointInfo> = by_ssid.into_values().collect();
    result.sort_by(|a, b| b.strength.cmp(&a.strength));
    Ok(result)
}

/// Read access point properties from D-Bus.
async fn read_access_point(conn: &Connection, ap_path: &str) -> Result<AccessPoint> {
    use crate::dbus_property::get_property;

    let ssid_bytes: Vec<u8> = get_property(
        conn,
        ap_path,
        "org.freedesktop.NetworkManager.AccessPoint",
        "Ssid",
    )
    .await
    .unwrap_or_default();

    let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();

    let strength: u8 = get_property(
        conn,
        ap_path,
        "org.freedesktop.NetworkManager.AccessPoint",
        "Strength",
    )
    .await
    .unwrap_or(0);

    let flags: u32 = get_property(
        conn,
        ap_path,
        "org.freedesktop.NetworkManager.AccessPoint",
        "Flags",
    )
    .await
    .unwrap_or(0);

    let wpa_flags: u32 = get_property(
        conn,
        ap_path,
        "org.freedesktop.NetworkManager.AccessPoint",
        "WpaFlags",
    )
    .await
    .unwrap_or(0);

    let rsn_flags: u32 = get_property(
        conn,
        ap_path,
        "org.freedesktop.NetworkManager.AccessPoint",
        "RsnFlags",
    )
    .await
    .unwrap_or(0);

    Ok(AccessPoint {
        path: ap_path.to_string(),
        ssid,
        strength,
        flags,
        wpa_flags,
        rsn_flags,
    })
}

/// Find saved WiFi connections matching the given SSID.
pub async fn get_connections_for_ssid(conn: &Connection, ssid: &str) -> Result<Vec<String>> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_INTERFACE)
        .await
        .context("Failed to create Settings proxy")?;

    let (settings_paths,): (Vec<OwnedObjectPath>,) = proxy
        .call("ListConnections", &())
        .await
        .context("Failed to list connections")?;

    let mut matching = Vec::new();

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

        if let Some(wireless) = settings.get("802-11-wireless") {
            if let Some(ssid_value) = wireless.get("ssid") {
                if let Ok(ssid_bytes) = <Vec<u8>>::try_from(ssid_value.clone()) {
                    let connection_ssid = String::from_utf8_lossy(&ssid_bytes);
                    if connection_ssid == ssid {
                        matching.push(path_str.to_string());
                    }
                }
            }
        }
    }

    Ok(matching)
}

/// Activate a connection on a device.
pub async fn activate_connection(
    conn: &Connection,
    connection_path: Option<&str>,
    device_path: &str,
    specific_object: Option<&str>,
) -> Result<String> {
    let conn_path = connection_path.unwrap_or("/");
    let specific = specific_object.unwrap_or("/");

    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let (active_conn_path,): (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &(conn_path, device_path, specific))
        .await
        .context("Failed to activate connection")?;

    Ok(active_conn_path.to_string())
}

/// Disconnect a specific device.
pub async fn disconnect_device(conn: &Connection, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, device_path, NM_DEVICE_INTERFACE)
        .await
        .context("Failed to create Device proxy")?;

    let _: () = proxy
        .call("Disconnect", &())
        .await
        .context("Failed to disconnect device")?;

    Ok(())
}

/// Set WiFi enabled via D-Bus.
pub async fn set_wifi_enabled_dbus(conn: &Connection, enabled: bool) -> Result<()> {
    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        NM_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .context("Failed to create Properties proxy")?;

    let v = zbus::zvariant::Value::from(enabled);
    let _: () = proxy
        .call("Set", &(NM_INTERFACE, "WirelessEnabled", v))
        .await
        .context("Failed to set WirelessEnabled")?;

    Ok(())
}

/// Connect wired via raw D-Bus.
///
/// Reads the device's AvailableConnections property and activates the first one.
/// This mirrors what `nmcli device connect` does internally, and is more reliable
/// than `ActivateConnection("/", device, "/")` which may fail after a Disconnect.
pub async fn connect_wired_dbus(conn: &Connection, device_path: &str) -> Result<()> {
    use zbus::zvariant::ObjectPath;

    // Read AvailableConnections property directly (ao = array of object paths)
    let props_proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        device_path,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .context("Failed to create Properties proxy")?;

    let (raw_value,): (OwnedValue,) = props_proxy
        .call(
            "Get",
            &(NM_DEVICE_INTERFACE, "AvailableConnections"),
        )
        .await
        .context("Failed to get AvailableConnections property")?;

    let available: Vec<OwnedObjectPath> = Vec::try_from(raw_value)
        .unwrap_or_default();

    let connection_path: ObjectPath = if let Some(first) = available.first() {
        log::debug!(
            "[nm] Using connection profile {} for device {}",
            first.as_str(),
            device_path
        );
        ObjectPath::try_from(first.as_str())
            .unwrap_or(ObjectPath::from_static_str_unchecked("/"))
    } else {
        log::debug!("[nm] No available connections for {}, using auto-detect", device_path);
        ObjectPath::from_static_str_unchecked("/")
    };

    let device_obj = ObjectPath::try_from(device_path)
        .with_context(|| format!("Invalid device path: {}", device_path))?;
    let no_specific = ObjectPath::from_static_str_unchecked("/");

    let nm_proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let _: (OwnedObjectPath,) = nm_proxy
        .call(
            "ActivateConnection",
            &(&connection_path, &device_obj, &no_specific),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to activate wired connection {} on {}",
                connection_path, device_path
            )
        })?;

    Ok(())
}
