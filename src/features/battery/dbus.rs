//! UPower D-Bus client.
//!
//! Reads battery status from the UPower DisplayDevice on the system bus.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use zvariant::OwnedValue;

use crate::dbus::DbusHandle;

use super::values::{BatteryInfo, BatteryState};

const UPOWER_DEST: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const IFACE_DEVICE: &str = "org.freedesktop.UPower.Device";
const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// Read all battery properties from the UPower DisplayDevice.
pub async fn get_battery_info(conn: &DbusHandle) -> Result<BatteryInfo> {
    let proxy = zbus::Proxy::new(
        &*conn.connection(),
        UPOWER_DEST,
        DISPLAY_DEVICE_PATH,
        IFACE_PROPERTIES,
    )
    .await
    .context("Failed to create UPower Properties proxy")?;

    let (props,): (HashMap<String, OwnedValue>,) = proxy
        .call("GetAll", &(IFACE_DEVICE,))
        .await
        .context("Failed to call GetAll on UPower DisplayDevice")?;

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
///
/// On each change, re-reads all properties and sends the updated info through `tx`.
pub async fn listen_battery_changes(
    dbus: &DbusHandle,
    tx: flume::Sender<BatteryInfo>,
) -> Result<()> {
    let rule = format!(
        "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='{}',sender='{}'",
        DISPLAY_DEVICE_PATH, UPOWER_DEST
    );

    let mut rx = dbus.listen_signals(&rule).await?;
    let conn = Arc::new(dbus.clone());

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    // Verify the signal is for our device interface
                    if let Ok((iface, _, _)) =
                        msg.body()
                            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                    {
                        if iface != IFACE_DEVICE {
                            continue;
                        }
                    }

                    // Re-read all properties for consistency
                    match get_battery_info(&conn).await {
                        Ok(info) => {
                            info!(
                                "[battery/dbus] Updated: present={}, {}%, state={:?}",
                                info.present, info.percentage, info.state
                            );
                            if tx.send(info).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("[battery/dbus] Failed to read battery info: {e}");
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
        debug!("[battery] property monitoring stopped");
    });

    Ok(())
}
