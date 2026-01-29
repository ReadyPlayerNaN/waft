//! NetworkManager D-Bus integration using nmrs.
//!
//! This module uses nmrs for most NetworkManager operations.
//! D-Bus is only used directly for features nmrs doesn't expose:
//! - Link speed queries
//! - Saved connection profile lookup
//! - WiFi connection activation with saved credentials

use anyhow::Result;
use nmrs::{DeviceState, DeviceType, NetworkManager};
use zbus::zvariant::OwnedValue;

use crate::dbus::DbusHandle;

// D-Bus constants (only used for features nmrs doesn't support)
const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_INTERFACE: &str = "org.freedesktop.NetworkManager";
const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";

const DEVICE_TYPE_ETHERNET: u32 = 1;
const DEVICE_TYPE_WIFI: u32 = 2;

// =============================================================================
// nmrs-based functions (primary API)
// =============================================================================

/// Create a new nmrs NetworkManager instance.
/// This establishes its own D-Bus connection to the system bus.
pub async fn create_network_manager() -> Result<NetworkManager> {
    NetworkManager::new()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create NetworkManager: {}", e))
}

/// Check if NetworkManager is available on the system bus.
pub async fn check_availability_nmrs() -> bool {
    NetworkManager::new().await.is_ok()
}

/// Get all managed ethernet and WiFi devices using nmrs.
/// Filters out virtual interfaces and unmanaged devices.
pub async fn get_all_devices_nmrs(nm: &NetworkManager) -> Result<Vec<DeviceInfo>> {
    let devices = nm
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list devices: {}", e))?;

    let mut result = Vec::new();

    for device in devices {
        let device_info = match device_info_from_nmrs(&device) {
            Some(info) => info,
            None => continue,
        };

        if is_virtual_interface(&device_info.interface_name) {
            continue;
        }

        if !device_info.managed {
            continue;
        }

        result.push(device_info);
    }

    Ok(result)
}

/// Check if WiFi is enabled using nmrs.
#[allow(dead_code)]
pub async fn wifi_enabled_nmrs(nm: &NetworkManager) -> Result<bool> {
    nm.wifi_enabled()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to check WiFi enabled: {}", e))
}

/// Set WiFi enabled state using nmrs.
pub async fn set_wifi_enabled_nmrs(nm: &NetworkManager, enabled: bool) -> Result<()> {
    nm.set_wifi_enabled(enabled)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to set WiFi enabled: {}", e))
}

/// Trigger a WiFi scan using nmrs.
pub async fn scan_networks_nmrs(nm: &NetworkManager) -> Result<()> {
    nm.scan_networks()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to scan networks: {}", e))
}

/// List visible WiFi networks using nmrs.
pub async fn list_networks_nmrs(nm: &NetworkManager) -> Result<Vec<AccessPoint>> {
    let networks = nm
        .list_networks()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list networks: {}", e))?;

    Ok(networks
        .iter()
        .filter(|n| !n.ssid.is_empty())
        .map(access_point_from_nmrs)
        .collect())
}

/// Connect to wired network using nmrs.
pub async fn connect_wired_nmrs(nm: &NetworkManager) -> Result<()> {
    nm.connect_wired()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect wired: {}", e))
}

/// Disconnect from current network using nmrs.
pub async fn disconnect_nmrs(nm: &NetworkManager) -> Result<()> {
    nm.disconnect()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to disconnect: {}", e))
}

// =============================================================================
// D-Bus functions (only for features nmrs doesn't support)
// =============================================================================

/// Get link speed for a wired device in Mbps.
/// Note: nmrs doesn't expose this property, so we use D-Bus directly.
pub async fn get_link_speed(dbus: &DbusHandle, device_path: &str) -> Result<Option<u32>> {
    match get_device_property::<u32>(dbus, device_path, "Speed").await {
        Ok(speed) => Ok(Some(speed)),
        Err(_) => Ok(None),
    }
}

