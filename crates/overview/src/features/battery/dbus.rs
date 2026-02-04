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

    let present = DbusHandle::extract_property(&props, "IsPresent", false);
    let percentage = DbusHandle::extract_property(&props, "Percentage", 0.0);
    let state_u32: u32 = DbusHandle::extract_property(&props, "State", 0);
    let icon_name = DbusHandle::extract_property(&props, "IconName", String::new());
    let time_to_empty = DbusHandle::extract_property(&props, "TimeToEmpty", 0);
    let time_to_full = DbusHandle::extract_property(&props, "TimeToFull", 0);

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
