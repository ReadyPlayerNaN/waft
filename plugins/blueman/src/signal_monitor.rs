//! BlueZ D-Bus signal monitoring for adapter and device state changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{info, warn};
use waft_plugin::EntityNotifier;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use crate::dbus::{BLUEZ_DEST, IFACE_ADAPTER1, IFACE_DEVICE1};
use crate::state::State;

pub async fn monitor_bluez_signals(
    conn: Connection,
    state: Arc<StdMutex<State>>,
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
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add match rule")?;

    info!("[bluetooth] Listening for BlueZ PropertiesChanged signals");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("[bluetooth] D-Bus stream error: {}", e);
                continue;
            }
        };

        let header = msg.header();
        if header.member().map(|m| m.as_str()) != Some("PropertiesChanged")
            || header.interface().map(|i| i.as_str())
                != Some("org.freedesktop.DBus.Properties")
        {
            continue;
        }

        let obj_path = match header.path() {
            Some(p) => p.to_string(),
            None => continue,
        };

        let Ok((iface, props, _invalidated)) = msg.body().deserialize::<(
            String,
            HashMap<String, OwnedValue>,
            Vec<String>,
        )>() else {
            continue;
        };

        let mut changed = false;

        if iface == IFACE_ADAPTER1 {
            if let Some(powered_val) = props.get("Powered") {
                if let Ok(powered) = <bool>::try_from(powered_val.clone()) {
                    let mut st = match state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[bluetooth] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    if let Some(adapter) =
                        st.adapters.iter_mut().find(|a| a.path == obj_path)
                    {
                        if adapter.powered != powered {
                            info!(
                                "[bluetooth] Adapter {} powered: {}",
                                obj_path, powered
                            );
                            adapter.powered = powered;
                            changed = true;
                        }
                    }
                }
            }
        } else if iface == IFACE_DEVICE1 {
            if let Some(connected_val) = props.get("Connected") {
                if let Ok(connected) = <bool>::try_from(connected_val.clone()) {
                    let mut st = match state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[bluetooth] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    for adapter in &mut st.adapters {
                        if let Some(device) =
                            adapter.devices.iter_mut().find(|d| d.path == obj_path)
                        {
                            if device.connected != connected {
                                info!(
                                    "[bluetooth] Device {} connected: {}",
                                    obj_path, connected
                                );
                                device.connected = connected;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }

        if changed {
            notifier.notify();
        }
    }

    Ok(())
}
