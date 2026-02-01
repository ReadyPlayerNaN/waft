//! NetworkManager D-Bus integration using nmrs.
//!
//! This module uses nmrs for most NetworkManager operations.
//! D-Bus is only used directly for features nmrs doesn't expose:
//! - Link speed queries
//! - Saved connection profile lookup
//! - WiFi connection activation with saved credentials

use anyhow::Result;
use nmrs::{DeviceState, DeviceType, NetworkManager};
use std::sync::Arc;
use zbus::zvariant::OwnedValue;

use crate::dbus::DbusHandle;

// D-Bus constants (only used for features nmrs doesn't support)
const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_INTERFACE: &str = "org.freedesktop.NetworkManager";
const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_SETTINGS_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings";
const NM_CONNECTION_ACTIVE_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";

const DEVICE_TYPE_ETHERNET: u32 = 1;
const DEVICE_TYPE_WIFI: u32 = 2;

// =============================================================================
// nmrs-based functions (primary API)
// =============================================================================

/// Create a new nmrs NetworkManager instance.
/// This establishes its own D-Bus connection to the system bus.
pub async fn create_network_manager() -> Result<NetworkManager> {
    NetworkManager::new()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create NetworkManager: {}", e))
}

/// Check if NetworkManager is available on the system bus.
pub async fn check_availability_nmrs() -> bool {
    NetworkManager::new().await.is_ok()
}

/// Get all managed ethernet and WiFi devices using nmrs.
/// Filters out virtual interfaces and unmanaged devices.
pub async fn get_all_devices_nmrs(nm: &NetworkManager) -> Result<Vec<DeviceInfo>> {
    let devices = nm
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list devices: {}", e))?;

    let mut result = Vec::new();

    for device in devices {
        let device_info = match device_info_from_nmrs(&device) {
            Some(info) => info,
            None => continue,
        };

        if is_virtual_interface(&device_info.interface_name) {
            continue;
        }

        if !device_info.managed {
            continue;
        }

        result.push(device_info);
    }

    Ok(result)
}

/// Check if WiFi is enabled using nmrs.
#[allow(dead_code)]
pub async fn wifi_enabled_nmrs(nm: &NetworkManager) -> Result<bool> {
    nm.wifi_enabled()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to check WiFi enabled: {}", e))
}

/// Set WiFi enabled state using nmrs.
pub async fn set_wifi_enabled_nmrs(nm: &NetworkManager, enabled: bool) -> Result<()> {
    nm.set_wifi_enabled(enabled)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to set WiFi enabled: {}", e))
}

/// Trigger a WiFi scan using nmrs.
pub async fn scan_networks_nmrs(nm: &NetworkManager) -> Result<()> {
    nm.scan_networks()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to scan networks: {}", e))
}

/// List visible WiFi networks using nmrs.
pub async fn list_networks_nmrs(nm: &NetworkManager) -> Result<Vec<AccessPoint>> {
    let networks = nm
        .list_networks()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list networks: {}", e))?;

    Ok(networks
        .iter()
        .filter(|n| !n.ssid.is_empty())
        .map(access_point_from_nmrs)
        .collect())
}

/// Connect to wired network using nmrs.
pub async fn connect_wired_nmrs(nm: &NetworkManager) -> Result<()> {
    nm.connect_wired()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect wired: {}", e))
}

/// Disconnect from current network using nmrs.
pub async fn disconnect_nmrs(nm: &NetworkManager) -> Result<()> {
    nm.disconnect()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to disconnect: {}", e))
}

/// Disconnect a specific device via D-Bus.
/// This calls the Disconnect method on the Device interface.
pub async fn disconnect_device(dbus: &DbusHandle, device_path: &str) -> Result<()> {
    const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";

    dbus.connection()
        .call_method(
            Some(NM_SERVICE),
            device_path,
            Some(NM_DEVICE_INTERFACE),
            "Disconnect",
            &(),
        )
        .await?;

    Ok(())
}

// =============================================================================
// D-Bus functions (only for features nmrs doesn't support)
// =============================================================================

/// Get link speed for a wired device in Mbps.
/// Note: nmrs doesn't expose this property, so we use D-Bus directly.
pub async fn get_link_speed(dbus: &DbusHandle, device_path: &str) -> Result<Option<u32>> {
    match get_device_property::<u32>(dbus, device_path, "Speed").await {
        Ok(speed) => Ok(Some(speed)),
        Err(_) => Ok(None),
    }
}

