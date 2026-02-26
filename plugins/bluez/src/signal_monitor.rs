//! BlueZ D-Bus signal monitoring for adapter and device state changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{debug, info, warn};
use waft_plugin::{EntityNotifier, StateLocker};
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use waft_protocol::entity::bluetooth::ConnectionState;

use crate::dbus::{
    self, BLUEZ_DEST, IFACE_ADAPTER1, IFACE_DEVICE1, IFACE_OBJECT_MANAGER, IFACE_PROPERTIES,
};
use crate::state::State;

pub async fn monitor_bluez_signals(
    conn: Connection,
    state: Arc<StdMutex<State>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BLUEZ_DEST)?
        .interface(IFACE_PROPERTIES)?
        .member("PropertiesChanged")?
        .build();

    let obj_mgr_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BLUEZ_DEST)?
        .interface(IFACE_OBJECT_MANAGER)?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add PropertiesChanged match rule")?;

    dbus_proxy
        .add_match_rule(obj_mgr_rule)
        .await
        .context("Failed to add ObjectManager match rule")?;

    info!("[bluetooth] Listening for BlueZ PropertiesChanged and ObjectManager signals");

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
        let member = match header.member() {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let iface = match header.interface() {
            Some(i) => i.as_str().to_string(),
            None => continue,
        };

        let obj_path = match header.path() {
            Some(p) => p.to_string(),
            None => continue,
        };

        let mut changed = false;

        if iface == IFACE_PROPERTIES && member == "PropertiesChanged" {
            let Ok((prop_iface, props, _invalidated)) =
                msg.body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                continue;
            };

            if prop_iface == IFACE_ADAPTER1 {
                changed = handle_adapter_properties_changed(&state, &obj_path, &props);
            } else if prop_iface == IFACE_DEVICE1 {
                changed = handle_device_properties_changed(&state, &obj_path, &props);
            }
        } else if iface == IFACE_OBJECT_MANAGER && member == "InterfacesAdded" {
            changed = handle_interfaces_added(&state, &msg);
        } else if iface == IFACE_OBJECT_MANAGER && member == "InterfacesRemoved" {
            changed = handle_interfaces_removed(&state, &msg);
        }

        if changed {
            notifier.notify();
        }
    }

    warn!("[bluetooth] D-Bus signal stream ended -- signal monitoring is now unresponsive");

    Ok(())
}

/// Handle PropertiesChanged for an adapter (Powered, Discoverable, Discovering).
fn handle_adapter_properties_changed(
    state: &Arc<StdMutex<State>>,
    obj_path: &str,
    props: &HashMap<String, OwnedValue>,
) -> bool {
    let mut changed = false;
    let mut st = state.lock_or_recover();

    let Some(adapter) = st.adapters.iter_mut().find(|a| a.path == obj_path) else {
        return false;
    };

    if let Some(powered_val) = props.get("Powered")
        && let Ok(powered) = <bool>::try_from(powered_val.clone())
        && adapter.powered != powered
    {
        info!("[bluetooth] Adapter {} powered: {}", obj_path, powered);
        adapter.powered = powered;
        changed = true;
    }

    if let Some(discoverable_val) = props.get("Discoverable")
        && let Ok(discoverable) = <bool>::try_from(discoverable_val.clone())
        && adapter.discoverable != discoverable
    {
        info!(
            "[bluetooth] Adapter {} discoverable: {}",
            obj_path, discoverable
        );
        adapter.discoverable = discoverable;
        changed = true;
    }

    if let Some(discovering_val) = props.get("Discovering")
        && let Ok(discovering) = <bool>::try_from(discovering_val.clone())
        && adapter.discovering != discovering
    {
        info!(
            "[bluetooth] Adapter {} discovering: {}",
            obj_path, discovering
        );
        adapter.discovering = discovering;
        changed = true;
    }

    // Alias or Name change
    if let Some(name_val) = props.get("Alias").or_else(|| props.get("Name"))
        && let Ok(name) = String::try_from(name_val.clone())
        && !name.is_empty()
        && adapter.name != name
    {
        info!(
            "[bluetooth] Adapter {} name: {} -> {}",
            obj_path, adapter.name, name
        );
        adapter.name = name;
        changed = true;
    }

    changed
}

