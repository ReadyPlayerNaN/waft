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
    NM_WIRELESS_INTERFACE, get_property,
};
use crate::ethernet::refresh_ethernet_state;
use crate::ip_config::{fetch_public_ip, get_device_ip4_config};
use crate::nmrs_adapter;
use crate::state::{
    BluetoothDeviceInfo, CachedIpConfig, EthernetAdapterState, NmState, WiFiAdapterState,
};
use crate::tethering::refresh_tethering_states;
use crate::vpn::{is_vpn_type, refresh_vpn_states};
use waft_plugin::EntityNotifier;
use waft_plugin::lock_or_recover;

fn is_nm_active_connections_change(
    obj_path: &str,
    prop_iface: &str,
    props: &HashMap<String, OwnedValue>,
) -> bool {
    obj_path == NM_PATH && prop_iface == NM_INTERFACE && props.contains_key("ActiveConnections")
}

fn is_wifi_active_access_point_change(
    prop_iface: &str,
    props: &HashMap<String, OwnedValue>,
) -> bool {
    prop_iface == NM_WIRELESS_INTERFACE && props.contains_key("ActiveAccessPoint")
}

fn is_nm_wireless_enabled_change(
    obj_path: &str,
    prop_iface: &str,
    props: &HashMap<String, OwnedValue>,
) -> bool {
    obj_path == NM_PATH && prop_iface == NM_INTERFACE && props.contains_key("WirelessEnabled")
}

pub async fn refresh_wifi_active_access_point(
    nm: &nmrs::NetworkManager,
    state: &Arc<StdMutex<NmState>>,
    device_path: &str,
) -> Result<bool> {
    for _attempt in 0..3 {
        let (interface_name, revision) = {
            let st = lock_or_recover(state);
            let Some(adapter) = st.wifi_adapters.iter().find(|a| a.path == device_path) else {
                return Ok(false);
            };
            (adapter.interface_name.clone(), st.wifi_revision)
        };

        let active_ap = crate::nmrs_adapter::get_active_access_point(nm, &interface_name).await?;

        let mut st = lock_or_recover(state);
        // A signal, action, or scan changed WiFi state while the D-Bus read was
        // in flight. Retry against the newer revision rather than reverting it.
        if st.wifi_revision != revision {
            continue;
        }
        let Some(adapter) = st.wifi_adapters.iter_mut().find(|a| a.path == device_path) else {
            return Ok(false);
        };

        let changed = match active_ap {
            Some(ap_info) => {
                let mut changed = adapter.active_ssid.as_deref() != Some(&ap_info.ssid);
                adapter.active_ssid = Some(ap_info.ssid.clone());

                if let Some(existing) = adapter
                    .access_points
                    .iter_mut()
                    .find(|ap| ap.ssid == ap_info.ssid)
                {
                    if existing.strength != ap_info.strength
                        || existing.secure != ap_info.secure
                        || existing.known != ap_info.known
                        || existing.ap_path != ap_info.ap_path
                        || existing.security_type != ap_info.security_type
                    {
                        *existing = ap_info;
                        changed = true;
                    }
                } else {
                    adapter.access_points.push(ap_info);
                    changed = true;
                }
                changed
            }
            None => adapter.active_ssid.take().is_some(),
        };

        if changed {
            st.wifi_revision = st.wifi_revision.wrapping_add(1);
        }
        return Ok(changed);
    }

    Ok(false)
}

async fn refresh_all_wifi_active_access_points(
    nm: &nmrs::NetworkManager,
    state: &Arc<StdMutex<NmState>>,
) -> Result<bool> {
    let device_paths: Vec<String> = {
        let st = lock_or_recover(state);
        st.wifi_adapters.iter().map(|a| a.path.clone()).collect()
    };

    let mut changed = false;
    for device_path in device_paths {
        changed |= refresh_wifi_active_access_point(nm, state, &device_path).await?;
    }

    Ok(changed)
}