/// Get IPv4 configuration for a device.
/// Returns (address, prefix_length, gateway) if available.
/// Note: nmrs doesn't expose IP4Config, so we use D-Bus directly.
pub async fn get_ip4_config(
    dbus: &DbusHandle,
    device_path: &str,
) -> Result<Option<(String, u32, Option<String>)>> {
    // Get the IP4Config object path from the device
    let ip4_config_path: String = match dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            device_path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_DEVICE_INTERFACE, "Ip4Config"),
        )
        .await
    {
        Ok(reply) => {
            let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
            match zbus::zvariant::OwnedObjectPath::try_from(value) {
                Ok(path) => path.to_string(),
                Err(_) => return Ok(None),
            }
        }
        Err(_) => return Ok(None),
    };

    if ip4_config_path == "/" {
        return Ok(None);
    }

    // Get AddressData from IP4Config
    let address_data: Vec<std::collections::HashMap<String, OwnedValue>> = match dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            ip4_config_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.NetworkManager.IP4Config", "AddressData"),
        )
        .await
    {
        Ok(reply) => {
            let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
            match Vec::<std::collections::HashMap<String, OwnedValue>>::try_from(value) {
                Ok(data) => data,
                Err(_) => return Ok(None),
            }
        }
        Err(_) => return Ok(None),
    };

    if address_data.is_empty() {
        return Ok(None);
    }

    let first_addr = &address_data[0];
    let address = first_addr
        .get("address")
        .and_then(|v| String::try_from(v.clone()).ok());
    let prefix = first_addr
        .get("prefix")
        .and_then(|v| u32::try_from(v.clone()).ok());

    let (address, prefix) = match (address, prefix) {
        (Some(a), Some(p)) => (a, p),
        _ => return Ok(None),
    };

    // Get Gateway from IP4Config
    let gateway: Option<String> = match dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            ip4_config_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.NetworkManager.IP4Config", "Gateway"),
        )
        .await
    {
        Ok(reply) => {
            let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
            match String::try_from(value) {
                Ok(gw) if !gw.is_empty() => Some(gw),
                _ => None,
            }
        }
        Err(_) => None,
    };

    Ok(Some((address, prefix, gateway)))
}

/// Get IPv6 configuration for a device.
/// Returns the IPv6 address if available.
/// Note: nmrs doesn't expose IP6Config, so we use D-Bus directly.
pub async fn get_ip6_config(dbus: &DbusHandle, device_path: &str) -> Result<Option<String>> {
    // Get the IP6Config object path from the device
    let ip6_config_path: String = match dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            device_path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_DEVICE_INTERFACE, "Ip6Config"),
        )
        .await
    {
        Ok(reply) => {
            let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
            match zbus::zvariant::OwnedObjectPath::try_from(value) {
                Ok(path) => path.to_string(),
                Err(_) => return Ok(None),
            }
        }
        Err(_) => return Ok(None),
    };

    if ip6_config_path == "/" {
        return Ok(None);
    }

    // Get AddressData from IP6Config
    let address_data: Vec<std::collections::HashMap<String, OwnedValue>> = match dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            ip6_config_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.NetworkManager.IP6Config", "AddressData"),
        )
        .await
    {
        Ok(reply) => {
            let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
            match Vec::<std::collections::HashMap<String, OwnedValue>>::try_from(value) {
                Ok(data) => data,
                Err(_) => return Ok(None),
            }
        }
        Err(_) => return Ok(None),
    };

    if address_data.is_empty() {
        return Ok(None);
    }

    let first_addr = &address_data[0];
    let address = first_addr
        .get("address")
        .and_then(|v| String::try_from(v.clone()).ok());

    Ok(address)
}

/// Get combined IP configuration for a device.
/// Returns an IpConfiguration struct with all available IP information.
pub async fn get_ip_configuration(dbus: &DbusHandle, device_path: &str) -> Result<IpConfiguration> {
    let mut config = IpConfiguration::default();

    if let Ok(Some((address, prefix, gateway))) = get_ip4_config(dbus, device_path).await {
        config.ipv4_address = Some(address);
        config.subnet_mask = Some(prefix_to_subnet_mask(prefix));
        config.gateway = gateway;
    }

    if let Ok(Some(address)) = get_ip6_config(dbus, device_path).await {
        config.ipv6_address = Some(address);
    }

    Ok(config)
}

