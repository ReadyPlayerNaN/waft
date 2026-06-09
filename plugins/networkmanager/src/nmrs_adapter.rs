//! Read-side helpers for mapping nmrs models into Waft plugin state.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use nmrs::NetworkManager;
use nmrs::models::{
    AccessPoint, DeviceState as NmDeviceState, DeviceType, SavedConnection, SettingsSummary,
};
use waft_plugin::entity::network::SecurityType;
use zbus::zvariant::OwnedValue;

use crate::device_discovery::DeviceInfo;
use crate::is_virtual_interface;
use crate::state::{AccessPointInfo, BluetoothDeviceInfo, CachedConnectionSettings};

pub fn nm_device_state_code(state: &NmDeviceState) -> u32 {
    match state {
        NmDeviceState::Unmanaged => 10,
        NmDeviceState::Unavailable => 20,
        NmDeviceState::Disconnected => 30,
        NmDeviceState::Prepare => 40,
        NmDeviceState::Config => 50,
        NmDeviceState::NeedAuth => 60,
        NmDeviceState::IpConfig => 70,
        NmDeviceState::IpCheck => 80,
        NmDeviceState::Secondaries => 90,
        NmDeviceState::Activated => 100,
        NmDeviceState::Deactivating => 110,
        NmDeviceState::Failed => 120,
        NmDeviceState::Other(code) => *code,
        _ => 0,
    }
}

pub fn security_type_from_security_features(sec: &nmrs::models::SecurityFeatures) -> SecurityType {
    if sec.eap || sec.eap_suite_b_192 {
        SecurityType::Enterprise
    } else if sec.sae {
        SecurityType::Wpa3
    } else if sec.psk {
        if sec.ccmp {
            SecurityType::Wpa2
        } else {
            SecurityType::Wpa
        }
    } else if sec.wep40 || sec.wep104 || sec.privacy {
        SecurityType::Wep
    } else {
        SecurityType::Open
    }
}

pub fn security_type_from_access_point(ap: &AccessPoint) -> SecurityType {
    security_type_from_security_features(&ap.security)
}

pub async fn discover_devices(nm: &NetworkManager) -> Result<Vec<DeviceInfo>> {
    let devices = nm.list_devices().await?;
    let mut result = Vec::new();

    for device in devices {
        if !matches!(device.device_type, DeviceType::Ethernet | DeviceType::Wifi) {
            continue;
        }
        if device.managed == Some(false) || is_virtual_interface(&device.interface) {
            continue;
        }

        result.push(DeviceInfo {
            path: device.path,
            device_type: match device.device_type {
                DeviceType::Ethernet => 1,
                DeviceType::Wifi => 2,
                DeviceType::Bluetooth => 5,
                DeviceType::Other(code) => code,
                _ => 0,
            },
            interface_name: device.interface,
            device_state: nm_device_state_code(&device.state),
        });
    }

    Ok(result)
}

pub async fn discover_bluetooth_devices(nm: &NetworkManager) -> Result<Vec<BluetoothDeviceInfo>> {
    let devices = nm.list_devices().await?;
    Ok(devices
        .into_iter()
        .filter(|device| matches!(device.device_type, DeviceType::Bluetooth))
        .map(|device| BluetoothDeviceInfo {
            path: device.path,
            device_state: nm_device_state_code(&device.state),
        })
        .collect())
}

pub async fn get_device_info_by_path(
    nm: &NetworkManager,
    device_path: &str,
) -> Result<Option<DeviceInfo>> {
    let devices = discover_devices(nm).await?;
    Ok(devices
        .into_iter()
        .find(|device| device.path == device_path))
}

pub async fn get_active_access_point(
    nm: &NetworkManager,
    interface: &str,
) -> Result<Option<AccessPointInfo>> {
    let mut aps = nm.list_access_points(Some(interface)).await?;
    aps.retain(|ap| ap.is_active);
    let Some(ap) = aps.into_iter().max_by_key(|ap| ap.strength) else {
        return Ok(None);
    };

    Ok(Some(AccessPointInfo {
        ssid: ap.ssid.clone(),
        strength: ap.strength,
        secure: !ap.security.is_open(),
        known: true,
        ap_path: ap.path.to_string(),
        security_type: security_type_from_access_point(&ap),
        cached_settings: None,
    }))
}

