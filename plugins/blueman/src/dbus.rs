//! BlueZ D-Bus constants, property helpers, and operations.

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::info;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use crate::state::{AdapterState, DeviceState, State};

pub const BLUEZ_DEST: &str = "org.bluez";
pub const IFACE_ADAPTER1: &str = "org.bluez.Adapter1";
pub const IFACE_DEVICE1: &str = "org.bluez.Device1";
pub const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

// ---------------------------------------------------------------------------
// Property extraction
// ---------------------------------------------------------------------------

pub fn extract_prop<T: TryFrom<OwnedValue>>(
    props: &HashMap<String, OwnedValue>,
    key: &str,
    default: T,
) -> T {
    props
        .get(key)
        .and_then(|v| T::try_from(v.clone()).ok())
        .unwrap_or(default)
}

pub fn extract_prop_or(
    props: &HashMap<String, OwnedValue>,
    keys: &[&str],
    default: String,
) -> String {
    for key in keys {
        if let Some(v) = props.get(*key) {
            if let Ok(s) = String::try_from(v.clone()) {
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    default
}

// ---------------------------------------------------------------------------
// D-Bus operations
// ---------------------------------------------------------------------------

pub type ManagedObjects =
    HashMap<zbus::zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

pub async fn get_managed_objects(conn: &Connection) -> Result<ManagedObjects> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, "/", IFACE_OBJECT_MANAGER)
        .await
        .context("Failed to create ObjectManager proxy")?;

    let (objects,): (ManagedObjects,) = proxy
        .call("GetManagedObjects", &())
        .await
        .context("Failed to call GetManagedObjects")?;

    Ok(objects)
}

pub async fn load_state(conn: &Connection) -> Result<State> {
    let objects = get_managed_objects(conn).await?;

    let mut adapters: Vec<AdapterState> = Vec::new();

    // First pass: find all adapters
    for (path, interfaces) in &objects {
        if let Some(adapter_props) = interfaces.get(IFACE_ADAPTER1) {
            let name = extract_prop_or(adapter_props, &["Alias", "Name"], "Bluetooth".to_string());
            let powered = extract_prop(adapter_props, "Powered", false);

            adapters.push(AdapterState {
                path: path.to_string(),
                name,
                powered,
                devices: Vec::new(),
            });
        }
    }

    // Sort adapters by path for stable ordering
    adapters.sort_by(|a, b| a.path.cmp(&b.path));

    // Second pass: find paired devices for each adapter
    for adapter in &mut adapters {
        let mut devices: Vec<DeviceState> = Vec::new();

        for (path, interfaces) in &objects {
            let path_str = path.to_string();

            if !path_str.starts_with(&adapter.path) {
                continue;
            }

            if let Some(device_props) = interfaces.get(IFACE_DEVICE1) {
                let paired = extract_prop(device_props, "Paired", false);
                if !paired {
                    continue;
                }

                let name = extract_prop_or(
                    device_props,
                    &["Alias", "Name"],
                    "Unknown Device".to_string(),
                );
                let icon = extract_prop(
                    device_props,
                    "Icon",
                    "bluetooth-symbolic".to_string(),
                );
                let connected = extract_prop(device_props, "Connected", false);
                let battery_percentage: Option<u8> = device_props
                    .get("Percentage")
                    .and_then(|v| u8::try_from(v.clone()).ok())
                    .filter(|&p| p > 0);

                devices.push(DeviceState {
                    path: path_str,
                    name,
                    icon,
                    connected,
                    battery_percentage,
                });
            }
        }

        // Sort devices by name
        devices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        adapter.devices = devices;
    }

    Ok(State { adapters })
}

pub async fn set_powered(conn: &Connection, adapter_path: &str, powered: bool) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, adapter_path, IFACE_PROPERTIES)
        .await
        .context("Failed to create Properties proxy")?;

    let v = zbus::zvariant::Value::from(powered);
    let _: () = proxy
        .call("Set", &(IFACE_ADAPTER1, "Powered", v))
        .await
        .context("Failed to set Powered property")?;

    info!(
        "[bluetooth] Set adapter {} powered: {}",
        adapter_path, powered
    );

    Ok(())
}

pub async fn connect_device(conn: &Connection, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, device_path, IFACE_DEVICE1)
        .await
        .context("Failed to create Device1 proxy")?;

    let _: () = proxy
        .call("Connect", &())
        .await
        .context("Failed to connect to device")?;

    Ok(())
}

pub async fn disconnect_device(conn: &Connection, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, device_path, IFACE_DEVICE1)
        .await
        .context("Failed to create Device1 proxy")?;

    let _: () = proxy
        .call("Disconnect", &())
        .await
        .context("Failed to disconnect from device")?;

    Ok(())
}
