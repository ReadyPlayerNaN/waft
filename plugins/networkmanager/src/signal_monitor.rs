//! D-Bus signal monitoring for NetworkManager state changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedValue};

use crate::dbus_property::{
    DEVICE_TYPE_BLUETOOTH, DEVICE_TYPE_ETHERNET, DEVICE_TYPE_WIFI, NM_CONNECTION_ACTIVE_INTERFACE,
    NM_DEVICE_INTERFACE, NM_INTERFACE, NM_PATH, NM_SERVICE, NM_VPN_CONNECTION_INTERFACE,
    get_property,
};
use crate::device_discovery::get_device_info_dbus;
use crate::ethernet::refresh_ethernet_state;
use crate::ip_config::{fetch_public_ip, get_device_ip4_config};
use crate::state::{
    BluetoothDeviceInfo, CachedIpConfig, EthernetAdapterState, NmState, WiFiAdapterState,
};
use crate::tethering::refresh_tethering_states;
use crate::vpn::{is_vpn_type, refresh_vpn_states};
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
                let Ok((prop_iface, props, _invalidated)) =
                    msg.body()
                        .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                else {
                    continue;
                };

                let mut changed = false;

                // Handle VPN and tethering ActiveConnection state changes
                if obj_path.contains("/ActiveConnection/")
                    && prop_iface == NM_CONNECTION_ACTIVE_INTERFACE
                    && let Some(state_val) = props.get("State")
                    && let Ok(state_code) = u32::try_from(state_val.clone())
                {
                    let conn_type = if let Some(type_val) = props.get("Type") {
                        String::try_from(type_val.clone()).unwrap_or_default()
                    } else {
                        get_property::<String>(
                            &conn,
                            &obj_path,
                            NM_CONNECTION_ACTIVE_INTERFACE,
                            "Type",
                        )
                        .await
                        .unwrap_or_default()
                    };

                    if is_vpn_type(&conn_type) {
                        debug!(
                            "[nm] VPN state changed: path={}, state={}",
                            obj_path, state_code
                        );
                        if let Err(e) = refresh_vpn_states(&conn, &state).await {
                            error!("[nm] Failed to refresh VPN states: {}", e);
                        }
                        changed = true;
                    } else if conn_type == "bluetooth" {
                        debug!(
                            "[nm] Tethering state changed: path={}, state={}",
                            obj_path, state_code
                        );
                        if let Err(e) = refresh_tethering_states(&conn, &state).await {
                            error!("[nm] Failed to refresh tethering states: {}", e);
                        }
                        changed = true;
                    }
                }

                // Handle VPN.Connection.VpnState changes
                if obj_path.contains("/ActiveConnection/")
                    && prop_iface == NM_VPN_CONNECTION_INTERFACE
                    && props.contains_key("VpnState")
                {
                    debug!("[nm] VPN.Connection state changed: {}", obj_path);
                    if let Err(e) = refresh_vpn_states(&conn, &state).await {
                        error!("[nm] Failed to refresh VPN states: {}", e);
                    }
                    changed = true;
                }

                if changed {
                    notifier.notify();
                }
            }

            (iface_str, "DeviceAdded") if iface_str == NM_INTERFACE => {
                if let Ok(path) = msg.body().deserialize::<ObjectPath<'_>>() {
                    let device_path = path.to_string();
                    info!("[nm] Device added: {}", device_path);

                    // Read device type first, then branch without holding locks across awaits
                    let device_type: u32 =
                        get_property(&conn, &device_path, NM_DEVICE_INTERFACE, "DeviceType")
                            .await
                            .unwrap_or(0);

                    match device_type {
                        DEVICE_TYPE_ETHERNET | DEVICE_TYPE_WIFI => {
                            if let Ok(Some(info)) = get_device_info_dbus(&conn, &device_path).await
                            {
                                let mut st = match state.lock() {
                                    Ok(g) => g,
                                    Err(e) => {
                                        warn!("[nm] Mutex poisoned, recovering: {e}");
                                        e.into_inner()
                                    }
                                };
                                match info.device_type {
                                    DEVICE_TYPE_ETHERNET => {
                                        if !st.ethernet_adapters.iter().any(|a| a.path == info.path)
                                        {
                                            st.ethernet_adapters.push(EthernetAdapterState {
                                                path: info.path,
                                                interface_name: info.interface_name,
                                                device_state: info.device_state,
                                                ip_config: None,
                                                active_connection_uuid: None,
                                                profiles: Vec::new(),
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
                        }
                        DEVICE_TYPE_BLUETOOTH => {
                            let bt_state: u32 =
                                get_property(&conn, &device_path, NM_DEVICE_INTERFACE, "State")
                                    .await
                                    .unwrap_or(0);
                            info!(
                                "[nm] Bluetooth device added: {} state={}",
                                device_path, bt_state
                            );
                            {
                                let mut st = match state.lock() {
                                    Ok(g) => g,
                                    Err(e) => {
                                        warn!("[nm] Mutex poisoned, recovering: {e}");
                                        e.into_inner()
                                    }
                                };
                                if !st.bluetooth_devices.iter().any(|d| d.path == device_path) {
                                    st.bluetooth_devices.push(BluetoothDeviceInfo {
                                        path: device_path.clone(),
                                        device_state: bt_state,
                                    });
                                }
                            }
                            if let Err(e) = refresh_tethering_states(&conn, &state).await {
                                error!("[nm] Failed to refresh tethering states: {}", e);
                            }
                        }
                        _ => {}
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
                    st.bluetooth_devices.retain(|d| d.path != device_path);

                    notifier.notify();
                }
            }

            (iface_str, "StateChanged") if iface_str == NM_DEVICE_INTERFACE => {
                if let Ok((new_state, _old_state, _reason)) =
                    msg.body().deserialize::<(u32, u32, u32)>()
                {
                    let mut changed = false;
                    let mut refresh_ip_for_device: Option<String> = None;
                    let mut clear_ip = false;

                    {
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
                            && adapter.device_state != new_state
                        {
                            let was_connected = adapter.is_connected();
                            info!(
                                "[nm] Ethernet {} state: {} -> {}",
                                adapter.interface_name, adapter.device_state, new_state
                            );
                            adapter.device_state = new_state;
                            changed = true;

                            if adapter.is_connected() && !was_connected {
                                // Just connected - schedule IP config refresh
                                refresh_ip_for_device = Some(obj_path.clone());
                            } else if !adapter.is_connected() && was_connected {
                                // Disconnected - clear IP config
                                adapter.ip_config = None;
                                clear_ip = true;
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

                        // Update bluetooth device state (affects tethering visibility)
                        if let Some(bt_dev) =
                            st.bluetooth_devices.iter_mut().find(|d| d.path == obj_path)
                            && bt_dev.device_state != new_state
                        {
                            debug!(
                                "[nm] Bluetooth device {} state: {} -> {}",
                                obj_path, bt_dev.device_state, new_state
                            );
                            bt_dev.device_state = new_state;
                            changed = true;
                        }
                    }

                    // Refresh IP config outside the lock
                    if let Some(device_path) = refresh_ip_for_device {
                        // Small delay to let NM finish setting up the connection
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        if let Ok(Some(ip)) = get_device_ip4_config(&conn, &device_path).await {
                            let mut st = match state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    warn!("[nm] Mutex poisoned, recovering: {e}");
                                    e.into_inner()
                                }
                            };
                            if let Some(adapter) = st
                                .ethernet_adapters
                                .iter_mut()
                                .find(|a| a.path == device_path)
                            {
                                adapter.ip_config = Some(CachedIpConfig {
                                    address: ip.address,
                                    prefix: ip.prefix,
                                    gateway: ip.gateway,
                                });
                            }
                        }

                        // Refresh ethernet profile active connection state
                        if let Err(e) = refresh_ethernet_state(&conn, &state).await {
                            warn!("[nm] Failed to refresh ethernet state: {}", e);
                        }

                        // Also refresh public IP
                        if let Some(public_ip) = fetch_public_ip().await {
                            let mut st = match state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    warn!("[nm] Mutex poisoned, recovering: {e}");
                                    e.into_inner()
                                }
                            };
                            st.public_ip = Some(public_ip);
                        }
                    }

                    if clear_ip {
                        // Check if any adapter is still connected; if not, clear public IP
                        let any_connected = {
                            let st = match state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    warn!("[nm] Mutex poisoned, recovering: {e}");
                                    e.into_inner()
                                }
                            };
                            st.ethernet_adapters.iter().any(|a| a.is_connected())
                                || st.wifi_adapters.iter().any(|a| a.active_ssid.is_some())
                        };
                        if !any_connected {
                            let mut st = match state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    warn!("[nm] Mutex poisoned, recovering: {e}");
                                    e.into_inner()
                                }
                            };
                            st.public_ip = None;
                        }
                    }

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
