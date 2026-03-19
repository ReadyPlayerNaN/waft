//! WiFi operations: scanning, connecting, disconnecting.

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use waft_plugin::entity::network::SecurityType;

use crate::AccessPoint;
use crate::dbus_property::{
    NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE, NM_SETTINGS_CONNECTION_INTERFACE,
    NM_SETTINGS_INTERFACE, NM_SETTINGS_PATH, NM_WIRELESS_INTERFACE,
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
                            cached_settings: None,
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
        cached_settings: None,
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
            NM_SETTINGS_CONNECTION_INTERFACE,
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

/// Delete a saved connection profile via D-Bus.
pub async fn delete_connection(conn: &Connection, connection_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        connection_path,
        NM_SETTINGS_CONNECTION_INTERFACE,
    )
    .await
    .context("Failed to create Settings.Connection proxy")?;

    let _: () = proxy
        .call("Delete", &())
        .await
        .with_context(|| format!("Failed to delete connection {connection_path}"))?;

    Ok(())
}

/// Settings read from a NM connection profile for WiFi entities.
#[derive(Debug, Clone, Default)]
pub struct ConnectionSettings {
    pub autoconnect: Option<bool>,
    pub metered: Option<i32>,
    pub ip_method: Option<String>,
    pub dns_servers: Option<Vec<String>>,
}

/// Read connection profile settings via `GetSettings` D-Bus call.
pub async fn get_connection_settings(
    conn: &Connection,
    connection_path: &str,
) -> Result<ConnectionSettings> {
    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        connection_path,
        NM_SETTINGS_CONNECTION_INTERFACE,
    )
    .await
    .context("Failed to create Settings.Connection proxy")?;

    let (settings,): (HashMap<String, HashMap<String, OwnedValue>>,) =
        proxy.call("GetSettings", &()).await?;

    let mut result = ConnectionSettings::default();

    // connection.autoconnect (defaults to true in NM when absent)
    if let Some(connection) = settings.get("connection") {
        if let Some(ac) = connection.get("autoconnect") {
            result.autoconnect = bool::try_from(ac.clone()).ok();
        }
        if let Some(metered) = connection.get("metered") {
            result.metered = i32::try_from(metered.clone()).ok();
        }
    }

    // ipv4.method
    if let Some(ipv4) = settings.get("ipv4") {
        if let Some(method) = ipv4.get("method") {
            result.ip_method = String::try_from(method.clone()).ok();
        }
        // ipv4.dns is an array of u32 (network-byte-order IPv4 addresses)
        if let Some(dns) = ipv4.get("dns")
            && let Ok(addrs) = <Vec<u32>>::try_from(dns.clone())
        {
            result.dns_servers = Some(
                addrs
                    .iter()
                    .map(|&addr| {
                        let bytes = addr.to_le_bytes();
                        format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
                    })
                    .collect(),
            );
        }
    }

    Ok(result)
}

/// Update connection profile settings via D-Bus.
///
/// Reads the current settings, applies the requested changes, and calls `Update`.
pub async fn update_connection_settings(
    conn: &Connection,
    connection_path: &str,
    updates: &serde_json::Value,
) -> Result<()> {
    use zbus::zvariant::Value;

    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        connection_path,
        NM_SETTINGS_CONNECTION_INTERFACE,
    )
    .await
    .context("Failed to create Settings.Connection proxy")?;

    let (mut settings,): (HashMap<String, HashMap<String, OwnedValue>>,) =
        proxy.call("GetSettings", &()).await?;

    // Apply autoconnect
    if let Some(ac) = updates.get("autoconnect").and_then(|v| v.as_bool()) {
        let section = settings
            .entry("connection".to_string())
            .or_insert_with(HashMap::new);
        section.insert(
            "autoconnect".to_string(),
            Value::from(ac).try_into().unwrap(),
        );
    }

    // Apply metered (NM metered values: 0=unknown, 1=yes, 2=no, 3=guess-yes, 4=guess-no)
    if let Some(metered) = updates.get("metered").and_then(|v| v.as_i64()) {
        let section = settings
            .entry("connection".to_string())
            .or_insert_with(HashMap::new);
        section.insert(
            "metered".to_string(),
            Value::from(metered as i32).try_into().unwrap(),
        );
    }

    // Apply ip_method
    if let Some(method) = updates.get("ip_method").and_then(|v| v.as_str()) {
        let section = settings
            .entry("ipv4".to_string())
            .or_insert_with(HashMap::new);
        section.insert(
            "method".to_string(),
            Value::from(method).try_into().unwrap(),
        );
    }

    // Apply dns_servers
    if let Some(dns_arr) = updates.get("dns_servers").and_then(|v| v.as_array()) {
        let addrs: Vec<u32> = dns_arr
            .iter()
            .filter_map(|v| v.as_str())
            .filter_map(parse_ipv4_to_u32)
            .collect();
        let section = settings
            .entry("ipv4".to_string())
            .or_insert_with(HashMap::new);
        section.insert(
            "dns".to_string(),
            Value::from(addrs).try_into().unwrap(),
        );
    }

    let _: () = proxy
        .call("Update", &(&settings,))
        .await
        .context("Failed to update connection settings")?;

    Ok(())
}

