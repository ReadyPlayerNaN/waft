//! D-Bus signal monitoring for NetworkManager state changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use zbus::zvariant::{ObjectPath, OwnedValue};
use zbus::Connection;

use crate::dbus_property::{
    get_property, NM_CONNECTION_ACTIVE_INTERFACE, NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH,
    NM_SERVICE, NM_VPN_CONNECTION_INTERFACE, DEVICE_TYPE_ETHERNET, DEVICE_TYPE_WIFI,
};
use crate::device_discovery::get_device_info_dbus;
use crate::state::{EthernetAdapterState, NmState, WiFiAdapterState};
use crate::vpn::refresh_vpn_states;
use waft_plugin::EntityNotifier;

/// Monitor NM D-Bus signals and update shared state accordingly.
pub async fn monitor_nm_signals(
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    // Subscribe to PropertiesChanged signals from NM
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add PropertiesChanged match rule")?;

    // Also subscribe to DeviceAdded/DeviceRemoved signals
    let device_added_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .path(NM_PATH)?
        .interface(NM_INTERFACE)?
        .member("DeviceAdded")?
        .build();
    dbus_proxy.add_match_rule(device_added_rule).await?;

    let device_removed_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .path(NM_PATH)?
        .interface(NM_INTERFACE)?
        .member("DeviceRemoved")?
        .build();
    dbus_proxy.add_match_rule(device_removed_rule).await?;

    // Subscribe to Device StateChanged signals
    let state_changed_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .interface(NM_DEVICE_INTERFACE)?
        .member("StateChanged")?
        .build();
    dbus_proxy.add_match_rule(state_changed_rule).await?;

    info!("[nm] Listening for NetworkManager signals");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("[nm] D-Bus stream error: {}", e);
                continue;
            }
        };

        let header = msg.header();
        let member = header.member().map(|m| m.as_str()).unwrap_or("");
        let iface = header.interface().map(|i| i.as_str()).unwrap_or("");
        let obj_path = header.path().map(|p| p.to_string()).unwrap_or_default();

        match (iface, member) {
            ("org.freedesktop.DBus.Properties", "PropertiesChanged") => {
                let Ok((prop_iface, props, _invalidated)) = msg.body().deserialize::<(
                    String,
                    HashMap<String, OwnedValue>,
                    Vec<String>,
                )>() else {
                    continue;
                };

                let mut changed = false;

                // Handle VPN ActiveConnection state changes
                if obj_path.contains("/ActiveConnection/")
                    && prop_iface == NM_CONNECTION_ACTIVE_INTERFACE
                {
                    if let Some(state_val) = props.get("State") {
                        if let Ok(state_code) = u32::try_from(state_val.clone()) {
                            // Check if this is a VPN connection
                            let is_vpn = if let Some(type_val) = props.get("Type") {
                                String::try_from(type_val.clone())
                                    .map(|t| t == "vpn")
                                    .unwrap_or(false)
                            } else {
                                // Query the type
                                get_property::<String>(
                                    &conn,
                                    &obj_path,
                                    NM_CONNECTION_ACTIVE_INTERFACE,
                                    "Type",
                                )
                                .await
                                .map(|t| t == "vpn")
                                .unwrap_or(false)
                            };

                            if is_vpn {
                                debug!(
                                    "[nm] VPN state changed: path={}, state={}",
                                    obj_path, state_code
                                );
                                if let Err(e) = refresh_vpn_states(&conn, &state).await {
                                    error!("[nm] Failed to refresh VPN states: {}", e);
                                }
                                changed = true;
                            }
                        }
                    }
                }

                // Handle VPN.Connection.VpnState changes
                if obj_path.contains("/ActiveConnection/")
                    && prop_iface == NM_VPN_CONNECTION_INTERFACE
                {
                    if props.contains_key("VpnState") {
                        debug!("[nm] VPN.Connection state changed: {}", obj_path);
                        if let Err(e) = refresh_vpn_states(&conn, &state).await {
                            error!("[nm] Failed to refresh VPN states: {}", e);
                        }
                        changed = true;
                    }
                }

                if changed {
                    notifier.notify();
                }
            }

            (iface_str, "DeviceAdded") if iface_str == NM_INTERFACE => {
                if let Ok(path) = msg.body().deserialize::<ObjectPath<'_>>() {
                    let device_path = path.to_string();
                    info!("[nm] Device added: {}", device_path);

                    if let Ok(Some(info)) = get_device_info_dbus(&conn, &device_path).await {
                        let mut st = match state.lock() {
                            Ok(g) => g,
                            Err(e) => {
                                warn!("[nm] Mutex poisoned, recovering: {e}");
                                e.into_inner()
                            }
                        };
                        match info.device_type {
                            DEVICE_TYPE_ETHERNET => {
                                if !st.ethernet_adapters.iter().any(|a| a.path == info.path) {
                                    st.ethernet_adapters.push(EthernetAdapterState {
                                        path: info.path,
                                        interface_name: info.interface_name,
                                        device_state: info.device_state,
                                    });
                                }
                            }
                            DEVICE_TYPE_WIFI => {
                                if !st.wifi_adapters.iter().any(|a| a.path == info.path) {
                                    st.wifi_adapters.push(WiFiAdapterState {
                                        path: info.path,
                                        interface_name: info.interface_name,
                                        enabled: true,
                                        busy: false,
                                        active_ssid: None,
                                        access_points: Vec::new(),
                                        scanning: false,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }

                    notifier.notify();
                }
            }

            (iface_str, "DeviceRemoved") if iface_str == NM_INTERFACE => {
                if let Ok(path) = msg.body().deserialize::<ObjectPath<'_>>() {
                    let device_path = path.to_string();
                    info!("[nm] Device removed: {}", device_path);

                    let mut st = match state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[nm] Mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    st.ethernet_adapters.retain(|a| a.path != device_path);
                    st.wifi_adapters.retain(|a| a.path != device_path);

                    notifier.notify();
                }
            }

            (iface_str, "StateChanged") if iface_str == NM_DEVICE_INTERFACE => {
                if let Ok((new_state, _old_state, _reason)) =
                    msg.body().deserialize::<(u32, u32, u32)>()
                {
                    let mut changed = false;
                    let mut st = match state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[nm] Mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };

                    // Update ethernet adapter state
                    if let Some(adapter) =
                        st.ethernet_adapters.iter_mut().find(|a| a.path == obj_path)
                    {
                        if adapter.device_state != new_state {
                            info!(
                                "[nm] Ethernet {} state: {} -> {}",
                                adapter.interface_name, adapter.device_state, new_state
                            );
                            adapter.device_state = new_state;
                            changed = true;
                        }
                    }

                    // Update WiFi adapter state
                    if let Some(adapter) =
                        st.wifi_adapters.iter_mut().find(|a| a.path == obj_path)
                    {
                        debug!(
                            "[nm] WiFi {} device state change: {}",
                            adapter.interface_name, new_state
                        );
                        // If device transitions away from activated, clear active SSID
                        if new_state != 100 && adapter.active_ssid.is_some() {
                            adapter.active_ssid = None;
                            changed = true;
                        }
                        // If device becomes activated, mark as changed (scan will update SSID)
                        if new_state == 100 && adapter.active_ssid.is_none() {
                            changed = true;
                        }
                    }

                    drop(st);

                    if changed {
                        notifier.notify();
                    }
                }
            }

            _ => {}
        }
    }

    Ok(())
}