/// Handle PropertiesChanged for a device (Connected, Paired, Trusted, RSSI, Name/Alias).
fn handle_device_properties_changed(
    state: &Arc<StdMutex<State>>,
    obj_path: &str,
    props: &HashMap<String, OwnedValue>,
) -> bool {
    let mut changed = false;
    let mut st = state.lock_or_recover();

    for adapter in &mut st.adapters {
        let Some(device) = adapter.devices.iter_mut().find(|d| d.path == obj_path) else {
            continue;
        };

        if let Some(connected_val) = props.get("Connected")
            && let Ok(connected) = <bool>::try_from(connected_val.clone())
        {
            let new_state = if connected {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            };
            if device.connection_state != new_state {
                info!(
                    "[bluetooth] Device {} connection_state: {:?}",
                    obj_path, new_state
                );
                device.connection_state = new_state;
                changed = true;
            }
        }

        if let Some(paired_val) = props.get("Paired")
            && let Ok(paired) = <bool>::try_from(paired_val.clone())
            && device.paired != paired
        {
            info!("[bluetooth] Device {} paired: {}", obj_path, paired);
            device.paired = paired;
            changed = true;
        }

        if let Some(trusted_val) = props.get("Trusted")
            && let Ok(trusted) = <bool>::try_from(trusted_val.clone())
            && device.trusted != trusted
        {
            info!("[bluetooth] Device {} trusted: {}", obj_path, trusted);
            device.trusted = trusted;
            changed = true;
        }

        if let Some(rssi_val) = props.get("RSSI")
            && let Ok(rssi) = <i16>::try_from(rssi_val.clone())
        {
            let new_rssi = Some(rssi);
            if device.rssi != new_rssi {
                debug!("[bluetooth] Device {} rssi: {}", obj_path, rssi);
                device.rssi = new_rssi;
                changed = true;
            }
        }

        // Alias or Name change (prefer Alias, matching parse_device_props)
        if let Some(name_val) = props.get("Alias").or_else(|| props.get("Name"))
            && let Ok(name) = String::try_from(name_val.clone())
            && !name.is_empty()
            && device.name != name
        {
            info!(
                "[bluetooth] Device {} name: {} -> {}",
                obj_path, device.name, name
            );
            device.name = name;
            changed = true;
        }

        return changed;
    }

    changed
}

/// Handle InterfacesAdded: add new devices when Device1 interface appears.
fn handle_interfaces_added(state: &Arc<StdMutex<State>>, msg: &zbus::Message) -> bool {
    let Ok((path, interfaces)) = msg.body().deserialize::<(
        zbus::zvariant::OwnedObjectPath,
        HashMap<String, HashMap<String, OwnedValue>>,
    )>() else {
        return false;
    };

    let Some(device_props) = interfaces.get(IFACE_DEVICE1) else {
        return false;
    };

    let path_str = path.to_string();

    let mut st = state.lock_or_recover();

    // Find the adapter this device belongs to
    let adapter = st
        .adapters
        .iter_mut()
        .find(|a| path_str.starts_with(&a.path));

    let Some(adapter) = adapter else {
        debug!(
            "[bluetooth] InterfacesAdded for device {} but no matching adapter found",
            path_str
        );
        return false;
    };

    // Avoid adding duplicate devices
    if adapter.devices.iter().any(|d| d.path == path_str) {
        return false;
    }

    let device = dbus::parse_device_props(path_str.clone(), device_props);
    info!(
        "[bluetooth] New device appeared: {} ({})",
        device.name, path_str
    );
    adapter.devices.push(device);

    // Keep devices sorted by name
    adapter
        .devices
        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    true
}

/// Handle InterfacesRemoved: remove devices when Device1 interface disappears.
fn handle_interfaces_removed(state: &Arc<StdMutex<State>>, msg: &zbus::Message) -> bool {
    let Ok((path, interfaces)) = msg
        .body()
        .deserialize::<(zbus::zvariant::OwnedObjectPath, Vec<String>)>()
    else {
        return false;
    };

    if !interfaces.iter().any(|i| i == IFACE_DEVICE1) {
        return false;
    }

    let path_str = path.to_string();

    let mut st = state.lock_or_recover();

    let mut removed = false;
    for adapter in &mut st.adapters {
        let before = adapter.devices.len();
        adapter.devices.retain(|d| d.path != path_str);
        if adapter.devices.len() != before {
            info!("[bluetooth] Device removed: {}", path_str);
            removed = true;
        }
    }

    removed
}
