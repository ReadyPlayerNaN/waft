//! NetworkManager daemon - WiFi, Wired, and VPN network management.
//!
//! Provides three NamedWidget entries:
//! - WiFi toggle with expandable network list (weight 100)
//! - Wired toggle with expandable IP details (weight 101)
//! - VPN toggle with expandable connection list (weight 103)
//!
//! Monitors NetworkManager D-Bus signals for device/connection state changes.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin_sdk::*;
use zbus::Connection;

use waft_plugin_networkmanager::dbus_property::{DEVICE_TYPE_ETHERNET, DEVICE_TYPE_WIFI};
use waft_plugin_networkmanager::device_discovery::discover_devices;
use waft_plugin_networkmanager::signal_monitor::monitor_nm_signals;
use waft_plugin_networkmanager::state::{
    EthernetAdapterState, NmState, VpnConnectionInfo, VpnState, WiFiAdapterState,
};
use waft_plugin_networkmanager::vpn::{
    activate_vpn, deactivate_vpn, get_active_vpn_connections, get_vpn_profiles,
};
use waft_plugin_networkmanager::widget_builder;
use waft_plugin_networkmanager::wifi::{
    activate_connection, connect_wired_dbus, disconnect_device, get_connections_for_ssid,
    set_wifi_enabled_dbus,
};
use waft_plugin_networkmanager::wifi_scan::wifi_scan_task;

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
}

// ---------------------------------------------------------------------------
// PluginDaemon implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl PluginDaemon for NetworkManagerDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = self.state.lock().unwrap();
        widget_builder::build_widgets(&state)
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let action_id = action.id.as_str();

        if action_id == "toggle_wifi" {
            self.handle_toggle_wifi().await?;
        } else if let Some(ssid) = action_id.strip_prefix("connect_wifi:") {
            self.handle_connect_wifi(ssid).await?;
        } else if let Some(device_path) = action_id.strip_prefix("disconnect_wifi:") {
            self.handle_disconnect_wifi(device_path).await?;
        } else if let Some(device_path) = action_id.strip_prefix("toggle_wired:") {
            self.handle_toggle_wired(device_path).await?;
        } else if action_id == "toggle_vpn" {
            self.handle_toggle_vpn().await?;
        } else if let Some(conn_path) = action_id.strip_prefix("connect_vpn:") {
            self.handle_connect_vpn(conn_path).await?;
        } else if let Some(conn_path) = action_id.strip_prefix("disconnect_vpn:") {
            self.handle_disconnect_vpn(conn_path).await?;
        } else if action_id == "scan_wifi" {
            let _ = self.scan_tx.send(()).await;
        } else {
            debug!("[nm] Unknown action: {}", action_id);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

impl NetworkManagerDaemon {
    async fn handle_toggle_wifi(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

        Ok(())
    }

    async fn handle_connect_wifi(
        &mut self,
        ssid: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Connecting to WiFi: {}", ssid);

        let connections = get_connections_for_ssid(&self.conn, ssid).await?;
        if let Some(conn_path) = connections.first() {
            let device_path = {
                let state = self.state.lock().unwrap();
                state.wifi_adapters.first().map(|a| a.path.clone())
            };

            if let Some(ref device_path) = device_path {
                match activate_connection(&self.conn, Some(conn_path), device_path, None).await {
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

        Ok(())
    }

    async fn handle_disconnect_wifi(
        &mut self,
        device_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

        Ok(())
    }

    async fn handle_toggle_wired(
        &mut self,
        device_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            info!("[nm] Connecting wired: {}", device_path);
            if let Err(e) = connect_wired_dbus(&self.conn, device_path).await {
                error!("[nm] Failed to connect wired: {}", e);
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn handle_toggle_vpn(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

        Ok(())
    }

    async fn handle_connect_vpn(
        &mut self,
        conn_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("[nm] Connecting VPN: {}", conn_path);

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

        Ok(())
    }

    async fn handle_disconnect_vpn(
        &mut self,
        conn_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

        Ok(())
    }
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
