//! NetworkManager daemon - WiFi, Wired, and VPN network management.
//!
//! Provides three NamedWidget entries:
//! - WiFi toggle with expandable network list (weight 100)
//! - Wired toggle with expandable IP details (weight 101)
//! - VPN toggle with expandable connection list (weight 103)
//!
//! Monitors NetworkManager D-Bus signals for device/connection state changes.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin_sdk::*;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};
use zbus::Connection;

// ---------------------------------------------------------------------------
// D-Bus constants
// ---------------------------------------------------------------------------

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_INTERFACE: &str = "org.freedesktop.NetworkManager";
const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_SETTINGS_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings";
const NM_CONNECTION_ACTIVE_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const NM_VPN_CONNECTION_INTERFACE: &str = "org.freedesktop.NetworkManager.VPN.Connection";

const DEVICE_TYPE_ETHERNET: u32 = 1;
const DEVICE_TYPE_WIFI: u32 = 2;

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum VpnState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

impl VpnState {
    fn from_active_state(code: u32) -> Self {
        match code {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Disconnecting,
            _ => Self::Disconnected,
        }
    }
}

#[derive(Debug, Clone)]
struct AccessPointInfo {
    ssid: String,
    strength: u8,
    secure: bool,
}

#[derive(Debug, Clone)]
struct WiFiAdapterState {
    path: String,
    interface_name: String,
    enabled: bool,
    busy: bool,
    active_ssid: Option<String>,
    /// Known networks (have saved connection profiles).
    access_points: Vec<AccessPointInfo>,
    scanning: bool,
}

#[derive(Debug, Clone)]
struct EthernetAdapterState {
    path: String,
    interface_name: String,
    device_state: u32,
}

impl EthernetAdapterState {
    fn is_connected(&self) -> bool {
        self.device_state == 100
    }

    fn is_enabled(&self) -> bool {
        self.device_state >= 20
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VpnConnectionInfo {
    path: String,
    uuid: String,
    name: String,
    state: VpnState,
    /// Active connection D-Bus path when connected/connecting.
    active_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct NmState {
    wifi_adapters: Vec<WiFiAdapterState>,
    ethernet_adapters: Vec<EthernetAdapterState>,
    vpn_connections: Vec<VpnConnectionInfo>,
}

// ---------------------------------------------------------------------------
// D-Bus helper functions
// ---------------------------------------------------------------------------

async fn get_property<T>(
    conn: &Connection,
    path: &str,
    interface: &str,
    property: &str,
) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        path,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .context("Failed to create Properties proxy")?;

    let (value,): (OwnedValue,) = proxy
        .call("Get", &(interface, property))
        .await
        .with_context(|| format!("Failed to get property {}.{}", interface, property))?;

    T::try_from(value).map_err(|e| anyhow::anyhow!("Failed to convert property: {}", e))
}

fn is_virtual_interface(name: &str) -> bool {
    let virtual_prefixes = ["docker", "veth", "br-", "virbr", "vnet"];
    virtual_prefixes
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn get_wifi_icon(strength: Option<u8>, enabled: bool, connected: bool) -> &'static str {
    if !enabled || !connected {
        return "network-wireless-symbolic";
    }
    match strength {
        Some(s) if s > 75 => "network-wireless-signal-excellent-symbolic",
        Some(s) if s > 50 => "network-wireless-signal-good-symbolic",
        Some(s) if s > 25 => "network-wireless-signal-ok-symbolic",
        Some(_) => "network-wireless-signal-weak-symbolic",
        None => "network-wireless-symbolic",
    }
}

fn wired_icon(state: &EthernetAdapterState) -> &'static str {
    if !state.is_enabled() {
        "network-wired-offline-symbolic"
    } else if state.is_connected() {
        "network-wired-symbolic"
    } else {
        "network-wired-disconnected-symbolic"
    }
}

fn wired_details(state: &EthernetAdapterState) -> &'static str {
    if !state.is_enabled() {
        "Disabled"
    } else if state.is_connected() {
        "Connected"
    } else {
        "Disconnected"
    }
}

// ---------------------------------------------------------------------------
// D-Bus device discovery (nmrs-based)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DeviceInfo {
    path: String,
    device_type: u32,
    interface_name: String,
    device_state: u32,
}