/// Retrieve the WiFi PSK (pre-shared key) for a saved connection via `GetSecrets`.
///
/// NM's `GetSettings` redacts sensitive data; `GetSecrets` returns it.
/// Returns `None` for open networks or if no secret is stored.
pub async fn get_wifi_psk(conn: &Connection, connection_path: &str) -> Result<Option<String>> {
    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        connection_path,
        NM_SETTINGS_CONNECTION_INTERFACE,
    )
    .await
    .context("Failed to create Settings.Connection proxy for secrets")?;

    let (secrets,): (HashMap<String, HashMap<String, OwnedValue>>,) = proxy
        .call("GetSecrets", &("802-11-wireless-security",))
        .await
        .context("GetSecrets call failed")?;

    let psk = secrets
        .get("802-11-wireless-security")
        .and_then(|sec| sec.get("psk"))
        .and_then(|v| String::try_from(v.clone()).ok());

    Ok(psk)
}

/// Build a WiFi QR code string in the `WIFI:` URI format.
///
/// Format: `WIFI:T:<security>;S:<ssid>;P:<password>;;`
///
/// Special characters in SSID and password are escaped with backslash per the spec.
pub fn build_wifi_qr_string(
    ssid: &str,
    password: Option<&str>,
    security: SecurityType,
) -> String {
    let auth_type = match security {
        SecurityType::Open => "nopass",
        SecurityType::Wep => "WEP",
        SecurityType::Enterprise => "WPA", // EAP uses WPA in QR format
        _ => "WPA",
    };

    let escaped_ssid = escape_wifi_qr_field(ssid);

    if let Some(pw) = password {
        let escaped_pw = escape_wifi_qr_field(pw);
        format!("WIFI:T:{auth_type};S:{escaped_ssid};P:{escaped_pw};;")
    } else {
        format!("WIFI:T:{auth_type};S:{escaped_ssid};;")
    }
}

