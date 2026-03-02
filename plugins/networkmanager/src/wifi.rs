//! WiFi operations: scanning, connecting, disconnecting.

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use waft_plugin::entity::network::SecurityType;

use crate::AccessPoint;
use crate::dbus_property::{
    NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE, NM_SETTINGS_INTERFACE,
    NM_SETTINGS_PATH, NM_WIRELESS_INTERFACE,
};
use crate::detect_security_type;
use crate::state::AccessPointInfo;

/// Return the SSID of the currently active access point for a wireless device,
/// or None if the device has no active AP (path is "/" or D-Bus call fails).
pub async fn get_active_ssid(conn: &Connection, device_path: &str) -> Option<String> {
    use crate::dbus_property::get_property;

    let ap_path: zbus::zvariant::OwnedObjectPath =
        get_property(conn, device_path, NM_WIRELESS_INTERFACE, "ActiveAccessPoint")
            .await
            .ok()?;

    if ap_path.as_str() == "/" {
        return None;
    }

    let ssid_bytes: Vec<u8> = get_property(
        conn,
        ap_path.as_str(),
        "org.freedesktop.NetworkManager.AccessPoint",
        "Ssid",
    )
    .await
    .ok()?;

    let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();
    if ssid.is_empty() { None } else { Some(ssid) }
}