async fn discover_devices(nm: &nmrs::NetworkManager) -> Result<Vec<DeviceInfo>> {
    let devices = nm
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list devices: {}", e))?;

    let mut result = Vec::new();
    for device in devices {
        let device_type = match device.device_type {
            nmrs::DeviceType::Ethernet => DEVICE_TYPE_ETHERNET,
            nmrs::DeviceType::Wifi => DEVICE_TYPE_WIFI,
            _ => continue,
        };

        if is_virtual_interface(&device.interface) {
            continue;
        }

        if !device.managed.unwrap_or(false) {
            continue;
        }

        let device_state = match device.state {
            nmrs::DeviceState::Unmanaged => 10,
            nmrs::DeviceState::Unavailable => 20,
            nmrs::DeviceState::Disconnected => 30,
            nmrs::DeviceState::Prepare => 40,
            nmrs::DeviceState::Config => 50,
            nmrs::DeviceState::Activated => 100,
            nmrs::DeviceState::Deactivating => 110,
            nmrs::DeviceState::Failed => 120,
            nmrs::DeviceState::Other(code) => code,
            _ => 0,
        };

        result.push(DeviceInfo {
            path: device.path.clone(),
            device_type,
            interface_name: device.interface.clone(),
            device_state,
        });
    }

    Ok(result)
}

/// Get device info for a specific device path using raw D-Bus.
async fn get_device_info_dbus(conn: &Connection, device_path: &str) -> Result<Option<DeviceInfo>> {
    let device_type: u32 =
        match get_property(conn, device_path, NM_DEVICE_INTERFACE, "DeviceType").await {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

    if device_type != DEVICE_TYPE_ETHERNET && device_type != DEVICE_TYPE_WIFI {
        return Ok(None);
    }

    let interface_name: String =
        get_property(conn, device_path, NM_DEVICE_INTERFACE, "Interface").await?;

    if is_virtual_interface(&interface_name) {
        return Ok(None);
    }

    let managed: bool = get_property(conn, device_path, NM_DEVICE_INTERFACE, "Managed")
        .await
        .unwrap_or(false);
    if !managed {
        return Ok(None);
    }

    let device_state: u32 = get_property(conn, device_path, NM_DEVICE_INTERFACE, "State")
        .await
        .unwrap_or(0);

    Ok(Some(DeviceInfo {
        path: device_path.to_string(),
        device_type,
        interface_name,
        device_state,
    }))
}

// ---------------------------------------------------------------------------
// WiFi operations (raw D-Bus for saved connections)
// ---------------------------------------------------------------------------

/// Scan and list known WiFi networks using nmrs (called from background task, not Send-required).
async fn scan_and_list_known_networks(
    nm: &nmrs::NetworkManager,
    conn: &Connection,
) -> Result<Vec<AccessPointInfo>> {
    // Trigger scan
    if let Err(e) = nm
        .scan_networks()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
    {
        warn!("[nm] Failed to trigger scan: {}", e);
    }

    // Wait for scan results
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let networks = nm
        .list_networks()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list networks: {}", e))?;

    let mut by_ssid: HashMap<String, AccessPointInfo> = HashMap::new();

    for network in &networks {
        if network.ssid.is_empty() {
            continue;
        }

        // Only include networks with saved connection profiles
        match get_connections_for_ssid(conn, &network.ssid).await {
            Ok(connections) if !connections.is_empty() => {}
            _ => {
                debug!("[nm] Skipping network {} (no saved profile)", network.ssid);
                continue;
            }
        }

        let strength = network.strength.unwrap_or(0);
        let secure = network.secured;

        match by_ssid.get(&network.ssid) {
            Some(existing) if existing.strength >= strength => {
                // Keep existing (stronger or equal)
            }
            _ => {
                by_ssid.insert(
                    network.ssid.clone(),
                    AccessPointInfo {
                        ssid: network.ssid.clone(),
                        strength,
                        secure,
                    },
                );
            }
        }
    }

    let mut result: Vec<AccessPointInfo> = by_ssid.into_values().collect();
    // Sort by signal strength (strongest first)
    result.sort_by(|a, b| b.strength.cmp(&a.strength));
    Ok(result)
}

/// Find saved WiFi connections matching the given SSID.
async fn get_connections_for_ssid(conn: &Connection, ssid: &str) -> Result<Vec<String>> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_INTERFACE)
        .await
        .context("Failed to create Settings proxy")?;

    let (settings_paths,): (Vec<OwnedObjectPath>,) = proxy
        .call("ListConnections", &())
        .await
        .context("Failed to list connections")?;

    let mut matching = Vec::new();

    for settings_path in settings_paths {
        let path_str = settings_path.as_str();

        let conn_proxy = zbus::Proxy::new(
            conn,
            NM_SERVICE,
            path_str,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        let (settings,): (HashMap<String, HashMap<String, OwnedValue>>,) =
            conn_proxy.call("GetSettings", &()).await?;

        if let Some(wireless) = settings.get("802-11-wireless") {
            if let Some(ssid_value) = wireless.get("ssid") {
                if let Ok(ssid_bytes) = <Vec<u8>>::try_from(ssid_value.clone()) {
                    let connection_ssid = String::from_utf8_lossy(&ssid_bytes);
                    if connection_ssid == ssid {
                        matching.push(path_str.to_string());
                    }
                }
            }
        }
    }

    Ok(matching)
}

