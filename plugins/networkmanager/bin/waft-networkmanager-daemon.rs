//! NetworkManager daemon - WiFi, Wired, and VPN network management.
//!
//! Provides entity types:
//! - `network-adapter`: WiFi and Ethernet adapters with connection state
//! - `vpn`: VPN connection profiles with state
//!
//! Monitors NetworkManager D-Bus signals for device/connection state changes.

use std::sync::LazyLock;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::entity::network::{
    ADAPTER_ENTITY_TYPE, AdapterKind, ETHERNET_CONNECTION_ENTITY_TYPE, IpMethod, MeteredState,
    NetworkAdapter, SecurityType, TETHERING_CONNECTION_ENTITY_TYPE, TetheringConnection,
    VPN_ENTITY_TYPE, VpnState as EntityVpnState, WIFI_NETWORK_ENTITY_TYPE, WiFiNetwork,
};
use waft_plugin::*;
use zbus::Connection;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| {
    waft_i18n::I18n::new(&[
        ("en-US", include_str!("../locales/en-US/networkmanager.ftl")),
        ("cs-CZ", include_str!("../locales/cs-CZ/networkmanager.ftl")),
    ])
});

fn i18n() -> &'static waft_i18n::I18n {
    &I18N
}

use waft_plugin_networkmanager::bluez_discovery::discover_bluez_paired_devices;
use waft_plugin_networkmanager::bluez_signal_monitor::monitor_bluez_signals;
use waft_plugin_networkmanager::dbus_property::{DEVICE_TYPE_ETHERNET, DEVICE_TYPE_WIFI};
use waft_plugin_networkmanager::ethernet::{
    activate_ethernet_connection, deactivate_ethernet_connection,
};
use waft_plugin_networkmanager::ip_config::{fetch_public_ip, get_device_ip4_config};
use waft_plugin_networkmanager::nmrs_adapter;
use waft_plugin_networkmanager::signal_monitor::monitor_nm_signals;
use waft_plugin_networkmanager::state::{
    CachedIpConfig, EthernetAdapterState, NmState, TetheringConnectionState, VpnState,
    WiFiAdapterState,
};
use waft_plugin_networkmanager::tethering::{
    deactivate_tethering, get_active_tethering_connections, get_tethering_profiles,
};
use waft_plugin_networkmanager::vpn::{get_active_vpn_connections, get_vpn_profiles};
use waft_plugin_networkmanager::wifi::{
    activate_connection, add_and_activate_connection, build_wifi_qr_string, connect_wired_dbus,
    get_connections_for_ssid, get_wifi_psk,
};
use waft_plugin_networkmanager::wifi_scan::wifi_scan_task;

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct NetworkManagerPlugin {
    conn: Connection,
    nm: nmrs::NetworkManager,
    state: Arc<StdMutex<NmState>>,
    /// Channel to request WiFi scan from background task.
    scan_tx: tokio::sync::mpsc::Sender<()>,
}

impl NetworkManagerPlugin {
    async fn new(scan_tx: tokio::sync::mpsc::Sender<()>) -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;
        let nm = nmrs::NetworkManager::new()
            .await
            .context("Failed to create nmrs NetworkManager client")?;

        let mut state = NmState::default();