/// Scan WiFi networks and list all visible ones via D-Bus.
///
/// Calls `RequestScan` on each adapter, waits for results, then reads access points
/// via `GetAllAccessPoints`. Each network is marked with `known: true` if it has
/// a saved connection profile.
pub async fn scan_wifi_networks(
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

        let ap_paths: (Vec<OwnedObjectPath>,) = match proxy.call("GetAllAccessPoints", &()).await {
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

            let known = matches!(
                get_connections_for_ssid(conn, &ap.ssid).await,
                Ok(c) if !c.is_empty()
            );

            let secure = ap.is_secure();
            let security_type = detect_security_type(ap.flags, ap.wpa_flags, ap.rsn_flags);

            match by_ssid.get(&ap.ssid) {
                Some(existing) if existing.strength >= ap.strength => {}
                _ => {
                    by_ssid.insert(
                        ap.ssid.clone(),
                        AccessPointInfo {
                            ssid: ap.ssid,
                            strength: ap.strength,
                            secure,
                            known,
                            ap_path: ap.path.clone(),
                            security_type,
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

/// Return the access point info for the currently active AP on a wireless device,
/// or None if the device has no active AP (path is "/" or D-Bus call fails).
///
/// Unlike `get_active_ssid`, this also reads signal strength and security flags so
/// the caller can immediately show the connected network in the entity list without
/// waiting for a full scan.
pub async fn get_active_access_point(
    conn: &Connection,
    device_path: &str,
) -> Option<AccessPointInfo> {
    use crate::dbus_property::get_property;

    let ap_path: OwnedObjectPath =
        get_property(conn, device_path, NM_WIRELESS_INTERFACE, "ActiveAccessPoint")
            .await
            .ok()?;

    if ap_path.as_str() == "/" {
        return None;
    }

    let ap = read_access_point(conn, ap_path.as_str()).await.ok()?;
    if ap.ssid.is_empty() {
        return None;
    }

    let secure = ap.is_secure();
    let security_type = detect_security_type(ap.flags, ap.wpa_flags, ap.rsn_flags);
    Some(AccessPointInfo {
        ssid: ap.ssid,
        strength: ap.strength,
        secure,
        known: true, // connected → saved profile must exist
        ap_path: ap.path.clone(),
        security_type,
    })
}

/// Read access point properties from D-Bus.
pub async fn read_access_point(conn: &Connection, ap_path: &str) -> Result<AccessPoint> {
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

        if let Some(wireless) = settings.get("802-11-wireless")
            && let Some(ssid_value) = wireless.get("ssid")
            && let Ok(ssid_bytes) = <Vec<u8>>::try_from(ssid_value.clone())
        {
            let connection_ssid = String::from_utf8_lossy(&ssid_bytes);
            if connection_ssid == ssid {
                matching.push(path_str.to_string());
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
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, "org.freedesktop.DBus.Properties")
        .await
        .context("Failed to create Properties proxy")?;

    let v = zbus::zvariant::Value::from(enabled);
    let _: () = proxy
        .call("Set", &(NM_INTERFACE, "WirelessEnabled", v))
        .await
        .context("Failed to set WirelessEnabled")?;

    Ok(())
}

/// Create a new connection profile and activate it on the specified device.
///
/// Builds a partial NM connection settings dict from SSID, security type, and
/// optional password. NM fills in remaining defaults.
pub async fn add_and_activate_connection(
    conn: &Connection,
    device_path: &str,
    ap_path: &str,
    ssid: &str,
    security_type: SecurityType,
    password: Option<&str>,
) -> Result<String> {
    use zbus::zvariant::{ObjectPath, Value};

    // Build connection settings: a{sa{sv}}
    let mut connection_settings: HashMap<String, HashMap<String, Value<'_>>> = HashMap::new();

    // connection section
    let mut conn_section: HashMap<String, Value<'_>> = HashMap::new();
    conn_section.insert("type".to_string(), Value::from("802-11-wireless"));
    connection_settings.insert("connection".to_string(), conn_section);

    // 802-11-wireless section
    let mut wireless_section: HashMap<String, Value<'_>> = HashMap::new();
    wireless_section.insert("ssid".to_string(), Value::from(ssid.as_bytes().to_vec()));
    connection_settings.insert("802-11-wireless".to_string(), wireless_section);

    // 802-11-wireless-security section (if not open)
    match security_type {
        SecurityType::Open => {}
        SecurityType::Wep => {
            let mut sec: HashMap<String, Value<'_>> = HashMap::new();
            sec.insert("key-mgmt".to_string(), Value::from("none"));
            if let Some(pw) = password {
                sec.insert("wep-key0".to_string(), Value::from(pw));
            }
            connection_settings.insert("802-11-wireless-security".to_string(), sec);
        }
        SecurityType::Wpa | SecurityType::Wpa2 => {
            let mut sec: HashMap<String, Value<'_>> = HashMap::new();
            sec.insert("key-mgmt".to_string(), Value::from("wpa-psk"));
            if let Some(pw) = password {
                sec.insert("psk".to_string(), Value::from(pw));
            }
            connection_settings.insert("802-11-wireless-security".to_string(), sec);
        }
        SecurityType::Wpa3 => {
            let mut sec: HashMap<String, Value<'_>> = HashMap::new();
            sec.insert("key-mgmt".to_string(), Value::from("sae"));
            if let Some(pw) = password {
                sec.insert("psk".to_string(), Value::from(pw));
            }
            connection_settings.insert("802-11-wireless-security".to_string(), sec);
        }
        SecurityType::Enterprise => {
            // Enterprise networks require 802.1X configuration that varies widely.
            // We don't support this — caller should check before reaching here.
            return Err(anyhow::anyhow!("Enterprise (802.1X) networks are not supported"));
        }
    }

    let device_obj = ObjectPath::try_from(device_path)
        .with_context(|| format!("Invalid device path: {device_path}"))?;
    let ap_obj = ObjectPath::try_from(ap_path)
        .with_context(|| format!("Invalid AP path: {ap_path}"))?;

    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let (_settings_path, active_path): (OwnedObjectPath, OwnedObjectPath) = proxy
        .call(
            "AddAndActivateConnection",
            &(&connection_settings, &device_obj, &ap_obj),
        )
        .await
        .context("Failed to AddAndActivateConnection")?;

    Ok(active_path.to_string())
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
        .call("Get", &(NM_DEVICE_INTERFACE, "AvailableConnections"))
        .await
        .context("Failed to get AvailableConnections property")?;

    let available: Vec<OwnedObjectPath> = Vec::try_from(raw_value).unwrap_or_default();

    let connection_path: ObjectPath = if let Some(first) = available.first() {
        log::debug!(
            "[nm] Using connection profile {} for device {}",
            first.as_str(),
            device_path
        );
        ObjectPath::try_from(first.as_str()).unwrap_or(ObjectPath::from_static_str_unchecked("/"))
    } else {
        log::debug!(
            "[nm] No available connections for {}, using auto-detect",
            device_path
        );
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