/// Find all saved WiFi connections matching the given SSID.
/// Note: nmrs doesn't expose saved connection profiles, so we use D-Bus directly.
pub async fn get_connections_for_ssid(dbus: &DbusHandle, ssid: &str) -> Result<Vec<String>> {
    let settings_paths: Vec<zbus::zvariant::OwnedObjectPath> = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            "/org/freedesktop/NetworkManager/Settings",
            Some("org.freedesktop.NetworkManager.Settings"),
            "ListConnections",
            &(),
        )
        .await?
        .body()
        .deserialize()?;

    let mut matching_connections = Vec::new();

    for settings_path in settings_paths {
        let path_str = settings_path.as_str();

        let settings: std::collections::HashMap<
            String,
            std::collections::HashMap<String, OwnedValue>,
        > = dbus
            .connection()
            .call_method(
                Some(NM_SERVICE),
                path_str,
                Some("org.freedesktop.NetworkManager.Settings.Connection"),
                "GetSettings",
                &(),
            )
            .await?
            .body()
            .deserialize()?;

        if let Some(wireless) = settings.get("802-11-wireless") {
            if let Some(ssid_value) = wireless.get("ssid") {
                if let Ok(ssid_bytes) = <Vec<u8>>::try_from(ssid_value.clone()) {
                    let connection_ssid = String::from_utf8_lossy(&ssid_bytes);
                    if connection_ssid == ssid {
                        matching_connections.push(path_str.to_string());
                    }
                }
            }
        }
    }

    Ok(matching_connections)
}

/// Activate a network connection on a device.
/// Note: nmrs requires credentials for WiFi, so we use D-Bus for saved connections.
pub async fn activate_connection(
    dbus: &DbusHandle,
    connection_path: Option<&str>,
    device_path: &str,
    specific_object: Option<&str>,
) -> Result<String> {
    let conn_path = connection_path.unwrap_or("/");
    let specific = specific_object.unwrap_or("/");

    let active_conn_path: zbus::zvariant::OwnedObjectPath = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            NM_PATH,
            Some(NM_INTERFACE),
            "ActivateConnection",
            &(conn_path, device_path, specific),
        )
        .await?
        .body()
        .deserialize()?;

    Ok(active_conn_path.to_string())
}

// =============================================================================
// VPN D-Bus functions
// =============================================================================

/// Information about a VPN connection profile.
#[derive(Debug, Clone)]
pub struct VpnConnectionInfo {
    pub path: String,
    pub uuid: String,
    pub name: String,
}

/// Get all configured VPN connection profiles.
/// Note: nmrs doesn't expose VPN connection profiles, so we use D-Bus directly.
pub async fn get_vpn_connections(dbus: &DbusHandle) -> Result<Vec<VpnConnectionInfo>> {
    let settings_paths: Vec<zbus::zvariant::OwnedObjectPath> = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            NM_SETTINGS_PATH,
            Some(NM_SETTINGS_INTERFACE),
            "ListConnections",
            &(),
        )
        .await?
        .body()
        .deserialize()?;

    let mut vpn_connections = Vec::new();

    for settings_path in settings_paths {
        let path_str = settings_path.as_str();

        let settings: std::collections::HashMap<
            String,
            std::collections::HashMap<String, OwnedValue>,
        > = dbus
            .connection()
            .call_method(
                Some(NM_SERVICE),
                path_str,
                Some("org.freedesktop.NetworkManager.Settings.Connection"),
                "GetSettings",
                &(),
            )
            .await?
            .body()
            .deserialize()?;

        // Check if this is a VPN connection
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

                        vpn_connections.push(VpnConnectionInfo {
                            path: path_str.to_string(),
                            uuid,
                            name,
                        });
                    }
                }
            }
        }
    }

    Ok(vpn_connections)
}