async fn refresh_device_states(
    conn: &Connection,
    nm: &nmrs::NetworkManager,
    state: &Arc<StdMutex<NmState>>,
) -> Result<bool> {
    let devices = nmrs_adapter::discover_devices(nm).await?;
    let wireless_enabled =
        get_property::<bool>(conn, NM_PATH, NM_INTERFACE, "WirelessEnabled").await?;
    let mut changed = false;
    let mut st = lock_or_recover(state);

    for device in devices {
        if let Some(adapter) = st
            .ethernet_adapters
            .iter_mut()
            .find(|adapter| adapter.path == device.path)
            && adapter.device_state != device.device_state
        {
            adapter.device_state = device.device_state;
            changed = true;
        }
        if let Some(device_state) = st
            .bluetooth_devices
            .iter_mut()
            .find(|tracked| tracked.path == device.path)
            && device_state.device_state != device.device_state
        {
            device_state.device_state = device.device_state;
            changed = true;
        }
    }
    let mut wifi_changed = false;
    for adapter in &mut st.wifi_adapters {
        if adapter.enabled != wireless_enabled {
            adapter.enabled = wireless_enabled;
            if !wireless_enabled {
                adapter.active_ssid = None;
                adapter.access_points.clear();
            }
            wifi_changed = true;
            changed = true;
        }
    }
    if wifi_changed {
        st.wifi_revision = st.wifi_revision.wrapping_add(1);
    }

    Ok(changed)
}

async fn resync_network_state(
    conn: &Connection,
    nm: &nmrs::NetworkManager,
    state: &Arc<StdMutex<NmState>>,
    vpn_refresh_lock: &tokio::sync::Mutex<()>,
) -> bool {
    let mut refreshed = false;

    match refresh_vpn_states(conn, nm, state, vpn_refresh_lock).await {
        Ok(()) => refreshed = true,
        Err(error) => error!("[nm] Failed to refresh VPN states: {error}"),
    }
    match refresh_device_states(conn, nm, state).await {
        Ok(changed) => refreshed |= changed,
        Err(error) => error!("[nm] Failed to refresh device states: {error}"),
    }
    match refresh_all_wifi_active_access_points(nm, state).await {
        Ok(changed) => refreshed |= changed,
        Err(error) => error!("[nm] Failed to refresh WiFi state: {error}"),
    }
    match refresh_ethernet_state(conn, nm, state).await {
        Ok(()) => refreshed = true,
        Err(error) => error!("[nm] Failed to refresh ethernet state: {error}"),
    }
    match refresh_tethering_states(conn, nm, state).await {
        Ok(()) => refreshed = true,
        Err(error) => error!("[nm] Failed to refresh tethering state: {error}"),
    }

    refreshed
}

