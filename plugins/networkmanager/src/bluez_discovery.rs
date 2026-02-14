//! BlueZ paired device discovery for tethering availability.
//!
//! Queries BlueZ `ObjectManager.GetManagedObjects()` to find paired devices
//! and their `Connected` state. This is the source of truth for whether a
//! Bluetooth device is actually connected (NM's device state is unreliable
//! for this purpose).

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::debug;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Connection;

use crate::state::BluezPairedDevice;

const BLUEZ_DEST: &str = "org.bluez";
const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
pub const IFACE_DEVICE1: &str = "org.bluez.Device1";

type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Discover all paired BlueZ devices and their connection state.
pub async fn discover_bluez_paired_devices(
    conn: &Connection,
) -> Result<Vec<BluezPairedDevice>> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, "/", IFACE_OBJECT_MANAGER)
        .await
        .context("Failed to create BlueZ ObjectManager proxy")?;

    let (objects,): (ManagedObjects,) = proxy
        .call("GetManagedObjects", &())
        .await
        .context("Failed to call BlueZ GetManagedObjects")?;

    let mut devices = Vec::new();

    for (path, interfaces) in &objects {
        if let Some(device_props) = interfaces.get(IFACE_DEVICE1) {
            let paired = device_props
                .get("Paired")
                .and_then(|v| bool::try_from(v.clone()).ok())
                .unwrap_or(false);

            if !paired {
                continue;
            }

            let connected = device_props
                .get("Connected")
                .and_then(|v| bool::try_from(v.clone()).ok())
                .unwrap_or(false);

            debug!(
                "[nm] BlueZ paired device: {} connected={}",
                path, connected
            );

            devices.push(BluezPairedDevice {
                path: path.to_string(),
                connected,
            });
        }
    }

    Ok(devices)
}