/// Find all saved WiFi connections matching the given SSID.
/// Note: nmrs doesn't expose saved connection profiles, so we use D-Bus directly.
pub async fn get_connections_for_ssid(dbus: &DbusHandle, ssid: &str) -> Result<Vec<String>> {
    let settings_paths: Vec<zbus::zvariant::OwnedObjectPath> = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            "/org/freedesktop/NetworkManager/Settings",
            Some("org.freedesktop.NetworkManager.Settings"),
            "ListConnections",
            &(),
        )
        .await?
        .body()
        .deserialize()?;

    let mut matching_connections = Vec::new();

    for settings_path in settings_paths {
        let path_str = settings_path.as_str();

        let settings: std::collections::HashMap<String, std::collections::HashMap<String, OwnedValue>> = dbus
            .connection()
            .call_method(
                Some(NM_SERVICE),
                path_str,
                Some("org.freedesktop.NetworkManager.Settings.Connection"),
                "GetSettings",
                &(),
            )
            .await?
            .body()
            .deserialize()?;

        if let Some(wireless) = settings.get("802-11-wireless") {
            if let Some(ssid_value) = wireless.get("ssid") {
                if let Ok(ssid_bytes) = <Vec<u8>>::try_from(ssid_value.clone()) {
                    let connection_ssid = String::from_utf8_lossy(&ssid_bytes);
                    if connection_ssid == ssid {
                        matching_connections.push(path_str.to_string());
                    }
                }
            }
        }
    }

    Ok(matching_connections)
}

/// Activate a network connection on a device.
/// Note: nmrs requires credentials for WiFi, so we use D-Bus for saved connections.
pub async fn activate_connection(
    dbus: &DbusHandle,
    connection_path: Option<&str>,
    device_path: &str,
    specific_object: Option<&str>,
) -> Result<String> {
    let conn_path = connection_path.unwrap_or("/");
    let specific = specific_object.unwrap_or("/");

    let active_conn_path: zbus::zvariant::OwnedObjectPath = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            NM_PATH,
            Some(NM_INTERFACE),
            "ActivateConnection",
            &(conn_path, device_path, specific),
        )
        .await?
        .body()
        .deserialize()?;

    Ok(active_conn_path.to_string())
}

// =============================================================================
// Internal helpers and types
// =============================================================================

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub device_type: u32,
    pub interface_name: String,
    pub managed: bool,
    pub real: bool,
    pub device_state: u32,
}

#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub path: String,
    pub ssid: String,
    pub strength: u8,
    pub flags: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}

impl AccessPoint {
    pub fn is_secure(&self) -> bool {
        self.flags != 0 || self.wpa_flags != 0 || self.rsn_flags != 0
    }
}

fn device_state_to_u32(state: &DeviceState) -> u32 {
    match state {
        DeviceState::Unmanaged => 10,
        DeviceState::Unavailable => 20,
        DeviceState::Disconnected => 30,
        DeviceState::Prepare => 40,
        DeviceState::Config => 50,
        DeviceState::Activated => 100,
        DeviceState::Deactivating => 110,
        DeviceState::Failed => 120,
        DeviceState::Other(code) => *code,
        _ => 0,
    }
}

fn device_info_from_nmrs(device: &nmrs::Device) -> Option<DeviceInfo> {
    let device_type = match device.device_type {
        DeviceType::Ethernet => DEVICE_TYPE_ETHERNET,
        DeviceType::Wifi => DEVICE_TYPE_WIFI,
        _ => return None,
    };

    Some(DeviceInfo {
        path: device.path.clone(),
        device_type,
        interface_name: device.interface.clone(),
        managed: device.managed.unwrap_or(false),
        real: true,
        device_state: device_state_to_u32(&device.state),
    })
}

fn access_point_from_nmrs(network: &nmrs::Network) -> AccessPoint {
    let flags: u32 = if network.secured { 1 } else { 0 };
    let wpa_flags: u32 = if network.is_psk { 1 } else { 0 };
    let rsn_flags: u32 = if network.is_eap { 1 } else { 0 };

    AccessPoint {
        path: network.bssid.clone().unwrap_or_else(|| network.ssid.clone()),
        ssid: network.ssid.clone(),
        strength: network.strength.unwrap_or(0),
        flags,
        wpa_flags,
        rsn_flags,
    }
}

fn is_virtual_interface(name: &str) -> bool {
    let virtual_prefixes = ["docker", "veth", "br-", "virbr", "vnet"];
    virtual_prefixes.iter().any(|prefix| name.starts_with(prefix))
}

async fn get_device_property<T>(dbus: &DbusHandle, path: &str, property: &str) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let value: OwnedValue = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_DEVICE_INTERFACE, property),
        )
        .await?
        .body()
        .deserialize::<(OwnedValue,)>()?
        .0;

    value
        .try_into()
        .map_err(|e: T::Error| anyhow::anyhow!("Failed to convert property: {}", e))
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
