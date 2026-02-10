//! Network device discovery via nmrs and raw D-Bus.

use anyhow::Result;

use crate::dbus_property::{
    get_property, DEVICE_TYPE_ETHERNET, DEVICE_TYPE_WIFI, NM_DEVICE_INTERFACE,
};
use crate::is_virtual_interface;
use zbus::Connection;

/// Basic information about a network device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub device_type: u32,
    pub interface_name: String,
    pub device_state: u32,
}

/// Discover managed Ethernet and WiFi devices using nmrs.
pub async fn discover_devices(nm: &nmrs::NetworkManager) -> Result<Vec<DeviceInfo>> {
    let devices = nm
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list devices: {}", e))?;

    let mut result = Vec::new();
    for device in devices {
        let device_type = match device.device_type {
            nmrs::DeviceType::Ethernet => DEVICE_TYPE_ETHERNET,
            nmrs::DeviceType::Wifi => DEVICE_TYPE_WIFI,
            _ => continue,
        };

        if is_virtual_interface(&device.interface) {
            continue;
        }

        if !device.managed.unwrap_or(false) {
            continue;
        }

        let device_state = match device.state {
            nmrs::DeviceState::Unmanaged => 10,
            nmrs::DeviceState::Unavailable => 20,
            nmrs::DeviceState::Disconnected => 30,
            nmrs::DeviceState::Prepare => 40,
            nmrs::DeviceState::Config => 50,
            nmrs::DeviceState::Activated => 100,
            nmrs::DeviceState::Deactivating => 110,
            nmrs::DeviceState::Failed => 120,
            nmrs::DeviceState::Other(code) => code,
            _ => 0,
        };

        result.push(DeviceInfo {
            path: device.path.clone(),
            device_type,
            interface_name: device.interface.clone(),
            device_state,
        });
    }

    Ok(result)
}

/// Get device info for a specific device path using raw D-Bus.
pub async fn get_device_info_dbus(
    conn: &Connection,
    device_path: &str,
) -> Result<Option<DeviceInfo>> {
    let device_type: u32 =
        match get_property(conn, device_path, NM_DEVICE_INTERFACE, "DeviceType").await {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

    if device_type != DEVICE_TYPE_ETHERNET && device_type != DEVICE_TYPE_WIFI {
        return Ok(None);
    }

    let interface_name: String =
        get_property(conn, device_path, NM_DEVICE_INTERFACE, "Interface").await?;

    if is_virtual_interface(&interface_name) {
        return Ok(None);
    }

    let managed: bool = get_property(conn, device_path, NM_DEVICE_INTERFACE, "Managed")
        .await
        .unwrap_or(false);
    if !managed {
        return Ok(None);
    }

    let device_state: u32 = get_property(conn, device_path, NM_DEVICE_INTERFACE, "State")
        .await
        .unwrap_or(0);

    Ok(Some(DeviceInfo {
        path: device_path.to_string(),
        device_type,
        interface_name,
        device_state,
    }))
}
