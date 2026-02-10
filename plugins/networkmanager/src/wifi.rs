//! WiFi operations: scanning, connecting, disconnecting.

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Connection;

use crate::dbus_property::{NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE, NM_SETTINGS_INTERFACE, NM_SETTINGS_PATH};
use crate::state::AccessPointInfo;

/// Scan and list known WiFi networks using nmrs (called from background task, not Send-required).
pub async fn scan_and_list_known_networks(
    nm: &nmrs::NetworkManager,
    conn: &Connection,
) -> Result<Vec<AccessPointInfo>> {
    // Trigger scan
    if let Err(e) = nm
        .scan_networks()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
    {
        warn!("[nm] Failed to trigger scan: {}", e);
    }

    // Wait for scan results
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let networks = nm
        .list_networks()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list networks: {}", e))?;

    let mut by_ssid: HashMap<String, AccessPointInfo> = HashMap::new();

    for network in &networks {
        if network.ssid.is_empty() {
            continue;
        }

        // Only include networks with saved connection profiles
        match get_connections_for_ssid(conn, &network.ssid).await {
            Ok(connections) if !connections.is_empty() => {}
            _ => {
                debug!("[nm] Skipping network {} (no saved profile)", network.ssid);
                continue;
            }
        }

        let strength = network.strength.unwrap_or(0);
        let secure = network.secured;

        match by_ssid.get(&network.ssid) {
            Some(existing) if existing.strength >= strength => {
                // Keep existing (stronger or equal)
            }
            _ => {
                by_ssid.insert(
                    network.ssid.clone(),
                    AccessPointInfo {
                        ssid: network.ssid.clone(),
                        strength,
                        secure,
                    },
                );
            }
        }
    }

    let mut result: Vec<AccessPointInfo> = by_ssid.into_values().collect();
    // Sort by signal strength (strongest first)
    result.sort_by(|a, b| b.strength.cmp(&a.strength));
    Ok(result)
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

/// Set WiFi enabled via raw D-Bus (avoids nmrs non-Send futures).
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

/// Connect wired via raw D-Bus (ActivateConnection with "/" for auto-activate).
pub async fn connect_wired_dbus(conn: &Connection) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let _: (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &("/", "/", "/"))
        .await
        .context("Failed to auto-activate wired connection")?;

    Ok(())
}
