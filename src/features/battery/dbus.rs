//! UPower D-Bus client.
//!
//! Reads battery status from the UPower DisplayDevice on the system bus.

use anyhow::{Context, Result};
use log::{info, warn};
use std::sync::Arc;

use crate::dbus::DbusHandle;

use super::values::{BatteryInfo, BatteryState};

const UPOWER_DEST: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const IFACE_DEVICE: &str = "org.freedesktop.UPower.Device";

/// Read all battery properties from the UPower DisplayDevice.
pub async fn get_battery_info(conn: &DbusHandle) -> Result<BatteryInfo> {
    let props = conn
        .get_all_properties(UPOWER_DEST, DISPLAY_DEVICE_PATH, IFACE_DEVICE)
        .await
        .context("Failed to get UPower DisplayDevice properties")?;

    let present = props
        .get("IsPresent")
        .and_then(|v| <bool>::try_from(v.clone()).ok())
        .unwrap_or(false);

    let percentage = props
        .get("Percentage")
        .and_then(|v| <f64>::try_from(v.clone()).ok())
        .unwrap_or(0.0);

    let state_u32 = props
        .get("State")
        .and_then(|v| <u32>::try_from(v.clone()).ok())
        .unwrap_or(0);

    let icon_name = props
        .get("IconName")
        .and_then(|v| <String>::try_from(v.clone()).ok())
        .unwrap_or_default();

    let time_to_empty = props
        .get("TimeToEmpty")
        .and_then(|v| <i64>::try_from(v.clone()).ok())
        .unwrap_or(0);

    let time_to_full = props
        .get("TimeToFull")
        .and_then(|v| <i64>::try_from(v.clone()).ok())
        .unwrap_or(0);

    Ok(BatteryInfo {
        present,
        percentage,
        state: BatteryState::from_u32(state_u32),
        icon_name,
        time_to_empty,
        time_to_full,
    })
}

/// Listen for PropertiesChanged signals on the UPower DisplayDevice.
/// On each change, re-reads all properties and sends the updated info through `tx`.
pub async fn listen_battery_changes(
    dbus: &DbusHandle,
    tx: flume::Sender<BatteryInfo>,
) -> Result<()> {
    let conn = Arc::new(dbus.clone());

    dbus.listen_properties_changed(
        UPOWER_DEST,
        DISPLAY_DEVICE_PATH,
        IFACE_DEVICE,
        move |_iface, _changed| {
            let conn = conn.clone();
            let tx = tx.clone();

            // Re-read all properties for consistency
            tokio::spawn(async move {
                match get_battery_info(&conn).await {
                    Ok(info) => {
                        info!(
                            "[battery/dbus] Updated: present={}, {}%, state={:?}",
                            info.present, info.percentage, info.state
                        );
                        let _ = tx.send(info);
                    }
                    Err(e) => {
                        warn!("[battery/dbus] Failed to read battery info: {e}");
                    }
                }
            });
        },
    )
    .await
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