/// Get currently active VPN connections.
/// Returns a list of (active_connection_path, connection_path, uuid, state).
/// ActiveConnection states: 0=unknown, 1=activating, 2=activated, 3=deactivating, 4=deactivated
pub async fn get_active_vpn_connections(
    dbus: &DbusHandle,
) -> Result<Vec<(String, String, String, u32)>> {
    // Get ActiveConnections property from NetworkManager
    let active_connections: Vec<zbus::zvariant::OwnedObjectPath> = match dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            NM_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_INTERFACE, "ActiveConnections"),
        )
        .await
    {
        Ok(reply) => {
            let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
            Vec::<zbus::zvariant::OwnedObjectPath>::try_from(value).unwrap_or_default()
        }
        Err(_) => return Ok(Vec::new()),
    };

    let mut vpn_active = Vec::new();

    for active_conn_path in active_connections {
        let path_str = active_conn_path.as_str();

        // Get the Type property (e.g., "vpn")
        let conn_type: String = match dbus
            .connection()
            .call_method(
                Some(NM_SERVICE),
                path_str,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(NM_CONNECTION_ACTIVE_INTERFACE, "Type"),
            )
            .await
        {
            Ok(reply) => {
                let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
                String::try_from(value).unwrap_or_default()
            }
            Err(_) => continue,
        };

        if conn_type == "vpn" {
            // Get Connection (Settings path)
            let connection_path: String = match dbus
                .connection()
                .call_method(
                    Some(NM_SERVICE),
                    path_str,
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &(NM_CONNECTION_ACTIVE_INTERFACE, "Connection"),
                )
                .await
            {
                Ok(reply) => {
                    let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
                    zbus::zvariant::OwnedObjectPath::try_from(value)
                        .map(|p| p.to_string())
                        .unwrap_or_default()
                }
                Err(_) => continue,
            };

            // Get UUID
            let uuid: String = match dbus
                .connection()
                .call_method(
                    Some(NM_SERVICE),
                    path_str,
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &(NM_CONNECTION_ACTIVE_INTERFACE, "Uuid"),
                )
                .await
            {
                Ok(reply) => {
                    let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
                    String::try_from(value).unwrap_or_default()
                }
                Err(_) => continue,
            };

            // Get State
            let state: u32 = match dbus
                .connection()
                .call_method(
                    Some(NM_SERVICE),
                    path_str,
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &(NM_CONNECTION_ACTIVE_INTERFACE, "State"),
                )
                .await
            {
                Ok(reply) => {
                    let value: OwnedValue = reply.body().deserialize::<(OwnedValue,)>()?.0;
                    u32::try_from(value).unwrap_or(0)
                }
                Err(_) => 0, // Unknown state
            };

            vpn_active.push((path_str.to_string(), connection_path, uuid, state));
        }
    }

    Ok(vpn_active)
}

/// Activate a VPN connection.
/// Returns the active connection path on success.
pub async fn activate_vpn_connection(dbus: &DbusHandle, connection_path: &str) -> Result<String> {
    let conn_path = zbus::zvariant::ObjectPath::try_from(connection_path)?;
    let device_path = zbus::zvariant::ObjectPath::try_from("/")?;
    let specific_path = zbus::zvariant::ObjectPath::try_from("/")?;

    let active_conn_path: zbus::zvariant::OwnedObjectPath = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            NM_PATH,
            Some(NM_INTERFACE),
            "ActivateConnection",
            &(conn_path, device_path, specific_path),
        )
        .await?
        .body()
        .deserialize()?;

    Ok(active_conn_path.to_string())
}

/// Deactivate an active VPN connection.
pub async fn deactivate_vpn_connection(
    dbus: &DbusHandle,
    active_connection_path: &str,
) -> Result<()> {
    dbus.connection()
        .call_method(
            Some(NM_SERVICE),
            NM_PATH,
            Some(NM_INTERFACE),
            "DeactivateConnection",
            &(zbus::zvariant::ObjectPath::try_from(active_connection_path)?),
        )
        .await?;

    Ok(())
}

/// Send-safe version of get_vpn_connections for use with spawn_on_tokio.
pub async fn get_vpn_connections_sendable(dbus: Arc<DbusHandle>) -> Result<Vec<VpnConnectionInfo>> {
    get_vpn_connections(&dbus).await
}