        // Discover devices
        match nmrs_adapter::discover_devices(&nm).await {
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
                                ip_config: None,
                                active_connection_uuid: None,
                                profiles: Vec::new(),
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
                error!("[nm] Failed to discover devices: {e}");
            }
        }

        // Read IP configuration for connected ethernet adapters
        for adapter in &mut state.ethernet_adapters {
            if adapter.is_connected() {
                match get_device_ip4_config(&conn, &adapter.path).await {
                    Ok(Some(ip)) => {
                        debug!(
                            "[nm] Ethernet {} IP: {}/{}",
                            adapter.interface_name, ip.address, ip.prefix
                        );
                        adapter.ip_config = Some(CachedIpConfig {
                            address: ip.address,
                            prefix: ip.prefix,
                            gateway: ip.gateway,
                        });
                    }
                    Ok(None) => {}
                    Err(e) => {
                        warn!(
                            "[nm] Failed to read IP config for {}: {}",
                            adapter.interface_name, e
                        );
                    }
                }
            }
        }

        // Read active AP for already-connected WiFi adapters.
        // Populates both active_ssid and access_points so the connected network
        // is immediately visible as a wifi-network entity without waiting for a scan.
        for adapter in &mut state.wifi_adapters {
            if let Ok(Some(ap_info)) =
                nmrs_adapter::get_active_access_point(&nm, &adapter.interface_name).await
            {
                debug!(
                    "[nm] WiFi {} already connected to {}",
                    adapter.interface_name, ap_info.ssid
                );
                adapter.active_ssid = Some(ap_info.ssid.clone());
                adapter.access_points.push(ap_info);
            }
        }

        // Fetch public IP if any adapter is connected
        let any_connected = state
            .ethernet_adapters
            .iter()
            .any(waft_plugin_networkmanager::state::EthernetAdapterState::is_connected)
            || state.wifi_adapters.iter().any(|a| a.active_ssid.is_some());
        if any_connected && let Some(public_ip) = fetch_public_ip().await {
            debug!("[nm] Public IP: {public_ip}");
            state.public_ip = Some(public_ip);
        }

        // Discover ethernet connection profiles
        match waft_plugin_networkmanager::ethernet::get_ethernet_profiles(&nm).await {
            Ok(profiles) => {
                info!("[nm] Found {} ethernet profiles", profiles.len());
                for adapter in &mut state.ethernet_adapters {
                    // Read active connection UUID for connected adapters
                    if adapter.is_connected() {
                        adapter.active_connection_uuid =
                            waft_plugin_networkmanager::ethernet::get_active_connection_uuid(
                                &conn,
                                &adapter.path,
                            )
                            .await
                            .unwrap_or(None);
                    }
                    adapter.profiles = profiles.clone();
                }
            }
            Err(e) => {
                error!("[nm] Failed to get ethernet profiles: {e}");
            }
        }

        // Discover VPN connections
        match get_vpn_profiles(&nm).await {
            Ok(profiles) => {
                info!("[nm] Found {} VPN profiles", profiles.len());

                let active_vpns = get_active_vpn_connections(&conn).await.unwrap_or_default();

                for profile in profiles {
                    let vpn_state = active_vpns
                        .get(&profile.uuid)
                        .cloned()
                        .unwrap_or(VpnState::Disconnected);

                    debug!(
                        "[nm] VPN {}: path={}, state={:?}",
                        profile.name, profile.path, vpn_state
                    );

                    state.vpn_connections.push(
                        waft_plugin_networkmanager::state::VpnConnectionInfo {
                            path: profile.path,
                            uuid: profile.uuid,
                            name: profile.name,
                            conn_type: profile.conn_type,
                            state: vpn_state,
                            active_path: None,
                        },
                    );
                }
            }
            Err(e) => {
                error!("[nm] Failed to get VPN profiles: {e}");
            }
        }

        // Discover bluetooth NM devices (tethering is only visible when one is ready)
        match nmrs_adapter::discover_bluetooth_devices(&nm).await {
            Ok(devices) => {
                let ready_count = devices.iter().filter(|d| d.ready()).count();
                info!(
                    "[nm] Found {} bluetooth NM devices ({} ready)",
                    devices.len(),
                    ready_count
                );
                state.bluetooth_devices = devices;
            }
            Err(e) => {
                warn!("[nm] Failed to discover bluetooth devices: {e}");
            }
        }

        // Discover BlueZ paired devices (source of truth for tethering visibility)
        match discover_bluez_paired_devices(&conn).await {
            Ok(devices) => {
                let connected_count = devices.iter().filter(|d| d.connected).count();
                info!(
                    "[nm] Found {} BlueZ paired devices ({} connected)",
                    devices.len(),
                    connected_count
                );
                state.bluez_paired_devices = devices;
            }
            Err(e) => {
                warn!("[nm] Failed to discover BlueZ paired devices: {e}");
            }
        }

        // Discover tethering (bluetooth) connections
        match get_tethering_profiles(&nm).await {
            Ok(profiles) => {
                info!("[nm] Found {} tethering profiles", profiles.len());

                let active = get_active_tethering_connections(&nm)
                    .await
                    .unwrap_or_default();

                for profile in profiles {
                    let is_active = profile
                        .bdaddr
                        .as_ref()
                        .and_then(|bdaddr| active.get(bdaddr))
                        .copied()
                        .unwrap_or(false);

                    debug!(
                        "[nm] Tethering {}: path={}, active={}, bdaddr={:?}",
                        profile.name, profile.path, is_active, profile.bdaddr
                    );

                    state.tethering_connections.push(TetheringConnectionState {
                        path: profile.path,
                        uuid: profile.uuid,
                        name: profile.name,
                        active: is_active,
                        active_path: None,
                        bdaddr: profile.bdaddr,
                    });
                }
            }
            Err(e) => {
                error!("[nm] Failed to get tethering profiles: {e}");
            }
        }

        let plugin = Self {
            conn,
            nm,
            state: Arc::new(StdMutex::new(state)),
            scan_tx,
        };

        Ok(plugin)
    }

    fn shared_state(&self) -> Arc<StdMutex<NmState>> {
        self.state.clone()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, NmState> {
        lock_or_recover(&self.state)
    }
}

// ---------------------------------------------------------------------------
// Entity building
// ---------------------------------------------------------------------------

fn to_entity_vpn_state(state: &VpnState) -> EntityVpnState {
    match state {
        VpnState::Disconnected => EntityVpnState::Disconnected,
        VpnState::Connecting => EntityVpnState::Connecting,
        VpnState::Connected => EntityVpnState::Connected,
        VpnState::Disconnecting => EntityVpnState::Disconnecting,
    }
}

/// Convert NM metered integer to protocol enum.
/// NM values: 0=unknown, 1=yes, 2=no, 3=guess-yes, 4=guess-no.
fn nm_metered_to_entity(nm_metered: i32) -> MeteredState {
    match nm_metered {
        1 => MeteredState::Yes,
        2 => MeteredState::No,
        3 => MeteredState::GuessYes,
        4 => MeteredState::GuessNo,
        _ => MeteredState::Unknown,
    }
}

/// Convert NM ipv4.method string to protocol enum.
fn nm_ip_method_to_entity(method: &str) -> IpMethod {
    match method {
        "auto" => IpMethod::Auto,
        "manual" => IpMethod::Manual,
        "link-local" => IpMethod::LinkLocal,
        "disabled" => IpMethod::Disabled,
        _ => IpMethod::Auto,
    }
}

fn entity_metered_to_nm(value: &serde_json::Value) -> Option<i32> {
    value
        .as_i64()
        .map(|v| v as i32)
        .or_else(|| match value.as_str()? {
            "Unknown" => Some(0),
            "Yes" => Some(1),
            "No" => Some(2),
            "GuessYes" => Some(3),
            "GuessNo" => Some(4),
            _ => None,
        })
}

fn entity_ip_method_to_nm(value: &serde_json::Value) -> Option<String> {
    Some(
        match value.as_str()? {
            "Auto" => "auto",
            "Manual" => "manual",
            "LinkLocal" => "link-local",
            "Disabled" => "disabled",
            other => other,
        }
        .to_string(),
    )
}

fn parse_ipv4_to_u32_local(s: &str) -> Option<u32> {
    let parts: Vec<u8> = s.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() == 4 {
        Some(u32::from_le_bytes([parts[0], parts[1], parts[2], parts[3]]))
    } else {
        None
    }
}

