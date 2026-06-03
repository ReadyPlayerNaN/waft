//! Remaining custom WiFi helpers that are not yet covered cleanly by public nmrs APIs.
//!
//! This module intentionally contains only the custom/raw-D-Bus paths that Waft
//! still needs after the nmrs migration:
//! - saved connection lookup by SSID
//! - legacy saved-profile activation by object path
//! - fresh WEP / legacy AddAndActivateConnection fallback
//! - per-adapter raw wired activation fallback
//! - WiFi secret lookup for QR sharing
//! - QR payload formatting

use std::collections::HashMap;

use anyhow::{Context, Result};
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use waft_plugin::entity::network::SecurityType;

use crate::dbus_property::{
    NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE, NM_SETTINGS_CONNECTION_INTERFACE,
    NM_SETTINGS_INTERFACE, NM_SETTINGS_PATH,
};

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

/// Activate a saved connection object on a device.
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

/// Create a new WiFi connection profile and activate it on the specified device.
///
/// This remains as the legacy fallback for features not currently covered by the
/// public nmrs WiFi connect surface, notably fresh WEP creation.
pub async fn add_and_activate_connection(
    conn: &Connection,
    device_path: &str,
    ap_path: &str,
    ssid: &str,
    security_type: SecurityType,
    password: Option<&str>,
) -> Result<String> {
    use zbus::zvariant::{ObjectPath, Value};

    let mut connection_settings: HashMap<String, HashMap<String, Value<'_>>> = HashMap::new();

    let mut conn_section: HashMap<String, Value<'_>> = HashMap::new();
    conn_section.insert("type".to_string(), Value::from("802-11-wireless"));
    connection_settings.insert("connection".to_string(), conn_section);

    let mut wireless_section: HashMap<String, Value<'_>> = HashMap::new();
    wireless_section.insert("ssid".to_string(), Value::from(ssid.as_bytes().to_vec()));
    connection_settings.insert("802-11-wireless".to_string(), wireless_section);

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
            return Err(anyhow::anyhow!(
                "Enterprise (802.1X) networks are not supported by the legacy fallback"
            ));
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
/// This remains as the fallback for per-adapter/per-profile Ethernet behavior
/// that is not exposed cleanly by public nmrs APIs.
pub async fn connect_wired_dbus(conn: &Connection, device_path: &str) -> Result<()> {
    use zbus::zvariant::ObjectPath;

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
        log::debug!("[nm] No available connections for {device_path}, using auto-detect");
        ObjectPath::from_static_str_unchecked("/")
    };

    let device_obj = ObjectPath::try_from(device_path)
        .with_context(|| format!("Invalid device path: {device_path}"))?;
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
                "Failed to activate wired connection {connection_path} on {device_path}"
            )
        })?;

    Ok(())
}

/// Retrieve the WiFi PSK (pre-shared key) for a saved connection via `GetSecrets`.
///
/// This remains custom because public nmrs saved-profile APIs intentionally do not
/// expose secrets directly.
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
pub fn build_wifi_qr_string(
    ssid: &str,
    password: Option<&str>,
    security: SecurityType,
) -> String {
    let auth_type = match security {
        SecurityType::Open => "nopass",
        SecurityType::Wep => "WEP",
        SecurityType::Enterprise => "WPA",
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let qr = build_wifi_qr_string(r#"Net\"Work"#, Some(r"p\ass"), SecurityType::Wpa2);
        assert_eq!(qr, r#"WIFI:T:WPA;S:Net\\\"Work;P:p\\ass;;"#);
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
}