/// Send-safe version of get_active_vpn_connections for use with spawn_on_tokio.
pub async fn get_active_vpn_connections_sendable(
    dbus: Arc<DbusHandle>,
) -> Result<Vec<(String, String, String, u32)>> {
    get_active_vpn_connections(&dbus).await
}

/// Send-safe version of activate_vpn_connection for use with spawn_on_tokio.
pub async fn activate_vpn_connection_sendable(
    dbus: Arc<DbusHandle>,
    connection_path: String,
) -> Result<String> {
    activate_vpn_connection(&dbus, &connection_path).await
}

/// Send-safe version of deactivate_vpn_connection for use with spawn_on_tokio.
pub async fn deactivate_vpn_connection_sendable(
    dbus: Arc<DbusHandle>,
    active_connection_path: String,
) -> Result<()> {
    deactivate_vpn_connection(&dbus, &active_connection_path).await
}

/// Send-safe version of disconnect_device for use with spawn_on_tokio.
pub async fn disconnect_device_sendable(dbus: Arc<DbusHandle>, device_path: String) -> Result<()> {
    disconnect_device(&dbus, &device_path).await
}

/// VPN interface for VPN-specific properties
const NM_VPN_CONNECTION_INTERFACE: &str = "org.freedesktop.NetworkManager.VPN.Connection";

