//! BlueZ D-Bus helpers.
//!
//! Interacts with BlueZ on the system bus.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use zvariant::{OwnedValue, Value};

use crate::dbus::{owned_value_to_bool, owned_value_to_string, DbusHandle};

pub const BLUEZ_DEST: &str = "org.bluez";
pub const IFACE_ADAPTER1: &str = "org.bluez.Adapter1";
pub const IFACE_DEVICE1: &str = "org.bluez.Device1";
pub const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
pub const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

/// Represents a Bluetooth adapter.
#[derive(Debug, Clone)]
pub struct BluetoothAdapter {
    pub path: String,
    pub name: String,
    pub powered: bool,
}

/// Represents a paired Bluetooth device.
#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub connected: bool,
}

/// Find all Bluetooth adapters via ObjectManager.GetManagedObjects.
pub async fn find_all_adapters(conn: &DbusHandle) -> Result<Vec<BluetoothAdapter>> {
    let proxy = zbus::Proxy::new(&*conn.connection(), BLUEZ_DEST, "/", IFACE_OBJECT_MANAGER)
        .await
        .context("Failed to create ObjectManager proxy")?;

    // GetManagedObjects returns Dict<ObjectPath, Dict<Interface, Dict<Property, Variant>>>
    type ManagedObjects =
        HashMap<zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

    let (objects,): (ManagedObjects,) = proxy
        .call("GetManagedObjects", &())
        .await
        .context("Failed to call GetManagedObjects")?;

    let mut adapters = Vec::new();

    // Find all objects that have the org.bluez.Adapter1 interface
    for (path, interfaces) in objects {
        if let Some(adapter_props) = interfaces.get(IFACE_ADAPTER1) {
            let name = adapter_props
                .get("Alias")
                .or_else(|| adapter_props.get("Name"))
                .and_then(|v| owned_value_to_string(v.clone()))
                .unwrap_or_else(|| crate::i18n::t("bluetooth-title"));

            let powered = adapter_props
                .get("Powered")
                .and_then(|v| owned_value_to_bool(v.clone()))
                .unwrap_or(false);

            adapters.push(BluetoothAdapter {
                path: path.to_string(),
                name,
                powered,
            });
        }
    }

    // Sort by path for consistent ordering
    adapters.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(adapters)
}

/// Get the Powered property from an adapter.
pub async fn get_powered(conn: &DbusHandle, adapter_path: &str) -> Result<bool> {
    let proxy = zbus::Proxy::new(
        &*conn.connection(),
        BLUEZ_DEST,
        adapter_path,
        IFACE_PROPERTIES,
    )
    .await
    .context("Failed to create Properties proxy")?;

    let (value,): (OwnedValue,) = proxy
        .call("Get", &(IFACE_ADAPTER1, "Powered"))
        .await
        .context("Failed to get Powered property")?;

    Ok(owned_value_to_bool(value).unwrap_or(false))
}

/// Set the Powered property on an adapter.
pub async fn set_powered(conn: Arc<DbusHandle>, adapter_path: &str, powered: bool) -> Result<()> {
    let proxy = zbus::Proxy::new(
        &*conn.connection(),
        BLUEZ_DEST,
        adapter_path,
        IFACE_PROPERTIES,
    )
    .await
    .context("Failed to create Properties proxy")?;

    let v = Value::from(powered);

    let _: () = proxy
        .call("Set", &(IFACE_ADAPTER1, "Powered", v))
        .await
        .context("Failed to set Powered property")?;

    Ok(())
}

/// Get all paired devices belonging to the specified adapter via ObjectManager.
/// Only returns devices where Paired=true, sorted by name.
pub async fn get_paired_devices(
    conn: &DbusHandle,
    adapter_path: &str,
) -> Result<Vec<BluetoothDevice>> {
    let proxy = zbus::Proxy::new(&*conn.connection(), BLUEZ_DEST, "/", IFACE_OBJECT_MANAGER)
        .await
        .context("Failed to create ObjectManager proxy")?;

    type ManagedObjects =
        HashMap<zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

    let (objects,): (ManagedObjects,) = proxy
        .call("GetManagedObjects", &())
        .await
        .context("Failed to call GetManagedObjects")?;

    let mut devices = Vec::new();

    // Find all objects that have the org.bluez.Device1 interface
    // and belong to our adapter
    for (path, interfaces) in objects {
        let path_str = path.to_string();

        // Check if this device belongs to our adapter
        if !path_str.starts_with(adapter_path) {
            continue;
        }

        if let Some(device_props) = interfaces.get(IFACE_DEVICE1) {
            let paired = device_props
                .get("Paired")
                .and_then(|v| owned_value_to_bool(v.clone()))
                .unwrap_or(false);

            // Only include paired devices
            if !paired {
                continue;
            }

            let name = device_props
                .get("Alias")
                .or_else(|| device_props.get("Name"))
                .and_then(|v| owned_value_to_string(v.clone()))
                .unwrap_or_else(|| crate::i18n::t("bluetooth-unknown-device"));

            let icon = device_props
                .get("Icon")
                .and_then(|v| owned_value_to_string(v.clone()))
                .unwrap_or_else(|| "bluetooth-symbolic".to_string());

            let connected = device_props
                .get("Connected")
                .and_then(|v| owned_value_to_bool(v.clone()))
                .unwrap_or(false);

            devices.push(BluetoothDevice {
                path: path_str,
                name,
                icon,
                paired,
                connected,
            });
        }
    }

    // Sort by name
    devices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(devices)
}

/// Connect to a Bluetooth device.
pub async fn connect_device(conn: Arc<DbusHandle>, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(&*conn.connection(), BLUEZ_DEST, device_path, IFACE_DEVICE1)
        .await
        .context("Failed to create Device1 proxy")?;

    let _: () = proxy
        .call("Connect", &())
        .await
        .context("Failed to connect to device")?;

    Ok(())
}

/// Disconnect from a Bluetooth device.
pub async fn disconnect_device(conn: Arc<DbusHandle>, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(&*conn.connection(), BLUEZ_DEST, device_path, IFACE_DEVICE1)
        .await
        .context("Failed to create Device1 proxy")?;

    let _: () = proxy
        .call("Disconnect", &())
        .await
        .context("Failed to disconnect from device")?;

    Ok(())
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
