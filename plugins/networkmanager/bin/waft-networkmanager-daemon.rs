//! NetworkManager daemon - WiFi, Wired, and VPN network management.
//!
//! Provides entity types:
//! - `network-adapter`: WiFi and Ethernet adapters with connection state
//! - `vpn`: VPN connection profiles with state
//!
//! Monitors NetworkManager D-Bus signals for device/connection state changes.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_i18n::I18n;
use waft_plugin::entity::network::{
    ADAPTER_ENTITY_TYPE, AdapterKind, ETHERNET_CONNECTION_ENTITY_TYPE, NetworkAdapter,
    TETHERING_CONNECTION_ENTITY_TYPE, TetheringConnection, VPN_ENTITY_TYPE,
    VpnState as EntityVpnState, WIFI_NETWORK_ENTITY_TYPE, WiFiNetwork,
};
use waft_plugin::*;
use zbus::Connection;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            (
                "en-US",
                include_str!("../locales/en-US/networkmanager.ftl"),
            ),
            (
                "cs-CZ",
                include_str!("../locales/cs-CZ/networkmanager.ftl"),
            ),
        ])
    })
}

use waft_plugin_networkmanager::bluez_discovery::discover_bluez_paired_devices;
use waft_plugin_networkmanager::bluez_signal_monitor::monitor_bluez_signals;
use waft_plugin_networkmanager::dbus_property::{DEVICE_TYPE_ETHERNET, DEVICE_TYPE_WIFI};
use waft_plugin_networkmanager::device_discovery::{discover_bluetooth_devices, discover_devices};
use waft_plugin_networkmanager::ethernet::{
    activate_ethernet_connection, deactivate_ethernet_connection,
};
use waft_plugin_networkmanager::ip_config::{fetch_public_ip, get_device_ip4_config};
use waft_plugin_networkmanager::signal_monitor::monitor_nm_signals;
use waft_plugin_networkmanager::state::{
    CachedIpConfig, EthernetAdapterState, NmState, TetheringConnectionState, VpnState,
    WiFiAdapterState,
};
use waft_plugin_networkmanager::tethering::{
    activate_tethering, deactivate_tethering, get_active_tethering_connections,
    get_tethering_profiles,
};
use waft_plugin_networkmanager::vpn::{
    activate_vpn, deactivate_vpn, get_active_vpn_connections, get_vpn_profiles,
};
use waft_plugin_networkmanager::wifi::{
    activate_connection, connect_wired_dbus, disconnect_device, get_connections_for_ssid,
    set_wifi_enabled_dbus,
};
use waft_plugin_networkmanager::wifi_scan::wifi_scan_task;

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct NetworkManagerPlugin {
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    /// Channel to request WiFi scan from background task.
    scan_tx: tokio::sync::mpsc::Sender<()>,
}

impl NetworkManagerPlugin {
    async fn new(scan_tx: tokio::sync::mpsc::Sender<()>) -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

        let mut state = NmState::default();

