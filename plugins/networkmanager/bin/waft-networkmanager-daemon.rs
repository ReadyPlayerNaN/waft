//! NetworkManager daemon - WiFi, Wired, and VPN network management.
//!
//! Provides entity types:
//! - `network-adapter`: WiFi and Ethernet adapters with connection state
//! - `vpn`: VPN connection profiles with state
//!
//! Monitors NetworkManager D-Bus signals for device/connection state changes.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::entity::network::{
    AdapterKind, EthernetConnection, NetworkAdapter, VpnState as EntityVpnState, WiFiNetwork,
    ADAPTER_ENTITY_TYPE, ETHERNET_CONNECTION_ENTITY_TYPE, VPN_ENTITY_TYPE,
    WIFI_NETWORK_ENTITY_TYPE,
};
use waft_plugin::*;
use zbus::Connection;

use waft_plugin_networkmanager::dbus_property::{DEVICE_TYPE_ETHERNET, DEVICE_TYPE_WIFI};
use waft_plugin_networkmanager::device_discovery::discover_devices;
use waft_plugin_networkmanager::signal_monitor::monitor_nm_signals;
use waft_plugin_networkmanager::state::{
    EthernetAdapterState, NmState, VpnState, WiFiAdapterState,
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

                    state
                        .vpn_connections
                        .push(waft_plugin_networkmanager::state::VpnConnectionInfo {
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

        let plugin = Self {
            conn,
            state: Arc::new(StdMutex::new(state)),
            scan_tx,
        };

        Ok((plugin, nm))
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
    let adapter_urn = Urn::new("networkmanager", ADAPTER_ENTITY_TYPE, &adapter.interface_name);
    let adapter_entity = NetworkAdapter {
        name: adapter.interface_name.clone(),
        enabled: adapter.enabled,
        connected: adapter.active_ssid.is_some(),
        ip: None,
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

fn ethernet_adapter_to_entities(adapter: &EthernetAdapterState) -> Vec<Entity> {
    let mut entities = Vec::new();

    // Adapter entity
    let adapter_urn = Urn::new("networkmanager", ADAPTER_ENTITY_TYPE, &adapter.interface_name);
    let adapter_entity = NetworkAdapter {
        name: adapter.interface_name.clone(),
        enabled: true,
        connected: adapter.is_connected(),
        ip: None,
        kind: AdapterKind::Wired,
    };
    entities.push(Entity::new(
        adapter_urn.clone(),
        ADAPTER_ENTITY_TYPE,
        &adapter_entity,
    ));

    // TODO: Emit ethernet connection child entities when we have profile data

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
            entities.extend(ethernet_adapter_to_entities(adapter));
        }

        for vpn in &state.vpn_connections {
            entities.push(vpn_to_entity(vpn));
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
                self.handle_ethernet_connection_action(&urn, uuid, &action).await?
            }
            "vpn" => {
                let vpn_id = urn.id();
                self.handle_vpn_action(vpn_id, &action).await?
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
        // Determine if this is a WiFi or Ethernet adapter
        let is_wifi = {
            let state = self.lock_state();
            state
                .wifi_adapters
                .iter()
                .any(|a| a.interface_name == adapter_name)
        };

        if is_wifi {
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
                    warn!("[nm] Cannot disconnect - WiFi adapter not found for: {}", ssid);
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
                debug!("[nm] Activate ethernet connection: {}", uuid);
                // TODO: Implement ethernet connection activation
            }
            "deactivate" => {
                debug!("[nm] Deactivate ethernet connection: {}", uuid);
                // TODO: Implement ethernet connection deactivation
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

    async fn handle_toggle_wifi_on(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    async fn handle_toggle_wifi_off(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[
        ADAPTER_ENTITY_TYPE,
        WIFI_NETWORK_ENTITY_TYPE,
        ETHERNET_CONNECTION_ENTITY_TYPE,
        VPN_ENTITY_TYPE,
    ]) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    info!("Starting networkmanager plugin...");

    // Build the tokio runtime manually so `handle_provides` runs without it
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        // Create scan channel for WiFi scanning (uses nmrs which has non-Send futures)
        let (scan_tx, scan_rx) = tokio::sync::mpsc::channel::<()>(4);

        let (plugin, nm) = NetworkManagerPlugin::new(scan_tx).await?;

        let shared_state = plugin.shared_state();
        let monitor_conn = plugin.conn.clone();
        let scan_conn = plugin.conn.clone();

        let (runtime, notifier) = PluginRuntime::new("networkmanager", plugin);

        let scan_notifier = notifier.clone();

        // Monitor NM D-Bus signals
        let monitor_state = shared_state.clone();
        let monitor_notifier = notifier.clone();
        tokio::spawn(async move {
            if let Err(e) =
                monitor_nm_signals(monitor_conn, monitor_state, monitor_notifier).await
            {
                error!("[nm] D-Bus signal monitoring failed: {e}");
            }
            warn!("[nm] D-Bus signal monitoring task stopped");
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

        runtime.run().await?;
        Ok(())
    })
}