/// Subscribe to VPN state changes.
/// Monitors PropertiesChanged signals on active connections to detect VPN state changes.
/// Handles both ActiveConnection.State and VPN.Connection.VpnState.
pub async fn subscribe_vpn_state_changed<F>(dbus: Arc<DbusHandle>, mut callback: F) -> Result<()>
where
    F: FnMut(String, u32) + 'static,
{
    use futures_util::StreamExt;
    use log::debug;

    // Subscribe to PropertiesChanged signals on the NetworkManager path
    // This will catch ActiveConnections changes
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, &*dbus.connection(), None).await?;

    while let Some(msg) = stream.next().await {
        if let Ok(msg) = msg {
            let path = msg
                .header()
                .path()
                .map(|p| p.to_string())
                .unwrap_or_default();

            // Check if this is an active connection path
            if path.contains("/ActiveConnection/") {
                let body = msg.body();
                let deserialize_result: Result<
                    (
                        String,
                        std::collections::HashMap<String, OwnedValue>,
                        Vec<String>,
                    ),
                    _,
                > = body.deserialize();

                if let Ok((interface, changed, _invalidated)) = deserialize_result {
                    // Handle ActiveConnection.State changes
                    if interface == NM_CONNECTION_ACTIVE_INTERFACE {
                        if let Some(state_value) = changed.get("State") {
                            if let Ok(state) = u32::try_from(state_value.clone()) {
                                debug!(
                                    "ActiveConnection State changed: path={}, state={}",
                                    path, state
                                );
                                // Check if this is a VPN connection
                                let is_vpn = if let Some(type_value) = changed.get("Type") {
                                    String::try_from(type_value.clone())
                                        .map(|t| t == "vpn")
                                        .unwrap_or(false)
                                } else {
                                    // Type not in changed properties, query it
                                    get_active_connection_type(&dbus, &path)
                                        .await
                                        .map(|t| t == "vpn")
                                        .unwrap_or(false)
                                };

                                if is_vpn {
                                    debug!(
                                        "VPN ActiveConnection state: path={}, state={}",
                                        path, state
                                    );
                                    callback(path, state);
                                }
                            }
                        }
                    }
                    // Handle VPN.Connection.VpnState changes
                    else if interface == NM_VPN_CONNECTION_INTERFACE {
                        if let Some(vpn_state_value) = changed.get("VpnState") {
                            if let Ok(vpn_state) = u32::try_from(vpn_state_value.clone()) {
                                debug!(
                                    "VPN.Connection VpnState changed: path={}, vpn_state={}",
                                    path, vpn_state
                                );
                                // Convert VPN-specific state to ActiveConnection state
                                // VPN states: 0=unknown, 1=prepare, 2=need_auth, 3=connect, 4=ip_config, 5=activated, 6=failed, 7=disconnected
                                // ActiveConnection states: 0=unknown, 1=activating, 2=activated, 3=deactivating, 4=deactivated
                                let active_state = match vpn_state {
                                    5 => 2,             // Activated -> Activated
                                    6 | 7 => 4,         // Failed/Disconnected -> Deactivated
                                    1 | 2 | 3 | 4 => 1, // Prepare/NeedAuth/Connect/IPConfig -> Activating
                                    _ => 0,             // Unknown
                                };
                                callback(path, active_state);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Helper to get the Type of an active connection.
async fn get_active_connection_type(dbus: &DbusHandle, path: &str) -> Result<String> {
    let conn_type: OwnedValue = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_CONNECTION_ACTIVE_INTERFACE, "Type"),
        )
        .await?
        .body()
        .deserialize::<(OwnedValue,)>()?
        .0;

    String::try_from(conn_type)
        .map_err(|e| anyhow::anyhow!("Failed to get connection type: {:?}", e))
}

// =============================================================================
// Device signal subscriptions
// =============================================================================

/// Subscribe to DeviceAdded signal from NetworkManager.
/// Calls the provided callback for each device added.
pub async fn subscribe_device_added<F>(dbus: Arc<DbusHandle>, mut callback: F) -> Result<()>
where
    F: FnMut(String) + 'static,
{
    use futures_util::StreamExt;
    use zbus::zvariant::ObjectPath;

    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .path(NM_PATH)?
        .interface(NM_INTERFACE)?
        .member("DeviceAdded")?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, &*dbus.connection(), None).await?;

    while let Some(msg) = stream.next().await {
        if let Ok(msg) = msg {
            let body = msg.body();
            if let Ok(path) = body.deserialize::<ObjectPath<'_>>() {
                callback(path.to_string());
            }
        }
    }

    Ok(())
}

/// Subscribe to DeviceRemoved signal from NetworkManager.
/// Calls the provided callback for each device removed.
pub async fn subscribe_device_removed<F>(dbus: Arc<DbusHandle>, mut callback: F) -> Result<()>
where
    F: FnMut(String) + 'static,
{
    use futures_util::StreamExt;
    use zbus::zvariant::ObjectPath;

    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .path(NM_PATH)?
        .interface(NM_INTERFACE)?
        .member("DeviceRemoved")?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, &*dbus.connection(), None).await?;

    while let Some(msg) = stream.next().await {
        if let Ok(msg) = msg {
            let body = msg.body();
            if let Ok(path) = body.deserialize::<ObjectPath<'_>>() {
                callback(path.to_string());
            }
        }
    }

    Ok(())
}

/// Subscribe to StateChanged signal from a specific device.
/// The callback receives (device_path, new_state, old_state, reason).
pub async fn subscribe_device_state_changed<F>(dbus: Arc<DbusHandle>, mut callback: F) -> Result<()>
where
    F: FnMut(String, u32, u32, u32) + 'static,
{
    use futures_util::StreamExt;
    use log::debug;

    const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";

    // Subscribe to StateChanged signals from any Device
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM_SERVICE)?
        .interface(NM_DEVICE_INTERFACE)?
        .member("StateChanged")?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, &*dbus.connection(), None).await?;

    while let Some(msg) = stream.next().await {
        if let Ok(msg) = msg {
            let path = msg
                .header()
                .path()
                .map(|p| p.to_string())
                .unwrap_or_default();
            let body = msg.body();
            // StateChanged signal has signature (uuu) - new_state, old_state, reason
            if let Ok((new_state, old_state, reason)) = body.deserialize::<(u32, u32, u32)>() {
                debug!(
                    "Device state changed: path={}, new={}, old={}, reason={}",
                    path, new_state, old_state, reason
                );
                callback(path, new_state, old_state, reason);
            }
        }
    }

    Ok(())
}

/// Send-safe version of get_device_info for use with spawn_on_tokio.
///
/// This wrapper is needed because `get_device_info` borrows the DbusHandle,
/// but `spawn_on_tokio` requires a `Send + 'static` future.
pub async fn get_device_info_sendable(
    dbus: Arc<DbusHandle>,
    device_path: String,
) -> Result<Option<DeviceInfo>> {
    get_device_info(&dbus, &device_path).await
}