/// Monitor NM D-Bus signals and update shared state accordingly.
pub async fn monitor_nm_signals(
    conn: Connection,
    nm: nmrs::NetworkManager,
    state: Arc<StdMutex<NmState>>,
    notifier: EntityNotifier,
    vpn_refresh_lock: Arc<tokio::sync::Mutex<()>>,
) -> Result<()> {
    let mut monitor_conn = conn;
    let mut monitor_nm = nm;
    loop {
        match monitor_nm_signals_once(
            monitor_conn.clone(),
            monitor_nm.clone(),
            state.clone(),
            notifier.clone(),
            vpn_refresh_lock.clone(),
        )
        .await
        {
            Ok(()) => warn!("[nm] D-Bus signal stream ended; reconnecting"),
            Err(error) => warn!("[nm] D-Bus signal monitor failed: {error}; reconnecting"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        match Connection::system().await {
            Ok(new_conn) => match nmrs::NetworkManager::new().await {
                Ok(new_nm) => {
                    monitor_conn = new_conn;
                    monitor_nm = new_nm;
                }
                Err(error) => warn!("[nm] Failed to recreate NetworkManager client: {error}"),
            },
            Err(error) => warn!("[nm] Failed to recreate system-bus connection: {error}"),
        }
    }
}

async fn monitor_nm_signals_once(
    conn: Connection,
    nm: nmrs::NetworkManager,
    state: Arc<StdMutex<NmState>>,
    notifier: EntityNotifier,
    vpn_refresh_lock: Arc<tokio::sync::Mutex<()>>,
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

    // NetworkManager may not emit a complete set of state signals across
    // suspend/resume. Subscribe to logind so we can take a fresh snapshot when
    // the system wakes instead of waiting for the next user action.
    let resume_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.login1")?
        .interface("org.freedesktop.login1.Manager")?
        .member("PrepareForSleep")?
        .build();
    dbus_proxy
        .add_match_rule(resume_rule)
        .await
        .context("Failed to add PrepareForSleep match rule")?;

    info!("[nm] Listening for NetworkManager signals");
    if resync_network_state(&conn, &nm, &state, &vpn_refresh_lock).await {
        notifier.notify();
    }

    let mut stream = zbus::MessageStream::from(&conn);
    let mut consecutive_stream_errors = 0u8;
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => {
                consecutive_stream_errors = 0;
                m
            }
            Err(e) => {
                consecutive_stream_errors = consecutive_stream_errors.saturating_add(1);
                warn!("[nm] D-Bus stream error: {e}");
                if consecutive_stream_errors >= 3 {
                    anyhow::bail!("D-Bus signal stream failed repeatedly");
                }
                continue;
            }
        };

        let header = msg.header();
        #[allow(clippy::redundant_closure_for_method_calls)]
        let member = header.member().map(|m| m.as_str()).unwrap_or("");
        #[allow(clippy::redundant_closure_for_method_calls)]
        let iface = header.interface().map(|i| i.as_str()).unwrap_or("");
        let obj_path = header
            .path()
            .map(std::string::ToString::to_string)
            .unwrap_or_default();

        match (iface, member) {
            ("org.freedesktop.login1.Manager", "PrepareForSleep") => {
                let Ok(sleeping) = msg.body().deserialize::<bool>() else {
                    continue;
                };
                if sleeping {
                    continue;
                }

                // NM and VPN helpers can still be restoring immediately after
                // resume. Debounce once, then read authoritative state from
                // each subsystem. This is event-driven, not a polling loop.
                let resume_conn = conn.clone();
                let resume_nm = nm.clone();
                let resume_state = state.clone();
                let resume_notifier = notifier.clone();
                let resume_refresh_lock = vpn_refresh_lock.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    if resync_network_state(
                        &resume_conn,
                        &resume_nm,
                        &resume_state,
                        &resume_refresh_lock,
                    )
                    .await
                    {
                        resume_notifier.notify();
                    }
                });
            }
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
                        debug!("[nm] VPN state changed: path={obj_path}, state={state_code}");
                        let refresh_conn = conn.clone();
                        let refresh_nm = nm.clone();
                        let refresh_state = state.clone();
                        let refresh_notifier = notifier.clone();
                        let refresh_lock = vpn_refresh_lock.clone();
                        tokio::spawn(async move {
                            if let Err(error) = refresh_vpn_states(
                                &refresh_conn,
                                &refresh_nm,
                                &refresh_state,
                                &refresh_lock,
                            )
                            .await
                            {
                                error!("[nm] Failed to refresh VPN states: {error}");
                            }
                            refresh_notifier.notify();
                        });
                        changed = true;
                    } else if conn_type == "bluetooth" {
                        debug!("[nm] Tethering state changed: path={obj_path}, state={state_code}");
                        let refresh_conn = conn.clone();
                        let refresh_nm = nm.clone();
                        let refresh_state = state.clone();
                        let refresh_notifier = notifier.clone();
                        tokio::spawn(async move {
                            if let Err(error) =
                                refresh_tethering_states(&refresh_conn, &refresh_nm, &refresh_state)
                                    .await
                            {
                                error!("[nm] Failed to refresh tethering states: {error}");
                            }
                            refresh_notifier.notify();
                        });
                        changed = true;
                    }
                }

                // Handle VPN.Connection.VpnState changes
                if obj_path.contains("/ActiveConnection/")
                    && prop_iface == NM_VPN_CONNECTION_INTERFACE
                    && props.contains_key("VpnState")
                {
                    debug!("[nm] VPN.Connection state changed: {obj_path}");
                    let refresh_conn = conn.clone();
                    let refresh_nm = nm.clone();
                    let refresh_state = state.clone();
                    let refresh_notifier = notifier.clone();
                    let refresh_lock = vpn_refresh_lock.clone();
                    tokio::spawn(async move {
                        if let Err(error) = refresh_vpn_states(
                            &refresh_conn,
                            &refresh_nm,
                            &refresh_state,
                            &refresh_lock,
                        )
                        .await
                        {
                            error!("[nm] Failed to refresh VPN states: {error}");
                        }
                        refresh_notifier.notify();
                    });
                    changed = true;
                }

                if is_nm_wireless_enabled_change(&obj_path, &prop_iface, &props)
                    && let Some(enabled_val) = props.get("WirelessEnabled")
                    && let Ok(enabled) = bool::try_from(enabled_val.clone())
                {
                    let mut st = lock_or_recover(&state);
                    for adapter in &mut st.wifi_adapters {
                        adapter.enabled = enabled;
                        if !enabled {
                            adapter.active_ssid = None;
                            adapter.access_points.clear();
                        }
                    }
                    st.wifi_revision = st.wifi_revision.wrapping_add(1);
                    changed = true;
                }

                // Self-heal on global NM connection graph changes. This covers cases
                // where the per-device signal ordering leaves Waft behind real state.
                if is_nm_active_connections_change(&obj_path, &prop_iface, &props) {
                    debug!("[nm] ActiveConnections changed; refreshing VPN and WiFi state");
                    let refresh_conn = conn.clone();
                    let refresh_nm = nm.clone();
                    let refresh_state = state.clone();
                    let refresh_notifier = notifier.clone();
                    let refresh_lock = vpn_refresh_lock.clone();
                    tokio::spawn(async move {
                        if let Err(error) = refresh_vpn_states(
                            &refresh_conn,
                            &refresh_nm,
                            &refresh_state,
                            &refresh_lock,
                        )
                        .await
                        {
                            error!("[nm] Failed to refresh VPN states: {error}");
                        }
                        if let Err(error) =
                            refresh_all_wifi_active_access_points(&refresh_nm, &refresh_state).await
                        {
                            error!("[nm] Failed to refresh WiFi active APs: {error}");
                        }
                        refresh_notifier.notify();
                    });
                    changed = true;
                }

                // Handle WiFi ActiveAccessPoint changes in both directions.
                // The previous logic only handled disconnect ("/"). Refreshing from NM
                // here makes connect transitions self-healing too.
                if is_wifi_active_access_point_change(&prop_iface, &props) {
                    let refresh_nm = nm.clone();
                    let refresh_state = state.clone();
                    let refresh_path = obj_path.clone();
                    let refresh_notifier = notifier.clone();
                    tokio::spawn(async move {
                        match refresh_wifi_active_access_point(
                            &refresh_nm,
                            &refresh_state,
                            &refresh_path,
                        )
                        .await
                        {
                            Ok(true) => {
                                refresh_notifier.notify();
                            }
                            Ok(false) => {}
                            Err(error) => error!("[nm] Failed to refresh WiFi active AP: {error}"),
                        }
                    });
                }

                if changed {
                    notifier.notify();
                }
            }

            (iface_str, "DeviceAdded") if iface_str == NM_INTERFACE => {
                if let Ok(path) = msg.body().deserialize::<ObjectPath<'_>>() {
                    let device_path = path.to_string();
                    info!("[nm] Device added: {device_path}");

                    // Read device type first, then branch without holding locks across awaits
                    let device_type: u32 =
                        get_property(&conn, &device_path, NM_DEVICE_INTERFACE, "DeviceType")
                            .await
                            .unwrap_or(0);
                    let mut refresh_wifi_path = None;

                    match device_type {
                        DEVICE_TYPE_ETHERNET | DEVICE_TYPE_WIFI => {
                            if let Ok(Some(info)) =
                                nmrs_adapter::get_device_info_by_path(&nm, &device_path).await
                            {
                                let wifi_enabled = if info.device_type == DEVICE_TYPE_WIFI {
                                    get_property::<bool>(
                                        &conn,
                                        NM_PATH,
                                        NM_INTERFACE,
                                        "WirelessEnabled",
                                    )
                                    .await
                                    .unwrap_or(true)
                                } else {
                                    true
                                };
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
                                    DEVICE_TYPE_WIFI
                                        if !st
                                            .wifi_adapters
                                            .iter()
                                            .any(|a| a.path == info.path) =>
                                    {
                                        refresh_wifi_path = Some(info.path.clone());
                                        st.wifi_adapters.push(WiFiAdapterState {
                                            path: info.path,
                                            interface_name: info.interface_name,
                                            enabled: wifi_enabled,
                                            busy: false,
                                            active_ssid: None,
                                            access_points: Vec::new(),
                                            scanning: false,
                                        });
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
                            info!("[nm] Bluetooth device added: {device_path} state={bt_state}");
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
                            if let Err(e) = refresh_tethering_states(&conn, &nm, &state).await {
                                error!("[nm] Failed to refresh tethering states: {e}");
                            }
                        }
                        _ => {}
                    }

                    if let Some(wifi_path) = refresh_wifi_path
                        && let Err(error) =
                            refresh_wifi_active_access_point(&nm, &state, &wifi_path).await
                    {
                        error!("[nm] Failed to refresh added WiFi adapter: {error}");
                    }
                    notifier.notify();
                }
            }

            (iface_str, "DeviceRemoved") if iface_str == NM_INTERFACE => {
                if let Ok(path) = msg.body().deserialize::<ObjectPath<'_>>() {
                    let device_path = path.to_string();
                    info!("[nm] Device removed: {device_path}");

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
                    let mut refresh_ssid_for: Option<String> = None;
                    let mut clear_ip = false;

                    {
                        let mut st = match state.lock() {
                            Ok(g) => g,
                            Err(e) => {
                                warn!("[nm] Mutex poisoned, recovering: {e}");
                                e.into_inner()
                            }
                        };

                        let path_known = st.ethernet_adapters.iter().any(|a| a.path == obj_path)
                            || st.wifi_adapters.iter().any(|a| a.path == obj_path)
                            || st.bluetooth_devices.iter().any(|d| d.path == obj_path);

                        if !path_known {
                            warn!(
                                "[nm] StateChanged for unknown device path: {obj_path} (state={new_state})"
                            );
                        }

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

                        // Update WiFi state from the authoritative ActiveAccessPoint
                        // property after the device transition settles. Do not clear
                        // active_ssid from the signal itself: rapid transitions can
                        // deliver an older StateChanged after a newer connection.
                        if let Some(adapter) = st.wifi_adapters.iter().find(|a| a.path == obj_path)
                        {
                            debug!(
                                "[nm] WiFi {} device state change: {}",
                                adapter.interface_name, new_state
                            );
                            refresh_ssid_for = Some(adapter.path.clone());
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
                        if let Err(e) = refresh_ethernet_state(&conn, &nm, &state).await {
                            warn!("[nm] Failed to refresh ethernet state: {e}");
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
                            st.ethernet_adapters
                                .iter()
                                .any(super::state::EthernetAdapterState::is_connected)
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

                    // Populate active_ssid and access_points when WiFi device reaches
                    // activated state. Keep the small delay, but use the shared refresh
                    // helper so later ActiveAccessPoint changes can self-heal too.
                    if let Some(device_path) = refresh_ssid_for {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        match refresh_wifi_active_access_point(&nm, &state, &device_path).await {
                            Ok(true) => {
                                notifier.notify();
                            }
                            Ok(false) => {}
                            Err(e) => error!("[nm] Failed to refresh WiFi active AP: {e}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn owned_props(entries: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn detects_nm_active_connections_refresh_trigger() {
        let props = owned_props(&[(
            "ActiveConnections",
            zbus::zvariant::Value::from(Vec::<String>::new())
                .try_into()
                .expect("valid value"),
        )]);
        assert!(is_nm_active_connections_change(
            NM_PATH,
            NM_INTERFACE,
            &props
        ));
        assert!(!is_nm_active_connections_change(
            "/org/freedesktop/NetworkManager/Devices/1",
            NM_INTERFACE,
            &props
        ));
    }

    #[test]
    fn detects_wifi_active_access_point_refresh_trigger() {
        let props = owned_props(&[(
            "ActiveAccessPoint",
            zbus::zvariant::Value::from("/")
                .try_into()
                .expect("valid value"),
        )]);
        assert!(is_wifi_active_access_point_change(
            NM_WIRELESS_INTERFACE,
            &props
        ));
        assert!(!is_wifi_active_access_point_change(
            NM_DEVICE_INTERFACE,
            &props
        ));
    }

    #[test]
    fn detects_nm_wireless_enabled_refresh_trigger() {
        let props = owned_props(&[(
            "WirelessEnabled",
            zbus::zvariant::Value::from(true)
                .try_into()
                .expect("valid value"),
        )]);
        assert!(is_nm_wireless_enabled_change(NM_PATH, NM_INTERFACE, &props));
        assert!(!is_nm_wireless_enabled_change(
            "/org/freedesktop/NetworkManager/Devices/1",
            NM_INTERFACE,
            &props
        ));
    }
}