fn build_wifi_settings_patch(params: &serde_json::Value) -> nmrs::models::SettingsPatch {
    let mut patch = nmrs::models::SettingsPatch::default();
    patch.autoconnect = params
        .get("autoconnect")
        .and_then(serde_json::Value::as_bool);
    patch.raw_overlay = None;

    if let Some(metered) = params.get("metered").and_then(entity_metered_to_nm) {
        let overlay = patch
            .raw_overlay
            .get_or_insert_with(std::collections::HashMap::new);
        overlay
            .entry("connection".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(
                "metered".to_string(),
                zbus::zvariant::OwnedValue::from(metered),
            );
    }

    if let Some(method) = params.get("ip_method").and_then(entity_ip_method_to_nm) {
        let overlay = patch
            .raw_overlay
            .get_or_insert_with(std::collections::HashMap::new);
        overlay
            .entry("ipv4".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(
                "method".to_string(),
                zbus::zvariant::Value::from(method)
                    .try_into()
                    .expect("String is a valid zvariant Value"),
            );
    }

    if let Some(dns_arr) = params
        .get("dns_servers")
        .and_then(serde_json::Value::as_array)
    {
        let addrs: Vec<u32> = dns_arr
            .iter()
            .filter_map(|v| v.as_str())
            .filter_map(parse_ipv4_to_u32_local)
            .collect();
        let overlay = patch
            .raw_overlay
            .get_or_insert_with(std::collections::HashMap::new);
        overlay
            .entry("ipv4".to_string())
            .or_insert_with(std::collections::HashMap::new)
            .insert(
                "dns".to_string(),
                zbus::zvariant::Value::from(addrs)
                    .try_into()
                    .expect("Vec<u32> is a valid zvariant Value"),
            );
    }

    patch
}

fn build_nmrs_wifi_security(
    security_type: SecurityType,
    password: Option<&str>,
    known: bool,
) -> anyhow::Result<nmrs::WifiSecurity> {
    match security_type {
        SecurityType::Open => Ok(nmrs::WifiSecurity::Open),
        SecurityType::Enterprise => anyhow::bail!("enterprise-not-supported"),
        SecurityType::Wep => anyhow::bail!("wep-uses-legacy-flow"),
        SecurityType::Wpa | SecurityType::Wpa2 | SecurityType::Wpa3 => {
            if let Some(password) = password {
                Ok(nmrs::WifiSecurity::WpaPsk {
                    psk: password.to_string(),
                })
            } else if known {
                Ok(nmrs::WifiSecurity::Open)
            } else {
                anyhow::bail!("password-required")
            }
        }
    }
}

fn wifi_adapter_to_entities(
    adapter: &WiFiAdapterState,
    connecting_ssid: &Option<String>,
) -> Vec<Entity> {
    let mut entities = Vec::new();

    // Adapter entity
    let adapter_urn = Urn::new(
        "networkmanager",
        ADAPTER_ENTITY_TYPE,
        &adapter.interface_name,
    );
    let adapter_entity = NetworkAdapter {
        name: adapter.interface_name.clone(),
        enabled: adapter.enabled,
        connected: adapter.active_ssid.is_some(),
        scanning: adapter.scanning,
        ip: None,
        public_ip: None,
        kind: AdapterKind::Wireless,
    };
    entities.push(Entity::new(
        adapter_urn.clone(),
        ADAPTER_ENTITY_TYPE,
        &adapter_entity,
    ));

    // WiFi network child entities
    for ap in &adapter.access_points {
        let network_urn = adapter_urn.child(WIFI_NETWORK_ENTITY_TYPE, &ap.ssid);

        let (autoconnect, metered, dns_servers, ip_method) =
            if let Some(ref settings) = ap.cached_settings {
                (
                    settings.autoconnect,
                    settings.metered.map(nm_metered_to_entity),
                    settings.dns_servers.clone(),
                    settings.ip_method.as_deref().map(nm_ip_method_to_entity),
                )
            } else {
                (None, None, None, None)
            };

        let network_entity = WiFiNetwork {
            ssid: ap.ssid.clone(),
            strength: ap.strength,
            secure: ap.secure,
            known: ap.known,
            connected: adapter.active_ssid.as_ref() == Some(&ap.ssid),
            security_type: ap.security_type,
            connecting: connecting_ssid.as_ref() == Some(&ap.ssid),
            autoconnect,
            metered,
            dns_servers,
            ip_method,
        };
        entities.push(Entity::new(
            network_urn,
            WIFI_NETWORK_ENTITY_TYPE,
            &network_entity,
        ));
    }

    entities
}

fn ethernet_adapter_to_entities(
    adapter: &EthernetAdapterState,
    public_ip: &Option<String>,
) -> Vec<Entity> {
    let mut entities = Vec::new();

    // Adapter entity
    let adapter_urn = Urn::new(
        "networkmanager",
        ADAPTER_ENTITY_TYPE,
        &adapter.interface_name,
    );

    let ip = adapter
        .ip_config
        .as_ref()
        .map(|c| waft_plugin::entity::network::IpInfo {
            address: c.address.clone(),
            prefix: c.prefix,
            gateway: c.gateway.clone(),
        });

    let adapter_entity = NetworkAdapter {
        name: adapter.interface_name.clone(),
        enabled: true,
        connected: adapter.is_connected(),
        scanning: false,
        ip,
        public_ip: if adapter.is_connected() {
            public_ip.clone()
        } else {
            None
        },
        kind: AdapterKind::Wired,
    };
    entities.push(Entity::new(
        adapter_urn.clone(),
        ADAPTER_ENTITY_TYPE,
        &adapter_entity,
    ));

    // Ethernet connection profile child entities
    for profile in &adapter.profiles {
        let conn_urn = adapter_urn.child(ETHERNET_CONNECTION_ENTITY_TYPE, &profile.uuid);
        let conn_entity = waft_plugin::entity::network::EthernetConnection {
            name: profile.name.clone(),
            uuid: profile.uuid.clone(),
            active: adapter
                .active_connection_uuid
                .as_ref()
                .is_some_and(|u| *u == profile.uuid),
        };
        entities.push(Entity::new(
            conn_urn,
            ETHERNET_CONNECTION_ENTITY_TYPE,
            &conn_entity,
        ));
    }

    entities
}

fn vpn_to_entity(vpn: &waft_plugin_networkmanager::state::VpnConnectionInfo) -> Entity {
    let vpn_type = match vpn.conn_type.as_str() {
        "wireguard" => waft_plugin::entity::network::VpnType::Wireguard,
        _ => waft_plugin::entity::network::VpnType::Vpn,
    };
    let entity = waft_plugin::entity::network::Vpn {
        name: vpn.name.clone(),
        state: to_entity_vpn_state(&vpn.state),
        vpn_type,
    };

    Entity::new(
        Urn::new("networkmanager", VPN_ENTITY_TYPE, &vpn.name),
        VPN_ENTITY_TYPE,
        &entity,
    )
}

fn tethering_adapter_to_entities(
    tethering_connections: &[TetheringConnectionState],
) -> Vec<Entity> {
    let mut entities = Vec::new();

    let any_active = tethering_connections.iter().any(|c| c.active);

    let adapter_urn = Urn::new("networkmanager", ADAPTER_ENTITY_TYPE, "tethering");
    let adapter_entity = NetworkAdapter {
        name: "tethering".to_string(),
        enabled: true,
        connected: any_active,
        scanning: false,
        ip: None,
        public_ip: None,
        kind: AdapterKind::Tethering,
    };
    entities.push(Entity::new(
        adapter_urn.clone(),
        ADAPTER_ENTITY_TYPE,
        &adapter_entity,
    ));

    for conn in tethering_connections {
        let conn_urn = adapter_urn.child(TETHERING_CONNECTION_ENTITY_TYPE, &conn.uuid);
        let conn_entity = TetheringConnection {
            name: conn.name.clone(),
            uuid: conn.uuid.clone(),
            active: conn.active,
        };
        entities.push(Entity::new(
            conn_urn,
            TETHERING_CONNECTION_ENTITY_TYPE,
            &conn_entity,
        ));
    }

    entities
}

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl Plugin for NetworkManagerPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        let mut entities = Vec::new();

        for adapter in &state.wifi_adapters {
            entities.extend(wifi_adapter_to_entities(adapter, &state.connecting_ssid));
        }

        for adapter in &state.ethernet_adapters {
            entities.extend(ethernet_adapter_to_entities(adapter, &state.public_ip));
        }

        for vpn in &state.vpn_connections {
            entities.push(vpn_to_entity(vpn));
        }

        let bluez_connected = state.any_tethering_device_connected();
        let tethering_active = state.tethering_connections.iter().any(|c| c.active);
        if (bluez_connected || tethering_active) && !state.tethering_connections.is_empty() {
            entities.extend(tethering_adapter_to_entities(&state.tethering_connections));
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let entity_type = urn.entity_type();

        match entity_type {
            "network-adapter" => {
                let adapter_id = urn.id();
                self.handle_adapter_action(adapter_id, &action, &params)
                    .await?;
            }
            "wifi-network" => {
                let ssid = urn.id();
                return self
                    .handle_wifi_network_action(&urn, ssid, &action, &params)
                    .await;
            }
            "ethernet-connection" => {
                let uuid = urn.id();
                self.handle_ethernet_connection_action(&urn, uuid, &action)
                    .await?;
            }
            "vpn" => {
                let vpn_id = urn.id();
                self.handle_vpn_action(vpn_id, &action).await?;
            }
            "tethering-connection" => {
                let uuid = urn.id();
                self.handle_tethering_connection_action(uuid, &action)
                    .await?;
            }
            _ => {
                debug!("[nm] Unknown entity type: {entity_type}");
            }
        }

        Ok(serde_json::Value::Null)
    }
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

impl NetworkManagerPlugin {
    async fn handle_adapter_action(
        &self,
        adapter_name: &str,
        action: &str,
        params: &serde_json::Value,
    ) -> anyhow::Result<()> {
        // Determine adapter type
        let (is_wifi, is_tethering) = {
            let state = self.lock_state();
            (
                state
                    .wifi_adapters
                    .iter()
                    .any(|a| a.interface_name == adapter_name),
                adapter_name == "tethering",
            )
        };

        if is_tethering {
            match action {
                "activate" => self.handle_tethering_smart_toggle(true).await?,
                "deactivate" => self.handle_tethering_smart_toggle(false).await?,
                _ => debug!("[nm] Unknown tethering adapter action: {action}"),
            }
        } else if is_wifi {
            match action {
                "activate" => self.handle_toggle_wifi_on().await?,
                "deactivate" => self.handle_toggle_wifi_off().await?,
                "scan" => {
                    if let Err(e) = self.scan_tx.send(()).await {
                        warn!("[nm] Failed to send scan request: {e}");
                    }
                }
                "connect" => {
                    if let Some(ssid) = params.get("ssid").and_then(|v| v.as_str()) {
                        self.handle_connect_wifi(ssid, params).await?;
                    } else {
                        warn!("[nm] connect action missing ssid param");
                    }
                }
                "disconnect" => {
                    let device_path = {
                        let state = self.lock_state();
                        state
                            .wifi_adapters
                            .iter()
                            .find(|a| a.interface_name == adapter_name)
                            .map(|a| a.path.clone())
                    };
                    if let Some(path) = device_path {
                        self.handle_disconnect_wifi(&path).await?;
                    }
                }
                _ => debug!("[nm] Unknown WiFi action: {action}"),
            }
        } else {
            // Ethernet adapter
            match action {
                "activate" | "deactivate" => {
                    let device_path = {
                        let state = self.lock_state();
                        state
                            .ethernet_adapters
                            .iter()
                            .find(|a| a.interface_name == adapter_name)
                            .map(|a| a.path.clone())
                    };
                    if let Some(path) = device_path {
                        self.handle_toggle_wired(&path).await?;
                    }
                }
                _ => debug!("[nm] Unknown Ethernet action: {action}"),
            }
        }

        Ok(())
    }

    async fn handle_wifi_network_action(
        &self,
        _urn: &Urn,
        ssid: &str,
        action: &str,
        params: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        match action {
            "connect" => {
                debug!("[nm] Connect to WiFi network: {ssid}");
                self.handle_connect_wifi(ssid, params).await?;
            }
            "disconnect" => {
                debug!("[nm] Disconnect WiFi network: {ssid}");
                let device_path = {
                    let state = self.lock_state();
                    state
                        .wifi_adapters
                        .iter()
                        .find(|a| a.active_ssid.as_ref() == Some(&ssid.to_string()))
                        .map(|a| a.path.clone())
                };
                if let Some(path) = device_path {
                    self.handle_disconnect_wifi(&path).await?;
                } else {
                    warn!("[nm] Cannot disconnect - WiFi adapter not found for: {ssid}");
                }
            }
            "forget" => {
                info!("[nm] Forget WiFi network: {ssid}");

                // If currently connected, disconnect first
                let device_path = {
                    let state = self.lock_state();
                    state
                        .wifi_adapters
                        .iter()
                        .find(|a| a.active_ssid.as_ref() == Some(&ssid.to_string()))
                        .map(|a| a.path.clone())
                };
                if let Some(ref path) = device_path
                    && let Err(e) = self.handle_disconnect_wifi(path).await
                {
                    warn!("[nm] Failed to disconnect before forget: {e}");
                }

                // Delete all saved connection profiles for this SSID
                let saved = self.nm.list_saved_connections().await?;
                let mut deleted = 0usize;
                for conn in saved {
                    if let nmrs::models::SettingsSummary::Wifi {
                        ssid: saved_ssid, ..
                    } = &conn.summary
                        && saved_ssid == ssid
                    {
                        if let Err(e) = self.nm.delete_saved_connection(&conn.uuid).await {
                            error!("[nm] Failed to delete connection {}: {e}", conn.uuid);
                            return Err(e.into());
                        }
                        deleted += 1;
                    }
                }
                if deleted == 0 {
                    warn!("[nm] No saved connections found for SSID: {ssid}");
                } else {
                    info!("[nm] Deleted {deleted} connection(s) for SSID: {ssid}");
                }

                // Update state: mark network as not known, clear active SSID if needed
                {
                    let mut state = self.lock_state();
                    for adapter in &mut state.wifi_adapters {
                        if adapter.active_ssid.as_deref() == Some(ssid) {
                            adapter.active_ssid = None;
                        }
                        for ap in &mut adapter.access_points {
                            if ap.ssid == ssid {
                                ap.known = false;
                            }
                        }
                    }
                }
            }
            "update-settings" => {
                debug!("[nm] Update settings for WiFi network: {ssid}");

                let saved = self.nm.list_saved_connections().await?;
                let conn = saved
                    .into_iter()
                    .find(|conn| matches!(&conn.summary, nmrs::models::SettingsSummary::Wifi { ssid: saved_ssid, .. } if saved_ssid == ssid))
                    .ok_or_else(|| anyhow::anyhow!("No saved connection for SSID: {ssid}"))?;

                let patch = build_wifi_settings_patch(params);
                self.nm.update_saved_connection(&conn.uuid, patch).await?;
                info!("[nm] Updated settings for WiFi network: {ssid}");
            }
            "share" => {
                debug!("[nm] Share WiFi network: {ssid}");

                let security_type = {
                    let state = self.lock_state();
                    state
                        .wifi_adapters
                        .iter()
                        .flat_map(|a| &a.access_points)
                        .find(|ap| ap.ssid == ssid)
                        .map(|ap| ap.security_type)
                        .unwrap_or_default()
                };

                let connections = get_connections_for_ssid(&self.conn, ssid).await?;
                let psk = if let Some(conn_path) = connections.first() {
                    get_wifi_psk(&self.conn, conn_path).await?
                } else {
                    None
                };

                let qr_string = build_wifi_qr_string(ssid, psk.as_deref(), security_type);
                info!("[nm] Generated WiFi QR string for SSID: {ssid}");
                return Ok(serde_json::json!({ "qr_string": qr_string }));
            }
            _ => {
                debug!("[nm] Unknown wifi-network action: {action}");
            }
        }
        Ok(serde_json::Value::Null)
    }

    async fn handle_ethernet_connection_action(
        &self,
        urn: &Urn,
        uuid: &str,
        action: &str,
    ) -> anyhow::Result<()> {
        match action {
            "activate" => {
                info!("[nm] Activate ethernet connection: {uuid}");

                // Find the connection path and device path
                let (conn_path, device_path) = {
                    let state = self.lock_state();
                    let mut result = (None, None);
                    for adapter in &state.ethernet_adapters {
                        if let Some(profile) = adapter.profiles.iter().find(|p| p.uuid == uuid) {
                            result = (Some(profile.path.clone()), Some(adapter.path.clone()));
                            break;
                        }
                    }
                    result
                };

                if let (Some(conn_path), Some(device_path)) = (conn_path, device_path) {
                    match activate_ethernet_connection(&self.conn, &conn_path, &device_path).await {
                        Ok(_) => {
                            info!("[nm] Ethernet connection activated: {uuid}");
                            let mut state = self.lock_state();
                            for adapter in &mut state.ethernet_adapters {
                                if adapter.path == device_path {
                                    adapter.active_connection_uuid = Some(uuid.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            error!("[nm] Failed to activate ethernet connection: {e}");
                            return Err(e);
                        }
                    }
                } else {
                    warn!(
                        "[nm] Ethernet connection not found: {} (urn: {})",
                        uuid,
                        urn.as_str()
                    );
                }
            }
            "deactivate" => {
                info!("[nm] Deactivate ethernet connection: {uuid}");

                // Find the device path
                let device_path = {
                    let state = self.lock_state();
                    state
                        .ethernet_adapters
                        .iter()
                        .find(|a| a.active_connection_uuid.as_deref() == Some(uuid))
                        .map(|a| a.path.clone())
                };

                if let Some(device_path) = device_path {
                    match deactivate_ethernet_connection(&self.conn, &device_path).await {
                        Ok(()) => {
                            info!("[nm] Ethernet connection deactivated: {uuid}");
                            let mut state = self.lock_state();
                            for adapter in &mut state.ethernet_adapters {
                                if adapter.path == device_path {
                                    adapter.active_connection_uuid = None;
                                }
                            }
                        }
                        Err(e) => {
                            error!("[nm] Failed to deactivate ethernet connection: {e}");
                            return Err(e);
                        }
                    }
                } else {
                    warn!("[nm] No active ethernet connection with UUID: {uuid}");
                }
            }
            _ => {
                debug!("[nm] Unknown ethernet-connection action: {action}");
            }
        }
        Ok(())
    }

    async fn handle_vpn_action(&self, vpn_name: &str, action: &str) -> anyhow::Result<()> {
        match action {
            "connect" => {
                let vpn = {
                    let state = self.lock_state();
                    state
                        .vpn_connections
                        .iter()
                        .find(|v| v.name == vpn_name)
                        .map(|v| (v.uuid.clone(), v.name.clone(), v.conn_type.clone()))
                };
                if let Some((uuid, name, conn_type)) = vpn {
                    self.handle_connect_vpn(&uuid, &name, &conn_type).await?;
                } else {
                    warn!("[nm] VPN not found: {vpn_name}");
                }
            }
            "disconnect" => {
                let vpn = {
                    let state = self.lock_state();
                    state
                        .vpn_connections
                        .iter()
                        .find(|v| v.name == vpn_name)
                        .map(|v| (v.uuid.clone(), v.name.clone()))
                };
                if let Some((uuid, name)) = vpn {
                    self.handle_disconnect_vpn(&uuid, &name).await?;
                } else {
                    warn!("[nm] VPN not found: {vpn_name}");
                }
            }
            _ => debug!("[nm] Unknown VPN action: {action}"),
        }

        Ok(())
    }

    async fn handle_toggle_wifi_on(&self) -> anyhow::Result<()> {
        {
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.busy = true;
            }
        }

        if let Err(e) = self.nm.set_wireless_enabled(true).await {
            error!("[nm] Failed to enable WiFi: {e}");
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.busy = false;
            }
            return Err(e.into());
        }

        {
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.enabled = true;
                adapter.busy = false;
            }
        }

        // Trigger a scan after enabling WiFi
        if let Err(e) = self.scan_tx.send(()).await {
            warn!("[nm] Failed to send scan request: {e}");
        }

        Ok(())
    }

    async fn handle_toggle_wifi_off(&self) -> anyhow::Result<()> {
        {
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.busy = true;
            }
        }

        if let Err(e) = self.nm.set_wireless_enabled(false).await {
            error!("[nm] Failed to disable WiFi: {e}");
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.busy = false;
            }
            return Err(e.into());
        }

        {
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.enabled = false;
                adapter.busy = false;
                adapter.active_ssid = None;
                adapter.access_points.clear();
            }
        }

        Ok(())
    }

    async fn handle_connect_wifi(
        &self,
        ssid: &str,
        params: &serde_json::Value,
    ) -> anyhow::Result<()> {
        info!("[nm] Connecting to WiFi: {ssid}");

        let password = params.get("password").and_then(|v| v.as_str());
        let (interface_name, device_path, ap_info) = {
            let state = self.lock_state();
            let adapter = state.wifi_adapters.first().cloned();
            let ap = state
                .wifi_adapters
                .iter()
                .flat_map(|a| a.access_points.iter())
                .find(|ap| ap.ssid == ssid)
                .cloned();
            (
                adapter.as_ref().map(|a| a.interface_name.clone()),
                adapter.as_ref().map(|a| a.path.clone()),
                ap,
            )
        };

        let Some(interface_name) = interface_name else {
            anyhow::bail!("No WiFi adapter available");
        };

        let fallback_legacy = match ap_info.as_ref() {
            Some(ap) => matches!(
                ap.security_type,
                SecurityType::Wep | SecurityType::Enterprise
            ),
            None => false,
        };

        if fallback_legacy {
            let connections = get_connections_for_ssid(&self.conn, ssid).await?;
            if let Some(conn_path) = connections.first() {
                let Some(device_path) = device_path.clone() else {
                    anyhow::bail!("No WiFi adapter available");
                };
                activate_connection(&self.conn, Some(conn_path), &device_path, None).await?;
            } else {
                let Some(device_path) = device_path.clone() else {
                    anyhow::bail!("No WiFi adapter available");
                };
                let Some(ap) = ap_info.as_ref() else {
                    anyhow::bail!("Access point not found for SSID: {ssid}");
                };
                if ap.security_type != SecurityType::Open && password.is_none() {
                    if ap.security_type == SecurityType::Enterprise {
                        anyhow::bail!("enterprise-not-supported");
                    }
                    anyhow::bail!("password-required");
                }
                add_and_activate_connection(
                    &self.conn,
                    &device_path,
                    &ap.ap_path,
                    ssid,
                    ap.security_type,
                    password,
                )
                .await?;
            }

            let mut state = self.lock_state();
            state.connecting_ssid = None;
            for adapter in &mut state.wifi_adapters {
                if adapter.interface_name == interface_name {
                    adapter.active_ssid = Some(ssid.to_string());
                }
            }
            return Ok(());
        }

        let known = ap_info.as_ref().map(|ap| ap.known).unwrap_or(true);
        let security_type = ap_info
            .as_ref()
            .map(|ap| ap.security_type)
            .unwrap_or(SecurityType::Open);
        let creds = build_nmrs_wifi_security(security_type, password, known)?;

        {
            let mut state = self.lock_state();
            state.connecting_ssid = Some(ssid.to_string());
        }

        match self.nm.wifi(&interface_name).connect(ssid, creds).await {
            Ok(_) => {
                info!("[nm] WiFi connection activated for {ssid}");
                let mut state = self.lock_state();
                state.connecting_ssid = None;
                for adapter in &mut state.wifi_adapters {
                    if adapter.interface_name == interface_name {
                        adapter.active_ssid = Some(ssid.to_string());
                    }
                }
            }
            Err(e) => {
                error!("[nm] Failed to connect WiFi via nmrs: {e}");
                let mut state = self.lock_state();
                state.connecting_ssid = None;
                return Err(anyhow::anyhow!(e));
            }
        }

        Ok(())
    }

    async fn handle_disconnect_wifi(&self, device_path: &str) -> anyhow::Result<()> {
        info!("[nm] Disconnecting WiFi: {device_path}");

        let interface_name = {
            let state = self.lock_state();
            state
                .wifi_adapters
                .iter()
                .find(|a| a.path == device_path)
                .map(|a| a.interface_name.clone())
        };

        let Some(interface_name) = interface_name else {
            anyhow::bail!("WiFi adapter not found for path: {device_path}");
        };

        if let Err(e) = self.nm.wifi(&interface_name).disconnect().await {
            error!("[nm] Failed to disconnect WiFi via nmrs: {e}");
            return Err(anyhow::anyhow!(e));
        }

        {
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                if adapter.path == device_path {
                    adapter.active_ssid = None;
                }
            }
        }

        Ok(())
    }

    async fn handle_toggle_wired(&self, device_path: &str) -> anyhow::Result<()> {
        let (is_connected, interface_name, adapter_count) = {
            let state = self.lock_state();
            (
                state
                    .ethernet_adapters
                    .iter()
                    .find(|a| a.path == device_path)
                    .map(waft_plugin_networkmanager::state::EthernetAdapterState::is_connected)
                    .unwrap_or(false),
                state
                    .ethernet_adapters
                    .iter()
                    .find(|a| a.path == device_path)
                    .map(|a| a.interface_name.clone()),
                state.ethernet_adapters.len(),
            )
        };

        if is_connected {
            let Some(interface_name) = interface_name else {
                anyhow::bail!("Ethernet adapter not found for path: {device_path}");
            };
            info!("[nm] Disconnecting wired: {interface_name}");
            if let Err(e) = self.nm.disconnect(Some(&interface_name)).await {
                error!("[nm] Failed to disconnect wired via nmrs: {e}");
                return Err(e.into());
            }
        } else {
            info!("[nm] Connecting wired: {device_path}");
            if adapter_count <= 1 {
                if let Err(e) = self.nm.connect_wired().await {
                    error!("[nm] Failed to connect wired via nmrs: {e}");
                    return Err(e.into());
                }
            } else if let Err(e) = connect_wired_dbus(&self.conn, device_path).await {
                error!("[nm] Failed to connect wired: {e}");
                return Err(e);
            }
        }

        Ok(())
    }

    async fn handle_connect_vpn(
        &self,
        uuid: &str,
        name: &str,
        conn_type: &str,
    ) -> anyhow::Result<()> {
        info!("[nm] Connecting {conn_type} VPN: {name} ({uuid})");

        {
            let mut state = self.lock_state();
            if let Some(vpn) = state.vpn_connections.iter_mut().find(|v| v.uuid == uuid) {
                vpn.state = VpnState::Connecting;
            }
        }

        if let Err(e) = self.nm.connect_vpn_by_uuid(uuid).await {
            error!("[nm] Failed to connect {conn_type} VPN {name} ({uuid}): {e}");
            let mut state = self.lock_state();
            if let Some(vpn) = state.vpn_connections.iter_mut().find(|v| v.uuid == uuid) {
                vpn.state = VpnState::Disconnected;
            }
            return Err(e.into());
        }

        Ok(())
    }

    async fn handle_disconnect_vpn(&self, uuid: &str, name: &str) -> anyhow::Result<()> {
        info!("[nm] Disconnecting VPN: {name} ({uuid})");

        {
            let mut state = self.lock_state();
            if let Some(vpn) = state.vpn_connections.iter_mut().find(|v| v.uuid == uuid) {
                vpn.state = VpnState::Disconnecting;
            }
        }

        if let Err(e) = self.nm.disconnect_vpn_by_uuid(uuid).await {
            error!("[nm] Failed to disconnect VPN {name} ({uuid}): {e}");
            let mut state = self.lock_state();
            if let Some(vpn) = state.vpn_connections.iter_mut().find(|v| v.uuid == uuid) {
                vpn.state = VpnState::Connected;
            }
            return Err(e.into());
        }

        Ok(())
    }

    async fn handle_tethering_connection_action(
        &self,
        uuid: &str,
        action: &str,
    ) -> anyhow::Result<()> {
        match action {
            "connect" => {
                if self
                    .lock_state()
                    .tethering_connections
                    .iter()
                    .any(|c| c.uuid == uuid)
                {
                    self.handle_connect_tethering(uuid).await?;
                } else {
                    warn!("[nm] Tethering connection not found: {uuid}");
                }
            }
            "disconnect" => {
                if self
                    .lock_state()
                    .tethering_connections
                    .iter()
                    .any(|c| c.uuid == uuid)
                {
                    self.handle_disconnect_tethering(uuid).await?;
                } else {
                    warn!("[nm] No active tethering connection for: {uuid}");
                }
            }
            _ => debug!("[nm] Unknown tethering-connection action: {action}"),
        }
        Ok(())
    }

    async fn handle_tethering_smart_toggle(&self, connect: bool) -> anyhow::Result<()> {
        if connect {
            // Connect the first available tethering profile
            let conn_uuid = {
                let state = self.lock_state();
                state
                    .tethering_connections
                    .iter()
                    .find(|c| !c.active)
                    .map(|c| c.uuid.clone())
            };
            if let Some(uuid) = conn_uuid {
                self.handle_connect_tethering(&uuid).await?;
            } else {
                debug!("[nm] No inactive tethering connections to activate");
            }
        } else {
            // Disconnect all active tethering connections
            let active_connections: Vec<String> = {
                let state = self.lock_state();
                state
                    .tethering_connections
                    .iter()
                    .filter(|c| c.active)
                    .map(|c| c.uuid.clone())
                    .collect()
            };
            for uuid in active_connections {
                self.handle_disconnect_tethering(&uuid).await?;
            }
        }
        Ok(())
    }

    async fn handle_connect_tethering(&self, uuid: &str) -> anyhow::Result<()> {
        let (name, bdaddr) = {
            let state = self.lock_state();
            state
                .tethering_connections
                .iter()
                .find(|c| c.uuid == uuid)
                .map(|c| (c.name.clone(), c.bdaddr.clone()))
                .ok_or_else(|| anyhow::anyhow!("Tethering connection not found: {uuid}"))?
        };

        let bdaddr = bdaddr.ok_or_else(|| {
            anyhow::anyhow!("Tethering profile missing Bluetooth address: {uuid}")
        })?;
        info!("[nm] Connecting tethering: {name} ({bdaddr})");

        let bt_device = self
            .nm
            .list_bluetooth_devices()
            .await?
            .into_iter()
            .find(|device| device.bdaddr == bdaddr)
            .ok_or_else(|| {
                anyhow::anyhow!("No Bluetooth device found for tethering address: {bdaddr}")
            })?;

        let identity =
            nmrs::models::BluetoothIdentity::new(bdaddr.clone(), bt_device.bt_caps.into())?;

        if let Err(e) = self.nm.connect_bluetooth(&name, &identity).await {
            error!("[nm] Failed to connect tethering via nmrs: {e}");
            return Err(e.into());
        }

        let mut state = self.lock_state();
        if let Some(conn) = state
            .tethering_connections
            .iter_mut()
            .find(|c| c.uuid == uuid)
        {
            conn.active = true;
        }

        Ok(())
    }

    async fn handle_disconnect_tethering(&self, uuid: &str) -> anyhow::Result<()> {
        let (active_path, bdaddr) = {
            let state = self.lock_state();
            state
                .tethering_connections
                .iter()
                .find(|c| c.uuid == uuid)
                .map(|c| (c.active_path.clone(), c.bdaddr.clone()))
                .ok_or_else(|| anyhow::anyhow!("Tethering connection not found: {uuid}"))?
        };

        let interface = if let Some(bdaddr) = bdaddr.as_ref() {
            self.nm
                .list_devices()
                .await?
                .into_iter()
                .find(|device| {
                    device.is_bluetooth()
                        && (device.identity.current_mac == *bdaddr
                            || device.identity.permanent_mac == *bdaddr)
                })
                .map(|device| device.interface)
        } else {
            None
        };

        if let Some(interface) = interface {
            if let Err(e) = self.nm.disconnect(Some(&interface)).await {
                error!("[nm] Failed to disconnect tethering via nmrs: {e}");
                return Err(e.into());
            }
        } else if let Some(active_path) = active_path.as_ref() {
            if let Err(e) = deactivate_tethering(&self.conn, active_path).await {
                error!("[nm] Failed to disconnect tethering: {e}");
                return Err(e);
            }
        } else {
            anyhow::bail!("No tethering interface or active connection found for: {uuid}");
        }

        info!("[nm] Tethering disconnected: {uuid}");
        let mut state = self.lock_state();
        if let Some(conn) = state
            .tethering_connections
            .iter_mut()
            .find(|c| c.uuid == uuid)
        {
            conn.active = false;
            conn.active_path = None;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    PluginRunner::new(
        "networkmanager",
        &[
            ADAPTER_ENTITY_TYPE,
            WIFI_NETWORK_ENTITY_TYPE,
            ETHERNET_CONNECTION_ENTITY_TYPE,
            VPN_ENTITY_TYPE,
            TETHERING_CONNECTION_ENTITY_TYPE,
        ],
    )
    .i18n(i18n(), "plugin-name", "plugin-description")
    .run(|notifier| async move {
        let (scan_tx, scan_rx) = tokio::sync::mpsc::channel::<()>(4);

        let plugin = NetworkManagerPlugin::new(scan_tx).await?;

        let shared_state = plugin.shared_state();
        let monitor_conn = plugin.conn.clone();
        let monitor_nm = plugin.nm.clone();
        let scan_conn = plugin.conn.clone();
        let scan_nm = plugin.nm.clone();

        // Monitor NM D-Bus signals
        let monitor_state = shared_state.clone();
        let monitor_notifier = notifier.clone();
        spawn_monitored("nm/signal-monitor", async move {
            monitor_nm_signals(monitor_conn, monitor_nm, monitor_state, monitor_notifier).await
        });

        // Monitor BlueZ D-Bus signals (paired device connection state for tethering).
        // Uses a dedicated system bus connection — sharing the NM connection causes
        // missed signals due to match rule/stream contention in zbus.
        let bluez_state = shared_state.clone();
        let bluez_notifier = notifier.clone();
        spawn_monitored("nm/bluez-monitor", async move {
            let bluez_conn = Connection::system().await?;
            monitor_bluez_signals(bluez_conn, bluez_state, bluez_notifier).await
        });

        // WiFi scan background task — pure D-Bus, runs on main tokio runtime
        let scan_state = shared_state.clone();
        let scan_notifier = notifier.clone();
        tokio::spawn(async move {
            wifi_scan_task(scan_rx, scan_conn, scan_nm, scan_state, scan_notifier).await;
        });

        Ok(plugin)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // nm_metered_to_entity tests

    #[test]
    fn metered_unknown_from_zero() {
        assert_eq!(nm_metered_to_entity(0), MeteredState::Unknown);
    }

    #[test]
    fn metered_yes() {
        assert_eq!(nm_metered_to_entity(1), MeteredState::Yes);
    }

    #[test]
    fn metered_no() {
        assert_eq!(nm_metered_to_entity(2), MeteredState::No);
    }

    #[test]
    fn metered_guess_yes() {
        assert_eq!(nm_metered_to_entity(3), MeteredState::GuessYes);
    }

    #[test]
    fn metered_guess_no() {
        assert_eq!(nm_metered_to_entity(4), MeteredState::GuessNo);
    }

    #[test]
    fn metered_unknown_from_negative() {
        assert_eq!(nm_metered_to_entity(-1), MeteredState::Unknown);
    }

    #[test]
    fn metered_unknown_from_out_of_range() {
        assert_eq!(nm_metered_to_entity(5), MeteredState::Unknown);
        assert_eq!(nm_metered_to_entity(99), MeteredState::Unknown);
    }

    // nm_ip_method_to_entity tests

    #[test]
    fn ip_method_auto() {
        assert_eq!(nm_ip_method_to_entity("auto"), IpMethod::Auto);
    }

    #[test]
    fn ip_method_manual() {
        assert_eq!(nm_ip_method_to_entity("manual"), IpMethod::Manual);
    }

    #[test]
    fn ip_method_link_local() {
        assert_eq!(nm_ip_method_to_entity("link-local"), IpMethod::LinkLocal);
    }

    #[test]
    fn ip_method_disabled() {
        assert_eq!(nm_ip_method_to_entity("disabled"), IpMethod::Disabled);
    }

    #[test]
    fn ip_method_unknown_defaults_to_auto() {
        assert_eq!(nm_ip_method_to_entity("shared"), IpMethod::Auto);
        assert_eq!(nm_ip_method_to_entity(""), IpMethod::Auto);
        assert_eq!(nm_ip_method_to_entity("something-else"), IpMethod::Auto);
    }
}