/// Escape special characters in WiFi QR code fields.
///
/// Per the Wi-Fi QR code spec, these characters must be backslash-escaped:
/// `\`, `;`, `,`, `"`, `:`
fn escape_wifi_qr_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | ';' | ',' | '"' | ':' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Parse an IPv4 address string to a u32 in network byte order (little-endian for NM).
fn parse_ipv4_to_u32(s: &str) -> Option<u32> {
    let parts: Vec<u8> = s.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() == 4 {
        Some(u32::from_le_bytes([parts[0], parts[1], parts[2], parts[3]]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ipv4_standard_address() {
        let result = parse_ipv4_to_u32("192.168.1.1").unwrap();
        // 192.168.1.1 in little-endian bytes: [192, 168, 1, 1]
        assert_eq!(result, u32::from_le_bytes([192, 168, 1, 1]));
    }

    #[test]
    fn parse_ipv4_loopback() {
        let result = parse_ipv4_to_u32("127.0.0.1").unwrap();
        assert_eq!(result, u32::from_le_bytes([127, 0, 0, 1]));
    }

    #[test]
    fn parse_ipv4_google_dns() {
        let result = parse_ipv4_to_u32("8.8.8.8").unwrap();
        assert_eq!(result, u32::from_le_bytes([8, 8, 8, 8]));
    }

    #[test]
    fn parse_ipv4_all_zeros() {
        let result = parse_ipv4_to_u32("0.0.0.0").unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn parse_ipv4_all_max() {
        let result = parse_ipv4_to_u32("255.255.255.255").unwrap();
        assert_eq!(result, u32::from_le_bytes([255, 255, 255, 255]));
    }

    #[test]
    fn parse_ipv4_too_few_octets() {
        assert_eq!(parse_ipv4_to_u32("192.168.1"), None);
    }

    #[test]
    fn parse_ipv4_too_many_octets() {
        assert_eq!(parse_ipv4_to_u32("192.168.1.1.1"), None);
    }

    #[test]
    fn parse_ipv4_empty_string() {
        assert_eq!(parse_ipv4_to_u32(""), None);
    }

    #[test]
    fn parse_ipv4_non_numeric() {
        assert_eq!(parse_ipv4_to_u32("abc.def.ghi.jkl"), None);
    }

    #[test]
    fn parse_ipv4_octet_out_of_range() {
        // 256 doesn't fit in u8, so parse::<u8> fails, producing fewer than 4 parts
        assert_eq!(parse_ipv4_to_u32("256.1.1.1"), None);
    }

    #[test]
    fn wifi_qr_wpa2_with_password() {
        let qr = build_wifi_qr_string("MyNetwork", Some("secret123"), SecurityType::Wpa2);
        assert_eq!(qr, "WIFI:T:WPA;S:MyNetwork;P:secret123;;");
    }

    #[test]
    fn wifi_qr_open_no_password() {
        let qr = build_wifi_qr_string("OpenNet", None, SecurityType::Open);
        assert_eq!(qr, "WIFI:T:nopass;S:OpenNet;;");
    }

    #[test]
    fn wifi_qr_wep_with_password() {
        let qr = build_wifi_qr_string("WepNet", Some("wepkey"), SecurityType::Wep);
        assert_eq!(qr, "WIFI:T:WEP;S:WepNet;P:wepkey;;");
    }

    #[test]
    fn wifi_qr_escapes_special_chars() {
        let qr = build_wifi_qr_string("My;Net:work", Some("pass;word"), SecurityType::Wpa3);
        assert_eq!(qr, r"WIFI:T:WPA;S:My\;Net\:work;P:pass\;word;;");
    }

    #[test]
    fn wifi_qr_escapes_backslash_and_quotes() {
        let qr = build_wifi_qr_string(r#"Net"Work"#, Some(r"p\ass"), SecurityType::Wpa2);
        assert_eq!(qr, r#"WIFI:T:WPA;S:Net\"Work;P:p\\ass;;"#);
    }

    #[test]
    fn wifi_qr_escape_field_no_special() {
        assert_eq!(escape_wifi_qr_field("simple"), "simple");
    }

    #[test]
    fn wifi_qr_escape_field_all_special() {
        assert_eq!(escape_wifi_qr_field(r#"\;,":"#), r#"\\\;\,\"\:"#);
    }

    #[test]
    fn wifi_qr_wpa_maps_to_wpa_type() {
        let qr = build_wifi_qr_string("Net", Some("pass"), SecurityType::Wpa);
        assert_eq!(qr, "WIFI:T:WPA;S:Net;P:pass;;");
    }

    #[test]
    fn wifi_qr_enterprise_maps_to_wpa_type() {
        let qr = build_wifi_qr_string("CorpNet", Some("eap-pass"), SecurityType::Enterprise);
        assert_eq!(qr, "WIFI:T:WPA;S:CorpNet;P:eap-pass;;");
    }

    #[test]
    fn wifi_qr_empty_ssid() {
        let qr = build_wifi_qr_string("", Some("pass"), SecurityType::Wpa2);
        assert_eq!(qr, "WIFI:T:WPA;S:;P:pass;;");
    }

    #[test]
    fn wifi_qr_escape_comma_in_password() {
        let qr = build_wifi_qr_string("Net", Some("a,b"), SecurityType::Wpa2);
        assert_eq!(qr, r"WIFI:T:WPA;S:Net;P:a\,b;;");
    }
}