/// Activate a connection on a device.
async fn activate_connection(
    conn: &Connection,
    connection_path: Option<&str>,
    device_path: &str,
    specific_object: Option<&str>,
) -> Result<String> {
    let conn_path = connection_path.unwrap_or("/");
    let specific = specific_object.unwrap_or("/");

    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let (active_conn_path,): (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &(conn_path, device_path, specific))
        .await
        .context("Failed to activate connection")?;

    Ok(active_conn_path.to_string())
}

/// Disconnect a specific device.
async fn disconnect_device(conn: &Connection, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, device_path, NM_DEVICE_INTERFACE)
        .await
        .context("Failed to create Device proxy")?;

    let _: () = proxy
        .call("Disconnect", &())
        .await
        .context("Failed to disconnect device")?;

    Ok(())
}

/// Set WiFi enabled via raw D-Bus (avoids nmrs non-Send futures).
async fn set_wifi_enabled_dbus(conn: &Connection, enabled: bool) -> Result<()> {
    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        NM_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .context("Failed to create Properties proxy")?;

    let v = zbus::zvariant::Value::from(enabled);
    let _: () = proxy
        .call("Set", &(NM_INTERFACE, "WirelessEnabled", v))
        .await
        .context("Failed to set WirelessEnabled")?;

    Ok(())
}

/// Connect wired via raw D-Bus (ActivateConnection with "/" for auto-activate).
async fn connect_wired_dbus(conn: &Connection) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let _: (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &("/", "/", "/"))
        .await
        .context("Failed to auto-activate wired connection")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// VPN operations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct VpnProfileInfo {
    path: String,
    uuid: String,
    name: String,
}

async fn get_vpn_profiles(conn: &Connection) -> Result<Vec<VpnProfileInfo>> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_INTERFACE)
        .await
        .context("Failed to create Settings proxy")?;

    let (settings_paths,): (Vec<OwnedObjectPath>,) = proxy
        .call("ListConnections", &())
        .await
        .context("Failed to list connections")?;

    let mut vpn_profiles = Vec::new();

    for settings_path in settings_paths {
        let path_str = settings_path.as_str();

        let conn_proxy = zbus::Proxy::new(
            conn,
            NM_SERVICE,
            path_str,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        let (settings,): (HashMap<String, HashMap<String, OwnedValue>>,) =
            conn_proxy.call("GetSettings", &()).await?;

        if let Some(connection) = settings.get("connection") {
            if let Some(conn_type) = connection.get("type") {
                if let Ok(type_str) = String::try_from(conn_type.clone()) {
                    if type_str == "vpn" {
                        let name = connection
                            .get("id")
                            .and_then(|v| String::try_from(v.clone()).ok())
                            .unwrap_or_else(|| "Unknown VPN".to_string());
                        let uuid = connection
                            .get("uuid")
                            .and_then(|v| String::try_from(v.clone()).ok())
                            .unwrap_or_default();

                        vpn_profiles.push(VpnProfileInfo {
                            path: path_str.to_string(),
                            uuid,
                            name,
                        });
                    }
                }
            }
        }
    }

    Ok(vpn_profiles)
}

/// Get active VPN connections: (active_path, connection_path, uuid, state_code).
async fn get_active_vpn_connections(
    conn: &Connection,
) -> Result<Vec<(String, String, String, u32)>> {
    let active_connections: Vec<OwnedObjectPath> = match get_property(
        conn,
        NM_PATH,
        NM_INTERFACE,
        "ActiveConnections",
    )
    .await
    {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };

    let mut vpn_active = Vec::new();

    for active_conn_path in active_connections {
        let path_str = active_conn_path.as_str();

        let conn_type: String =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "Type").await {
                Ok(t) => t,
                Err(_) => continue,
            };

        if conn_type != "vpn" {
            continue;
        }

        let connection_path: OwnedObjectPath = match get_property(
            conn,
            path_str,
            NM_CONNECTION_ACTIVE_INTERFACE,
            "Connection",
        )
        .await
        {
            Ok(p) => p,
            Err(_) => continue,
        };

        let uuid: String =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "Uuid").await {
                Ok(u) => u,
                Err(_) => continue,
            };

        let state: u32 =
            match get_property(conn, path_str, NM_CONNECTION_ACTIVE_INTERFACE, "State").await {
                Ok(s) => s,
                Err(_) => 0,
            };

        vpn_active.push((
            path_str.to_string(),
            connection_path.to_string(),
            uuid,
            state,
        ));
    }

    Ok(vpn_active)
}