/// Get device info for a specific device path using D-Bus.
pub async fn get_device_info(dbus: &DbusHandle, device_path: &str) -> Result<Option<DeviceInfo>> {
    // Get device type
    let device_type: u32 = match get_device_property::<u32>(dbus, device_path, "DeviceType").await {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    // Only handle ethernet and wifi
    if device_type != DEVICE_TYPE_ETHERNET && device_type != DEVICE_TYPE_WIFI {
        return Ok(None);
    }

    // Get interface name
    let interface_name: String = get_device_property(dbus, device_path, "Interface").await?;

    // Skip virtual interfaces
    if is_virtual_interface(&interface_name) {
        return Ok(None);
    }

    // Get managed state
    let managed: bool = get_device_property(dbus, device_path, "Managed")
        .await
        .unwrap_or(false);

    // Get device state
    let device_state: u32 = get_device_property(dbus, device_path, "State")
        .await
        .unwrap_or(0);

    Ok(Some(DeviceInfo {
        path: device_path.to_string(),
        device_type,
        interface_name,
        managed,
        real: true,
        device_state,
    }))
}

// =============================================================================
// Internal helpers and types
// =============================================================================

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub device_type: u32,
    pub interface_name: String,
    pub managed: bool,
    pub real: bool,
    pub device_state: u32,
}

#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub path: String,
    pub ssid: String,
    pub strength: u8,
    pub flags: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}

impl AccessPoint {
    pub fn is_secure(&self) -> bool {
        self.flags != 0 || self.wpa_flags != 0 || self.rsn_flags != 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct IpConfiguration {
    pub ipv4_address: Option<String>,
    pub ipv6_address: Option<String>,
    pub subnet_mask: Option<String>,
    pub gateway: Option<String>,
}

fn device_state_to_u32(state: &DeviceState) -> u32 {
    match state {
        DeviceState::Unmanaged => 10,
        DeviceState::Unavailable => 20,
        DeviceState::Disconnected => 30,
        DeviceState::Prepare => 40,
        DeviceState::Config => 50,
        DeviceState::Activated => 100,
        DeviceState::Deactivating => 110,
        DeviceState::Failed => 120,
        DeviceState::Other(code) => *code,
        _ => 0,
    }
}

fn device_info_from_nmrs(device: &nmrs::Device) -> Option<DeviceInfo> {
    let device_type = match device.device_type {
        DeviceType::Ethernet => DEVICE_TYPE_ETHERNET,
        DeviceType::Wifi => DEVICE_TYPE_WIFI,
        _ => return None,
    };

    Some(DeviceInfo {
        path: device.path.clone(),
        device_type,
        interface_name: device.interface.clone(),
        managed: device.managed.unwrap_or(false),
        real: true,
        device_state: device_state_to_u32(&device.state),
    })
}

fn access_point_from_nmrs(network: &nmrs::Network) -> AccessPoint {
    let flags: u32 = if network.secured { 1 } else { 0 };
    let wpa_flags: u32 = if network.is_psk { 1 } else { 0 };
    let rsn_flags: u32 = if network.is_eap { 1 } else { 0 };

    AccessPoint {
        path: network
            .bssid
            .clone()
            .unwrap_or_else(|| network.ssid.clone()),
        ssid: network.ssid.clone(),
        strength: network.strength.unwrap_or(0),
        flags,
        wpa_flags,
        rsn_flags,
    }
}

fn is_virtual_interface(name: &str) -> bool {
    let virtual_prefixes = ["docker", "veth", "br-", "virbr", "vnet"];
    virtual_prefixes
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn prefix_to_subnet_mask(prefix: u32) -> String {
    if prefix == 0 {
        return "0.0.0.0".to_string();
    }
    if prefix > 32 {
        return "255.255.255.255".to_string();
    }
    let mask: u32 = !0u32 << (32 - prefix);
    format!(
        "{}.{}.{}.{}",
        (mask >> 24) & 0xFF,
        (mask >> 16) & 0xFF,
        (mask >> 8) & 0xFF,
        mask & 0xFF
    )
}

async fn get_device_property<T>(dbus: &DbusHandle, path: &str, property: &str) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let value: OwnedValue = dbus
        .connection()
        .call_method(
            Some(NM_SERVICE),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_DEVICE_INTERFACE, property),
        )
        .await?
        .body()
        .deserialize::<(OwnedValue,)>()?
        .0;

    value
        .try_into()
        .map_err(|e: T::Error| anyhow::anyhow!("Failed to convert property: {}", e))
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