        // Discover devices
        match discover_devices(&conn).await {
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
                error!("[nm] Failed to discover devices: {}", e);
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

        // Fetch public IP if any adapter is connected
        let any_connected = state.ethernet_adapters.iter().any(|a| a.is_connected())
            || state.wifi_adapters.iter().any(|a| a.active_ssid.is_some());
        if any_connected && let Some(public_ip) = fetch_public_ip().await {
            debug!("[nm] Public IP: {}", public_ip);
            state.public_ip = Some(public_ip);
        }

        // Discover ethernet connection profiles
        match waft_plugin_networkmanager::ethernet::get_ethernet_profiles(&conn).await {
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
                error!("[nm] Failed to get ethernet profiles: {}", e);
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

                    state.vpn_connections.push(
                        waft_plugin_networkmanager::state::VpnConnectionInfo {
                            path: profile.path,
                            uuid: profile.uuid,
                            name: profile.name,
                            state: vpn_state,
                            active_path,
                        },
                    );
                }
            }
            Err(e) => {
                error!("[nm] Failed to get VPN profiles: {}", e);
            }
        }

        // Discover bluetooth NM devices (tethering is only visible when one is ready)
        match discover_bluetooth_devices(&conn).await {
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
                warn!("[nm] Failed to discover bluetooth devices: {}", e);
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
                warn!("[nm] Failed to discover BlueZ paired devices: {}", e);
            }
        }

        // Discover tethering (bluetooth) connections
        match get_tethering_profiles(&conn).await {
            Ok(profiles) => {
                info!("[nm] Found {} tethering profiles", profiles.len());

                let active = get_active_tethering_connections(&conn)
                    .await
                    .unwrap_or_default();

                for profile in profiles {
                    let active_info = active.iter().find(|(_, uuid)| *uuid == profile.uuid);

                    debug!(
                        "[nm] Tethering {}: path={}, active={}, bdaddr={:?}",
                        profile.name,
                        profile.path,
                        active_info.is_some(),
                        profile.bdaddr
                    );

                    state.tethering_connections.push(TetheringConnectionState {
                        path: profile.path,
                        uuid: profile.uuid,
                        name: profile.name,
                        active: active_info.is_some(),
                        active_path: active_info.map(|(ap, _)| ap.clone()),
                        bdaddr: profile.bdaddr,
                    });
                }
            }
            Err(e) => {
                error!("[nm] Failed to get tethering profiles: {}", e);
            }
        }

        let plugin = Self {
            conn,
            state: Arc::new(StdMutex::new(state)),
            scan_tx,
        };

        Ok(plugin)
    }

    fn shared_state(&self) -> Arc<StdMutex<NmState>> {
        self.state.clone()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, NmState> {
        match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[nm] Mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
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

fn wifi_adapter_to_entities(adapter: &WiFiAdapterState) -> Vec<Entity> {
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
        let network_entity = WiFiNetwork {
            ssid: ap.ssid.clone(),
            strength: ap.strength,
            secure: ap.secure,
            known: true, // TODO: Check if network has saved profile
            connected: adapter.active_ssid.as_ref() == Some(&ap.ssid),
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
    let entity = waft_plugin::entity::network::Vpn {
        name: vpn.name.clone(),
        state: to_entity_vpn_state(&vpn.state),
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
            entities.extend(wifi_adapter_to_entities(adapter));
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let entity_type = urn.entity_type();

        match entity_type {
            "network-adapter" => {
                let adapter_id = urn.id();
                self.handle_adapter_action(adapter_id, &action, &params)
                    .await?
            }
            "wifi-network" => {
                let ssid = urn.id();
                self.handle_wifi_network_action(&urn, ssid, &action).await?
            }
            "ethernet-connection" => {
                let uuid = urn.id();
                self.handle_ethernet_connection_action(&urn, uuid, &action)
                    .await?
            }
            "vpn" => {
                let vpn_id = urn.id();
                self.handle_vpn_action(vpn_id, &action).await?
            }
            "tethering-connection" => {
                let uuid = urn.id();
                self.handle_tethering_connection_action(uuid, &action)
                    .await?
            }
            _ => {
                debug!("[nm] Unknown entity type: {}", entity_type);
            }
        }

        Ok(())
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
                        self.handle_connect_wifi(ssid).await?;
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action {
            "connect" => {
                debug!("[nm] Connect to WiFi network: {}", ssid);
                self.handle_connect_wifi(ssid).await?;
            }
            "disconnect" => {
                debug!("[nm] Disconnect WiFi network: {}", ssid);
                // Find the WiFi adapter and disconnect it
                let device_path = {
                    let state = self.lock_state();
                    state
                        .wifi_adapters
                        .iter()
                        .find(|a| a.active_ssid.as_ref() == Some(&ssid.to_string()))
                        .map(|a| a.path.clone())
                };
                if let Some(path) = device_path {
                    disconnect_device(&self.conn, &path).await?;
                } else {
                    warn!(
                        "[nm] Cannot disconnect - WiFi adapter not found for: {}",
                        ssid
                    );
                }
            }
            _ => {
                debug!("[nm] Unknown wifi-network action: {}", action);
            }
        }
        Ok(())
    }

    async fn handle_ethernet_connection_action(
        &self,
        urn: &Urn,
        uuid: &str,
        action: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action {
            "activate" => {
                info!("[nm] Activate ethernet connection: {}", uuid);

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
                            info!("[nm] Ethernet connection activated: {}", uuid);
                            let mut state = self.lock_state();
                            for adapter in &mut state.ethernet_adapters {
                                if adapter.path == device_path {
                                    adapter.active_connection_uuid = Some(uuid.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            error!("[nm] Failed to activate ethernet connection: {}", e);
                            return Err(e.into());
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
                info!("[nm] Deactivate ethernet connection: {}", uuid);

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
                            info!("[nm] Ethernet connection deactivated: {}", uuid);
                            let mut state = self.lock_state();
                            for adapter in &mut state.ethernet_adapters {
                                if adapter.path == device_path {
                                    adapter.active_connection_uuid = None;
                                }
                            }
                        }
                        Err(e) => {
                            error!("[nm] Failed to deactivate ethernet connection: {}", e);
                            return Err(e.into());
                        }
                    }
                } else {
                    warn!("[nm] No active ethernet connection with UUID: {}", uuid);
                }
            }
            _ => {
                debug!("[nm] Unknown ethernet-connection action: {}", action);
            }
        }
        Ok(())
    }

    async fn handle_vpn_action(
        &self,
        vpn_name: &str,
        action: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action {
            "connect" => {
                let conn_path = {
                    let state = self.lock_state();
                    state
                        .vpn_connections
                        .iter()
                        .find(|v| v.name == vpn_name)
                        .map(|v| v.path.clone())
                };
                if let Some(path) = conn_path {
                    self.handle_connect_vpn(&path).await?;
                } else {
                    warn!("[nm] VPN not found: {vpn_name}");
                }
            }
            "disconnect" => {
                let conn_path = {
                    let state = self.lock_state();
                    state
                        .vpn_connections
                        .iter()
                        .find(|v| v.name == vpn_name)
                        .map(|v| v.path.clone())
                };
                if let Some(path) = conn_path {
                    self.handle_disconnect_vpn(&path).await?;
                } else {
                    warn!("[nm] VPN not found: {vpn_name}");
                }
            }
            _ => debug!("[nm] Unknown VPN action: {action}"),
        }

        Ok(())
    }

    async fn handle_toggle_wifi_on(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.busy = true;
            }
        }

        if let Err(e) = set_wifi_enabled_dbus(&self.conn, true).await {
            error!("[nm] Failed to enable WiFi: {}", e);
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

    async fn handle_toggle_wifi_off(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut state = self.lock_state();
            for adapter in &mut state.wifi_adapters {
                adapter.busy = true;
            }
        }

        if let Err(e) = set_wifi_enabled_dbus(&self.conn, false).await {
            error!("[nm] Failed to disable WiFi: {}", e);
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Connecting to WiFi: {}", ssid);

        let connections = get_connections_for_ssid(&self.conn, ssid).await?;
        if let Some(conn_path) = connections.first() {
            let device_path = {
                let state = self.lock_state();
                state.wifi_adapters.first().map(|a| a.path.clone())
            };

            if let Some(ref device_path) = device_path {
                match activate_connection(&self.conn, Some(conn_path), device_path, None).await {
                    Ok(_) => {
                        info!("[nm] WiFi connection activated for {}", ssid);
                        let mut state = self.lock_state();
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

        Ok(())
    }

    async fn handle_disconnect_wifi(
        &self,
        device_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Disconnecting WiFi: {}", device_path);

        if let Err(e) = disconnect_device(&self.conn, device_path).await {
            error!("[nm] Failed to disconnect WiFi: {}", e);
            return Err(e.into());
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

    async fn handle_toggle_wired(
        &self,
        device_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let is_connected = {
            let state = self.lock_state();
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
            info!("[nm] Connecting wired: {}", device_path);
            if let Err(e) = connect_wired_dbus(&self.conn, device_path).await {
                error!("[nm] Failed to connect wired: {}", e);
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn handle_connect_vpn(
        &self,
        conn_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Connecting VPN: {}", conn_path);

        {
            let mut state = self.lock_state();
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
                let mut state = self.lock_state();
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
                let mut state = self.lock_state();
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

        Ok(())
    }

    async fn handle_disconnect_vpn(
        &self,
        conn_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Disconnecting VPN: {}", conn_path);

        let active_path = {
            let state = self.lock_state();
            state
                .vpn_connections
                .iter()
                .find(|v| v.path == conn_path)
                .and_then(|v| v.active_path.clone())
        };

        if let Some(ref active_path) = active_path {
            {
                let mut state = self.lock_state();
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
                let mut state = self.lock_state();
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

        Ok(())
    }

    async fn handle_tethering_connection_action(
        &self,
        uuid: &str,
        action: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action {
            "connect" => {
                let conn_path = {
                    let state = self.lock_state();
                    state
                        .tethering_connections
                        .iter()
                        .find(|c| c.uuid == uuid)
                        .map(|c| c.path.clone())
                };
                if let Some(path) = conn_path {
                    self.handle_connect_tethering(&path).await?;
                } else {
                    warn!("[nm] Tethering connection not found: {uuid}");
                }
            }
            "disconnect" => {
                let active_path = {
                    let state = self.lock_state();
                    state
                        .tethering_connections
                        .iter()
                        .find(|c| c.uuid == uuid)
                        .and_then(|c| c.active_path.clone())
                };
                if let Some(path) = active_path {
                    self.handle_disconnect_tethering(&path, uuid).await?;
                } else {
                    warn!("[nm] No active tethering connection for: {uuid}");
                }
            }
            _ => debug!("[nm] Unknown tethering-connection action: {action}"),
        }
        Ok(())
    }

    async fn handle_tethering_smart_toggle(
        &self,
        connect: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if connect {
            // Connect the first available tethering profile
            let conn_path = {
                let state = self.lock_state();
                state
                    .tethering_connections
                    .iter()
                    .find(|c| !c.active)
                    .map(|c| c.path.clone())
            };
            if let Some(path) = conn_path {
                self.handle_connect_tethering(&path).await?;
            } else {
                debug!("[nm] No inactive tethering connections to activate");
            }
        } else {
            // Disconnect all active tethering connections
            let active_connections: Vec<(String, String)> = {
                let state = self.lock_state();
                state
                    .tethering_connections
                    .iter()
                    .filter(|c| c.active)
                    .filter_map(|c| {
                        c.active_path
                            .as_ref()
                            .map(|ap| (ap.clone(), c.uuid.clone()))
                    })
                    .collect()
            };
            for (active_path, uuid) in active_connections {
                self.handle_disconnect_tethering(&active_path, &uuid)
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_connect_tethering(
        &self,
        conn_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Connecting tethering: {}", conn_path);

        match activate_tethering(&self.conn, conn_path).await {
            Ok(active_path) => {
                info!(
                    "[nm] Tethering connection initiated: {} -> {}",
                    conn_path, active_path
                );
                let mut state = self.lock_state();
                if let Some(conn) = state
                    .tethering_connections
                    .iter_mut()
                    .find(|c| c.path == conn_path)
                {
                    conn.active = true;
                    conn.active_path = Some(active_path);
                }
            }
            Err(e) => {
                error!("[nm] Failed to connect tethering: {}", e);
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn handle_disconnect_tethering(
        &self,
        active_path: &str,
        uuid: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Disconnecting tethering: {}", active_path);

        match deactivate_tethering(&self.conn, active_path).await {
            Ok(()) => {
                info!("[nm] Tethering disconnected: {}", uuid);
                let mut state = self.lock_state();
                if let Some(conn) = state
                    .tethering_connections
                    .iter_mut()
                    .find(|c| c.uuid == uuid)
                {
                    conn.active = false;
                    conn.active_path = None;
                }
            }
            Err(e) => {
                error!("[nm] Failed to disconnect tethering: {}", e);
                return Err(e.into());
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides_i18n(
        &[
            ADAPTER_ENTITY_TYPE,
            WIFI_NETWORK_ENTITY_TYPE,
            ETHERNET_CONNECTION_ENTITY_TYPE,
            VPN_ENTITY_TYPE,
            TETHERING_CONNECTION_ENTITY_TYPE,
        ],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    info!("Starting networkmanager plugin...");

    // Build the tokio runtime manually so `handle_provides` runs without it
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let (scan_tx, scan_rx) = tokio::sync::mpsc::channel::<()>(4);

        let plugin = NetworkManagerPlugin::new(scan_tx).await?;

        let shared_state = plugin.shared_state();
        let monitor_conn = plugin.conn.clone();
        let scan_conn = plugin.conn.clone();

        let (runtime, notifier) = PluginRuntime::new("networkmanager", plugin);

        let scan_notifier = notifier.clone();

        // Monitor NM D-Bus signals
        let monitor_state = shared_state.clone();
        let monitor_notifier = notifier.clone();
        tokio::spawn(async move {
            if let Err(e) = monitor_nm_signals(monitor_conn, monitor_state, monitor_notifier).await
            {
                error!("[nm] D-Bus signal monitoring failed: {e}");
            }
            warn!("[nm] D-Bus signal monitoring task stopped");
        });

        // Monitor BlueZ D-Bus signals (paired device connection state for tethering).
        // Uses a dedicated system bus connection — sharing the NM connection causes
        // missed signals due to match rule/stream contention in zbus.
        let bluez_state = shared_state.clone();
        let bluez_notifier = notifier.clone();
        tokio::spawn(async move {
            let bluez_conn = match Connection::system().await {
                Ok(c) => c,
                Err(e) => {
                    error!("[nm] Failed to connect to system bus for BlueZ: {e}");
                    return;
                }
            };
            if let Err(e) = monitor_bluez_signals(bluez_conn, bluez_state, bluez_notifier).await {
                error!("[nm] BlueZ signal monitoring failed: {e}");
            }
            warn!("[nm] BlueZ signal monitoring task stopped");
        });

        // WiFi scan background task — pure D-Bus, runs on main tokio runtime
        let scan_state = shared_state.clone();
        tokio::spawn(async move {
            wifi_scan_task(scan_rx, scan_conn, scan_state, scan_notifier).await;
        });

        runtime.run().await?;
        Ok(())
    })
}