async fn activate_vpn(conn: &Connection, connection_path: &str) -> Result<String> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let conn_obj = ObjectPath::try_from(connection_path)?;
    let device_obj = ObjectPath::try_from("/")?;
    let specific_obj = ObjectPath::try_from("/")?;

    let (active_conn_path,): (OwnedObjectPath,) = proxy
        .call("ActivateConnection", &(conn_obj, device_obj, specific_obj))
        .await
        .context("Failed to activate VPN connection")?;

    Ok(active_conn_path.to_string())
}

async fn deactivate_vpn(conn: &Connection, active_connection_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let active_obj = ObjectPath::try_from(active_connection_path)?;
    let _: () = proxy
        .call("DeactivateConnection", &(active_obj,))
        .await
        .context("Failed to deactivate VPN connection")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct NetworkManagerDaemon {
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    /// Channel to request WiFi scan from background task.
    scan_tx: tokio::sync::mpsc::Sender<()>,
}

impl NetworkManagerDaemon {
    async fn new(
        scan_tx: tokio::sync::mpsc::Sender<()>,
    ) -> Result<(Self, nmrs::NetworkManager)> {
        let nm = nmrs::NetworkManager::new()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create NetworkManager: {}", e))?;

        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

        let mut state = NmState::default();

        // Discover devices
        match discover_devices(&nm).await {
            Ok(devices) => {
                info!("[nm] Found {} network devices", devices.len());
                for device in devices {
                    debug!(
                        "[nm] Device: {} ({}) type={} state={}",
                        device.interface_name, device.path, device.device_type, device.device_state
                    );
                    match device.device_type {
                        DEVICE_TYPE_ETHERNET => {
                            state.ethernet_adapters.push(EthernetAdapterState {
                                path: device.path,
                                interface_name: device.interface_name,
                                device_state: device.device_state,
                            });
                        }
                        DEVICE_TYPE_WIFI => {
                            state.wifi_adapters.push(WiFiAdapterState {
                                path: device.path,
                                interface_name: device.interface_name,
                                enabled: true,
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
            Err(e) => {
                error!("[nm] Failed to discover devices: {}", e);
            }
        }

        // Discover VPN connections
        match get_vpn_profiles(&conn).await {
            Ok(profiles) => {
                info!("[nm] Found {} VPN profiles", profiles.len());

                let active_vpns = get_active_vpn_connections(&conn).await.unwrap_or_default();

                for profile in profiles {
                    let active_info = active_vpns
                        .iter()
                        .find(|(_, _, uuid, _)| *uuid == profile.uuid);

                    let vpn_state = active_info
                        .map(|(_, _, _, state_code)| VpnState::from_active_state(*state_code))
                        .unwrap_or(VpnState::Disconnected);

                    let active_path = active_info.map(|(ap, _, _, _)| ap.clone());

                    debug!(
                        "[nm] VPN {}: path={}, state={:?}",
                        profile.name, profile.path, vpn_state
                    );

                    state.vpn_connections.push(VpnConnectionInfo {
                        path: profile.path,
                        uuid: profile.uuid,
                        name: profile.name,
                        state: vpn_state,
                        active_path,
                    });
                }
            }
            Err(e) => {
                error!("[nm] Failed to get VPN profiles: {}", e);
            }
        }

        let daemon = Self {
            conn,
            state: Arc::new(StdMutex::new(state)),
            scan_tx,
        };

        Ok((daemon, nm))
    }

    fn shared_state(&self) -> Arc<StdMutex<NmState>> {
        self.state.clone()
    }

    // -----------------------------------------------------------------------
    // Widget building
    // -----------------------------------------------------------------------

    fn build_widgets(state: &NmState) -> Vec<NamedWidget> {
        let mut widgets = Vec::new();

        // WiFi adapters
        for adapter in &state.wifi_adapters {
            widgets.push(Self::build_wifi_widget(adapter));
        }

        // Ethernet adapters
        for adapter in &state.ethernet_adapters {
            widgets.push(Self::build_wired_widget(adapter));
        }

        // VPN (single widget for all VPN connections)
        if !state.vpn_connections.is_empty() {
            widgets.push(Self::build_vpn_widget(&state.vpn_connections));
        }

        widgets
    }

    fn build_wifi_widget(adapter: &WiFiAdapterState) -> NamedWidget {
        let connected = adapter.active_ssid.is_some();
        let signal_strength = if connected {
            adapter
                .access_points
                .iter()
                .find(|ap| Some(&ap.ssid) == adapter.active_ssid.as_ref())
                .map(|ap| ap.strength)
        } else {
            None
        };

        let icon = get_wifi_icon(signal_strength, adapter.enabled, connected);

        let details = if !adapter.enabled {
            Some("Disabled".to_string())
        } else if let Some(ref ssid) = adapter.active_ssid {
            Some(ssid.clone())
        } else if !adapter.access_points.is_empty() {
            let count = adapter.access_points.len();
            Some(format!(
                "{} network{} available",
                count,
                if count == 1 { "" } else { "s" }
            ))
        } else {
            None
        };

        // Build network list as expanded content
        let expanded_content = if !adapter.access_points.is_empty() || connected {
            let mut container = ContainerBuilder::new(Orientation::Vertical).spacing(4);

            // Show available networks sorted by signal strength
            for ap in &adapter.access_points {
                let is_active = adapter.active_ssid.as_deref() == Some(&ap.ssid);
                let ap_icon = get_wifi_icon(Some(ap.strength), true, true);

                let mut row = MenuRowBuilder::new(&ap.ssid).icon(ap_icon);

                if is_active {
                    row = row.trailing(Widget::Checkmark { visible: true });
                }

                if ap.secure {
                    row = row.sublabel("Secured");
                }

                row = row.on_click(format!("connect_wifi:{}", ap.ssid));
                container = container.child(row.build());
            }

            // If connected, add disconnect option
            if connected {
                let disconnect_row = MenuRowBuilder::new("Disconnect")
                    .icon("network-offline-symbolic")
                    .on_click(format!("disconnect_wifi:{}", adapter.path))
                    .build();
                container = container.child(disconnect_row);
            }

            Some(container.build())
        } else {
            None
        };

        let mut toggle = FeatureToggleBuilder::new(format!("Wi-Fi ({})", adapter.interface_name))
            .icon(icon)
            .active(adapter.enabled)
            .busy(adapter.busy)
            .on_toggle("toggle_wifi");

        if let Some(d) = &details {
            toggle = toggle.details(d);
        }

        if let Some(content) = expanded_content {
            toggle = toggle.expanded_content(content);
        } else {
            toggle = toggle.expandable(true);
        }

        NamedWidget {
            id: format!("networkmanager:wifi:{}", adapter.path),
            weight: 100,
            widget: toggle.build(),
        }
    }

    fn build_wired_widget(adapter: &EthernetAdapterState) -> NamedWidget {
        let icon = wired_icon(adapter);
        let details = wired_details(adapter);

        let toggle =
            FeatureToggleBuilder::new(format!("Wired ({})", adapter.interface_name))
                .icon(icon)
                .active(adapter.is_connected())
                .details(details)
                .on_toggle(format!("toggle_wired:{}", adapter.path))
                .expandable(true);

        NamedWidget {
            id: format!("networkmanager:wired:{}", adapter.path),
            weight: 101,
            widget: toggle.build(),
        }
    }

    fn build_vpn_widget(vpn_connections: &[VpnConnectionInfo]) -> NamedWidget {
        // Determine overall VPN state
        let (connected_name, overall_state) = Self::derive_vpn_state(vpn_connections);
        let any_active = overall_state != VpnState::Disconnected;

        let icon = if any_active {
            "network-vpn-symbolic"
        } else {
            "network-vpn-disconnected-symbolic"
        };

        let details = match &overall_state {
            VpnState::Connected => connected_name.clone(),
            VpnState::Connecting => Some("Connecting...".to_string()),
            VpnState::Disconnecting => Some("Disconnecting...".to_string()),
            VpnState::Disconnected => None,
        };

        // Build VPN connection list as expanded content
        let mut container = ContainerBuilder::new(Orientation::Vertical).spacing(4);

        for vpn in vpn_connections {
            let sublabel = match &vpn.state {
                VpnState::Connecting => Some("Connecting...".to_string()),
                VpnState::Connected => Some("Connected".to_string()),
                VpnState::Disconnecting => Some("Disconnecting...".to_string()),
                VpnState::Disconnected => None,
            };

            let is_busy = matches!(vpn.state, VpnState::Connecting | VpnState::Disconnecting);

            let trailing = if is_busy {
                Widget::Spinner { spinning: true }
            } else {
                let is_connected = vpn.state == VpnState::Connected;
                let action_id = if is_connected {
                    format!("disconnect_vpn:{}", vpn.path)
                } else {
                    format!("connect_vpn:{}", vpn.path)
                };
                SwitchBuilder::new()
                    .active(is_connected)
                    .on_toggle(action_id)
                    .build()
            };

            let mut row = MenuRowBuilder::new(&vpn.name)
                .icon("network-vpn-symbolic")
                .trailing(trailing);

            if let Some(ref sub) = sublabel {
                row = row.sublabel(sub);
            }

            // Click action: toggle connection
            let click_action = if vpn.state == VpnState::Connected {
                format!("disconnect_vpn:{}", vpn.path)
            } else if vpn.state == VpnState::Disconnected {
                format!("connect_vpn:{}", vpn.path)
            } else {
                String::new()
            };

            if !click_action.is_empty() {
                row = row.on_click(click_action);
            }

            container = container.child(row.build());
        }

        let mut toggle = FeatureToggleBuilder::new("VPN")
            .icon(icon)
            .active(any_active)
            .on_toggle("toggle_vpn")
            .expanded_content(container.build());

        if let Some(d) = &details {
            toggle = toggle.details(d);
        }

        NamedWidget {
            id: "networkmanager:vpn".to_string(),
            weight: 103,
            widget: toggle.build(),
        }
    }

    fn derive_vpn_state(connections: &[VpnConnectionInfo]) -> (Option<String>, VpnState) {
        for conn in connections {
            match conn.state {
                VpnState::Connected => return (Some(conn.name.clone()), VpnState::Connected),
                VpnState::Connecting => return (Some(conn.name.clone()), VpnState::Connecting),
                VpnState::Disconnecting => {
                    return (Some(conn.name.clone()), VpnState::Disconnecting)
                }
                VpnState::Disconnected => {}
            }
        }
        (None, VpnState::Disconnected)
    }
}

// ---------------------------------------------------------------------------
// PluginDaemon implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl PluginDaemon for NetworkManagerDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = self.state.lock().unwrap();
        Self::build_widgets(&state)
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let action_id = action.id.as_str();

        if action_id == "toggle_wifi" {
            // Toggle WiFi enabled state
            let current_enabled = {
                let state = self.state.lock().unwrap();
                state
                    .wifi_adapters
                    .first()
                    .map(|a| a.enabled)
                    .unwrap_or(true)
            };

            let new_enabled = !current_enabled;

            // Set busy
            {
                let mut state = self.state.lock().unwrap();
                for adapter in &mut state.wifi_adapters {
                    adapter.busy = true;
                }
            }

            // Use raw D-Bus to avoid nmrs non-Send futures in handle_action
            if let Err(e) = set_wifi_enabled_dbus(&self.conn, new_enabled).await {
                error!("[nm] Failed to set WiFi enabled: {}", e);
                let mut state = self.state.lock().unwrap();
                for adapter in &mut state.wifi_adapters {
                    adapter.busy = false;
                }
                return Err(e.into());
            }

            {
                let mut state = self.state.lock().unwrap();
                for adapter in &mut state.wifi_adapters {
                    adapter.enabled = new_enabled;
                    adapter.busy = false;
                    if !new_enabled {
                        adapter.active_ssid = None;
                        adapter.access_points.clear();
                    }
                }
            }

            // If enabling WiFi, trigger a scan
            if new_enabled {
                let _ = self.scan_tx.send(()).await;
            }
        } else if let Some(ssid) = action_id.strip_prefix("connect_wifi:") {
            info!("[nm] Connecting to WiFi: {}", ssid);

            // Find saved connection for this SSID
            let connections = get_connections_for_ssid(&self.conn, ssid).await?;
            if let Some(conn_path) = connections.first() {
                // Find the WiFi device path
                let device_path = {
                    let state = self.state.lock().unwrap();
                    state.wifi_adapters.first().map(|a| a.path.clone())
                };

                if let Some(ref device_path) = device_path {
                    match activate_connection(&self.conn, Some(conn_path), device_path, None).await
                    {
                        Ok(_) => {
                            info!("[nm] WiFi connection activated for {}", ssid);
                            let mut state = self.state.lock().unwrap();
                            for adapter in &mut state.wifi_adapters {
                                if adapter.path == *device_path {
                                    adapter.active_ssid = Some(ssid.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            error!("[nm] Failed to activate WiFi: {}", e);
                            return Err(e.into());
                        }
                    }
                }
            } else {
                warn!("[nm] No saved connection found for SSID: {}", ssid);
            }
        } else if let Some(device_path) = action_id.strip_prefix("disconnect_wifi:") {
            info!("[nm] Disconnecting WiFi: {}", device_path);

            if let Err(e) = disconnect_device(&self.conn, device_path).await {
                error!("[nm] Failed to disconnect WiFi: {}", e);
                return Err(e.into());
            }

            {
                let mut state = self.state.lock().unwrap();
                for adapter in &mut state.wifi_adapters {
                    if adapter.path == device_path {
                        adapter.active_ssid = None;
                    }
                }
            }
        } else if let Some(device_path) = action_id.strip_prefix("toggle_wired:") {
            let is_connected = {
                let state = self.state.lock().unwrap();
                state
                    .ethernet_adapters
                    .iter()
                    .find(|a| a.path == device_path)
                    .map(|a| a.is_connected())
                    .unwrap_or(false)
            };

            if is_connected {
                info!("[nm] Disconnecting wired: {}", device_path);
                if let Err(e) = disconnect_device(&self.conn, device_path).await {
                    error!("[nm] Failed to disconnect wired: {}", e);
                    return Err(e.into());
                }
            } else {
                info!("[nm] Connecting wired");
                // Use raw D-Bus to auto-activate wired
                if let Err(e) = connect_wired_dbus(&self.conn).await {
                    error!("[nm] Failed to connect wired: {}", e);
                    return Err(e.into());
                }
            }
        } else if action_id == "toggle_vpn" {
            // If any VPN is connected, disconnect all. Otherwise, do nothing (user picks from menu).
            let active_vpns: Vec<(String, String)> = {
                let state = self.state.lock().unwrap();
                state
                    .vpn_connections
                    .iter()
                    .filter(|v| v.state == VpnState::Connected)
                    .filter_map(|v| {
                        v.active_path
                            .as_ref()
                            .map(|ap| (v.path.clone(), ap.clone()))
                    })
                    .collect()
            };

            if active_vpns.is_empty() {
                debug!("[nm] No active VPNs to disconnect");
            } else {
                for (conn_path, active_path) in active_vpns {
                    // Set disconnecting state
                    {
                        let mut state = self.state.lock().unwrap();
                        if let Some(vpn) = state
                            .vpn_connections
                            .iter_mut()
                            .find(|v| v.path == conn_path)
                        {
                            vpn.state = VpnState::Disconnecting;
                        }
                    }

                    if let Err(e) = deactivate_vpn(&self.conn, &active_path).await {
                        error!("[nm] Failed to disconnect VPN {}: {}", conn_path, e);
                        let mut state = self.state.lock().unwrap();
                        if let Some(vpn) = state
                            .vpn_connections
                            .iter_mut()
                            .find(|v| v.path == conn_path)
                        {
                            vpn.state = VpnState::Connected;
                        }
                    }
                }
            }
        } else if let Some(conn_path) = action_id.strip_prefix("connect_vpn:") {
            info!("[nm] Connecting VPN: {}", conn_path);

            // Set connecting state
            {
                let mut state = self.state.lock().unwrap();
                if let Some(vpn) = state
                    .vpn_connections
                    .iter_mut()
                    .find(|v| v.path == conn_path)
                {
                    vpn.state = VpnState::Connecting;
                }
            }

            match activate_vpn(&self.conn, conn_path).await {
                Ok(active_path) => {
                    info!(
                        "[nm] VPN connection initiated: {} -> {}",
                        conn_path, active_path
                    );
                    let mut state = self.state.lock().unwrap();
                    if let Some(vpn) = state
                        .vpn_connections
                        .iter_mut()
                        .find(|v| v.path == conn_path)
                    {
                        vpn.active_path = Some(active_path);
                    }
                }
                Err(e) => {
                    error!("[nm] Failed to connect VPN: {}", e);
                    let mut state = self.state.lock().unwrap();
                    if let Some(vpn) = state
                        .vpn_connections
                        .iter_mut()
                        .find(|v| v.path == conn_path)
                    {
                        vpn.state = VpnState::Disconnected;
                    }
                    return Err(e.into());
                }
            }
        } else if let Some(conn_path) = action_id.strip_prefix("disconnect_vpn:") {
            info!("[nm] Disconnecting VPN: {}", conn_path);

            let active_path = {
                let state = self.state.lock().unwrap();
                state
                    .vpn_connections
                    .iter()
                    .find(|v| v.path == conn_path)
                    .and_then(|v| v.active_path.clone())
            };

            if let Some(ref active_path) = active_path {
                // Set disconnecting state
                {
                    let mut state = self.state.lock().unwrap();
                    if let Some(vpn) = state
                        .vpn_connections
                        .iter_mut()
                        .find(|v| v.path == conn_path)
                    {
                        vpn.state = VpnState::Disconnecting;
                    }
                }

                if let Err(e) = deactivate_vpn(&self.conn, active_path).await {
                    error!("[nm] Failed to disconnect VPN: {}", e);
                    let mut state = self.state.lock().unwrap();
                    if let Some(vpn) = state
                        .vpn_connections
                        .iter_mut()
                        .find(|v| v.path == conn_path)
                    {
                        vpn.state = VpnState::Connected;
                    }
                    return Err(e.into());
                }
            } else {
                warn!("[nm] No active connection path for VPN: {}", conn_path);
            }
        } else if action_id == "scan_wifi" {
            // Request a WiFi scan from the background task
            let _ = self.scan_tx.send(()).await;
        } else {
            debug!("[nm] Unknown action: {}", action_id);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Signal monitoring
// ---------------------------------------------------------------------------

async fn monitor_nm_signals(
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    notifier: WidgetNotifier,
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
                        let mut st = state.lock().unwrap();
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

                    let mut st = state.lock().unwrap();
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
                    let mut st = state.lock().unwrap();

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

/// Refresh VPN connection states from D-Bus.
async fn refresh_vpn_states(conn: &Connection, state: &Arc<StdMutex<NmState>>) -> Result<()> {
    let profiles = get_vpn_profiles(conn).await?;
    let active_vpns = get_active_vpn_connections(conn).await.unwrap_or_default();

    let mut new_connections = Vec::new();

    for profile in profiles {
        let active_info = active_vpns
            .iter()
            .find(|(_, _, uuid, _)| *uuid == profile.uuid);

        let vpn_state = active_info
            .map(|(_, _, _, state_code)| VpnState::from_active_state(*state_code))
            .unwrap_or(VpnState::Disconnected);

        let active_path = active_info.map(|(ap, _, _, _)| ap.clone());

        new_connections.push(VpnConnectionInfo {
            path: profile.path,
            uuid: profile.uuid,
            name: profile.name,
            state: vpn_state,
            active_path,
        });
    }

    let mut st = state.lock().unwrap();
    st.vpn_connections = new_connections;

    Ok(())
}

/// Background task: handles WiFi scanning using nmrs (non-Send).
/// Receives scan requests via channel and updates shared state.
async fn wifi_scan_task(
    mut scan_rx: tokio::sync::mpsc::Receiver<()>,
    nm: nmrs::NetworkManager,
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    notifier: WidgetNotifier,
) {
    while let Some(()) = scan_rx.recv().await {
        debug!("[nm] WiFi scan requested");

        // Set scanning state
        {
            let mut st = state.lock().unwrap();
            for adapter in &mut st.wifi_adapters {
                adapter.scanning = true;
            }
        }
        notifier.notify();

        match scan_and_list_known_networks(&nm, &conn).await {
            Ok(networks) => {
                info!("[nm] WiFi scan found {} known networks", networks.len());
                let mut st = state.lock().unwrap();
                for adapter in &mut st.wifi_adapters {
                    adapter.access_points = networks.clone();
                    adapter.scanning = false;
                }
            }
            Err(e) => {
                error!("[nm] WiFi scan failed: {}", e);
                let mut st = state.lock().unwrap();
                for adapter in &mut st.wifi_adapters {
                    adapter.scanning = false;
                }
            }
        }

        notifier.notify();
    }

    warn!("[nm] WiFi scan task stopped");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting networkmanager daemon...");

    // Create scan channel for WiFi scanning (uses nmrs which has non-Send futures)
    let (scan_tx, scan_rx) = tokio::sync::mpsc::channel::<()>(4);

    let (daemon, nm) = NetworkManagerDaemon::new(scan_tx).await?;

    let shared_state = daemon.shared_state();
    let monitor_conn = daemon.conn.clone();
    let scan_conn = daemon.conn.clone();

    let (server, notifier) = PluginServer::new("networkmanager-daemon", daemon);

    let scan_notifier = notifier.clone();

    // Monitor NM D-Bus signals
    let monitor_state = shared_state.clone();
    let monitor_notifier = notifier.clone();
    tokio::spawn(async move {
        if let Err(e) = monitor_nm_signals(monitor_conn, monitor_state, monitor_notifier).await {
            error!("[nm] D-Bus signal monitoring failed: {}", e);
        }
    });

    // WiFi scan background task (runs nmrs which has non-Send futures).
    // Use a dedicated thread with a single-threaded runtime + LocalSet
    // because nmrs futures are !Send and cannot be spawned on the multi-threaded runtime.
    let scan_state = shared_state.clone();
    std::thread::Builder::new()
        .name("nm-wifi-scan".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create scan runtime");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                wifi_scan_task(scan_rx, nm, scan_conn, scan_state, scan_notifier).await;
            });
        })
        .expect("Failed to spawn WiFi scan thread");

    server.run().await?;

    Ok(())
}
