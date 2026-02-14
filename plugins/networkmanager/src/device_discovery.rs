//! Network device discovery via D-Bus.

use anyhow::{Context, Result};

use crate::dbus_property::{
    get_property, NM_INTERFACE, NM_PATH, NM_SERVICE, DEVICE_TYPE_BLUETOOTH, DEVICE_TYPE_ETHERNET,
    DEVICE_TYPE_WIFI, NM_DEVICE_INTERFACE,
};
use crate::is_virtual_interface;
use zbus::zvariant::OwnedObjectPath;
use zbus::Connection;

/// Basic information about a network device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub device_type: u32,
    pub interface_name: String,
    pub device_state: u32,
}

/// Discover managed Ethernet and WiFi devices via D-Bus.
pub async fn discover_devices(conn: &Connection) -> Result<Vec<DeviceInfo>> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let (device_paths,): (Vec<OwnedObjectPath>,) = proxy
        .call("GetDevices", &())
        .await
        .context("Failed to call GetDevices")?;

    let mut result = Vec::new();
    for device_path in device_paths {
        match get_device_info_dbus(conn, device_path.as_str()).await {
            Ok(Some(info)) => result.push(info),
            Ok(None) => {} // filtered out (not ethernet/wifi, virtual, or unmanaged)
            Err(e) => {
                log::warn!("[nm] Failed to read device {}: {}", device_path, e);
            }
        }
    }

    Ok(result)
}

/// Discover bluetooth NM devices (type 5) with their state.
pub async fn discover_bluetooth_devices(
    conn: &Connection,
) -> Result<Vec<crate::state::BluetoothDeviceInfo>> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let (device_paths,): (Vec<OwnedObjectPath>,) = proxy
        .call("GetDevices", &())
        .await
        .context("Failed to call GetDevices")?;

    let mut result = Vec::new();
    for device_path in device_paths {
        let device_type: u32 = match get_property(
            conn,
            device_path.as_str(),
            NM_DEVICE_INTERFACE,
            "DeviceType",
        )
        .await
        {
            Ok(t) => t,
            Err(_) => continue,
        };

        if device_type == DEVICE_TYPE_BLUETOOTH {
            let device_state: u32 =
                get_property(conn, device_path.as_str(), NM_DEVICE_INTERFACE, "State")
                    .await
                    .unwrap_or(0);
            result.push(crate::state::BluetoothDeviceInfo {
                path: device_path.to_string(),
                device_state,
            });
        }
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
