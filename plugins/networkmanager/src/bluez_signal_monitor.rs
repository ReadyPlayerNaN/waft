//! BlueZ D-Bus signal monitoring for paired device connection state.
//!
//! Monitors `org.bluez` `PropertiesChanged` signals on `Device1` interface
//! for `Connected` and `Paired` property changes. This drives tethering
//! visibility — the tethering adapter appears when a paired Bluetooth device
//! connects and disappears when it disconnects.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{info, warn};
use waft_plugin::EntityNotifier;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use crate::bluez_discovery::IFACE_DEVICE1;
use crate::state::{BluezPairedDevice, NmState};

const BLUEZ_DEST: &str = "org.bluez";

/// Monitor BlueZ PropertiesChanged signals for paired device connection state.
pub async fn monitor_bluez_signals(
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BLUEZ_DEST)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy for BlueZ")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add BlueZ match rule")?;

    info!("[nm] Listening for BlueZ PropertiesChanged signals");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("[nm] BlueZ D-Bus stream error: {e}");
                continue;
            }
        };

        let header = msg.header();
        #[allow(clippy::redundant_closure_for_method_calls)]
        if header.member().map(|m| m.as_str()) != Some("PropertiesChanged")
            || header.interface().map(|i| i.as_str()) != Some("org.freedesktop.DBus.Properties")
        {
            continue;
        }

        let obj_path = match header.path() {
            Some(p) => p.to_string(),
            None => continue,
        };

        let Ok((iface, props, _invalidated)) =
            msg.body()
                .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
        else {
            continue;
        };

        if iface != IFACE_DEVICE1 {
            continue;
        }

        let mut changed = false;

        // Handle Paired property changes (device paired/unpaired)
        if let Some(paired_val) = props.get("Paired")
            && let Ok(paired) = bool::try_from(paired_val.clone())
        {
            let mut st = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    warn!("[nm] Mutex poisoned, recovering: {e}");
                    e.into_inner()
                }
            };
            if paired {
                // Newly paired — add to tracking if not already present
                if !st.bluez_paired_devices.iter().any(|d| d.path == obj_path) {
                    let connected = props
                        .get("Connected")
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false);
                    info!(
                        "[nm] BlueZ device paired: {obj_path} connected={connected}"
                    );
                    st.bluez_paired_devices.push(BluezPairedDevice {
                        path: obj_path.clone(),
                        connected,
                    });
                    changed = true;
                }
            } else {
                // Unpaired — remove from tracking
                let before = st.bluez_paired_devices.len();
                st.bluez_paired_devices.retain(|d| d.path != obj_path);
                if st.bluez_paired_devices.len() != before {
                    info!("[nm] BlueZ device unpaired: {obj_path}");
                    changed = true;
                }
            }
        }

        // Handle Connected property changes
        if let Some(connected_val) = props.get("Connected")
            && let Ok(connected) = bool::try_from(connected_val.clone())
        {
            let mut st = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    warn!("[nm] Mutex poisoned, recovering: {e}");
                    e.into_inner()
                }
            };

            if let Some(device) = st
                .bluez_paired_devices
                .iter_mut()
                .find(|d| d.path == obj_path)
            {
                // Known paired device — update connection state
                if device.connected != connected {
                    info!("[nm] BlueZ device {obj_path} connected: {connected}");
                    device.connected = connected;
                    changed = true;
                }
            } else if connected {
                // Unknown device connecting — might be newly paired while running.
                // Add to tracking (we only care about connected devices here;
                // the Paired signal handler above catches the pairing event).
                info!("[nm] BlueZ unknown device connected, adding: {obj_path}");
                st.bluez_paired_devices.push(BluezPairedDevice {
                    path: obj_path.clone(),
                    connected: true,
                });
                changed = true;
            }
        }

        if changed {
            notifier.notify();
        }
    }

    Ok(())
}