pub async fn scan_wifi_networks(
    nm: &NetworkManager,
    interfaces: &[String],
) -> Result<Vec<AccessPointInfo>> {
    for interface in interfaces {
        if let Err(err) = nm.wifi(interface).scan().await {
            log::warn!("[nm] Failed to trigger nmrs scan on {interface}: {err}");
        }
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let saved_wifi = wifi_saved_connections(nm).await?;
    let known_ssids: HashSet<String> = saved_wifi.keys().cloned().collect();

    let mut by_ssid: HashMap<String, AccessPointInfo> = HashMap::new();
    for interface in interfaces {
        let aps = match nm.list_access_points(Some(interface)).await {
            Ok(aps) => aps,
            Err(err) => {
                log::warn!("[nm] Failed to list APs for {interface}: {err}");
                continue;
            }
        };

        for ap in aps {
            if ap.ssid.is_empty() {
                continue;
            }

            let known = known_ssids.contains(&ap.ssid);
            let candidate = AccessPointInfo {
                ssid: ap.ssid.clone(),
                strength: ap.strength,
                secure: !ap.security.is_open(),
                known,
                ap_path: ap.path.to_string(),
                security_type: security_type_from_access_point(&ap),
                cached_settings: saved_wifi.get(&ap.ssid).cloned(),
            };

            match by_ssid.get(&ap.ssid) {
                Some(existing) if existing.strength >= candidate.strength => {}
                _ => {
                    by_ssid.insert(ap.ssid.clone(), candidate);
                }
            }
        }
    }

    let mut result: Vec<_> = by_ssid.into_values().collect();
    result.sort_by(|a, b| b.strength.cmp(&a.strength));
    Ok(result)
}

async fn wifi_saved_connections(
    nm: &NetworkManager,
) -> Result<HashMap<String, CachedConnectionSettings>> {
    let saved = nm.list_saved_connections().await?;
    let mut result = HashMap::new();

    for conn in saved {
        let ssid = match wifi_saved_ssid(&conn) {
            Some(ssid) => ssid,
            None => continue,
        };

        let raw = match nm.get_saved_connection_raw(&conn.uuid).await {
            Ok(raw) => raw,
            Err(err) => {
                log::debug!(
                    "[nm] Failed to load saved connection raw settings for {}: {err}",
                    conn.uuid
                );
                continue;
            }
        };

        result.insert(ssid, cached_settings_from_raw(&raw));
    }

    Ok(result)
}

fn wifi_saved_ssid(conn: &SavedConnection) -> Option<String> {
    match &conn.summary {
        SettingsSummary::Wifi { ssid, .. } => Some(ssid.clone()),
        _ => None,
    }
}

fn cached_settings_from_raw(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
) -> CachedConnectionSettings {
    let mut result = CachedConnectionSettings {
        autoconnect: None,
        metered: None,
        ip_method: None,
        dns_servers: None,
    };

    if let Some(connection) = settings.get("connection") {
        if let Some(ac) = connection.get("autoconnect") {
            result.autoconnect = bool::try_from(ac.clone()).ok();
        }
        if let Some(metered) = connection.get("metered") {
            result.metered = i32::try_from(metered.clone()).ok();
        }
    }

    if let Some(ipv4) = settings.get("ipv4") {
        if let Some(method) = ipv4.get("method") {
            result.ip_method = String::try_from(method.clone()).ok();
        }
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

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmrs::models::{DeviceState as NmDeviceState, SecurityFeatures};

    #[test]
    fn nm_device_state_code_maps_activated() {
        assert_eq!(nm_device_state_code(&NmDeviceState::Activated), 100);
    }

    #[test]
    fn security_type_maps_enterprise() {
        let mut sec = SecurityFeatures::default();
        sec.eap = true;
        assert_eq!(
            security_type_from_security_features(&sec),
            SecurityType::Enterprise
        );
    }

    #[test]
    fn security_type_maps_wpa3() {
        let mut sec = SecurityFeatures::default();
        sec.sae = true;
        assert_eq!(
            security_type_from_security_features(&sec),
            SecurityType::Wpa3
        );
    }

    #[test]
    fn security_type_maps_wpa2() {
        let mut sec = SecurityFeatures::default();
        sec.psk = true;
        sec.ccmp = true;
        assert_eq!(
            security_type_from_security_features(&sec),
            SecurityType::Wpa2
        );
    }

    #[test]
    fn security_type_maps_wpa() {
        let mut sec = SecurityFeatures::default();
        sec.psk = true;
        assert_eq!(
            security_type_from_security_features(&sec),
            SecurityType::Wpa
        );
    }

    #[test]
    fn security_type_maps_wep() {
        let mut sec = SecurityFeatures::default();
        sec.privacy = true;
        assert_eq!(
            security_type_from_security_features(&sec),
            SecurityType::Wep
        );
    }

    #[test]
    fn cached_settings_extracts_values() {
        let mut settings: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        settings.insert(
            "connection".to_string(),
            HashMap::from([
                ("autoconnect".to_string(), true.into()),
                ("metered".to_string(), 2i32.into()),
            ]),
        );
        settings.insert(
            "ipv4".to_string(),
            HashMap::from([
                (
                    "method".to_string(),
                    zbus::zvariant::Value::from("manual")
                        .try_into()
                        .expect("string value"),
                ),
                (
                    "dns".to_string(),
                    zbus::zvariant::Value::from(vec![u32::from_le_bytes([8, 8, 8, 8])])
                        .try_into()
                        .expect("vec value"),
                ),
            ]),
        );

        let parsed = cached_settings_from_raw(&settings);
        assert_eq!(parsed.autoconnect, Some(true));
        assert_eq!(parsed.metered, Some(2));
        assert_eq!(parsed.ip_method.as_deref(), Some("manual"));
        assert_eq!(parsed.dns_servers, Some(vec!["8.8.8.8".to_string()]));
    }
}
