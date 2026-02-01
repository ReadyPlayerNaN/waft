#![allow(dead_code)] // NetworkManager plugin is under development

use std::collections::HashMap;

use crate::common::ConnectionState;
use crate::set_field;
use crate::store::{PluginStore, StoreOp, StoreState};

/// Type alias for VPN connection state.
pub type VpnState = ConnectionState;

#[derive(Debug, Clone)]
pub struct AccessPointState {
    pub path: String,
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
    pub connecting: bool,
}

#[derive(Debug, Clone)]
pub struct WiFiAdapterState {
    pub path: String,
    pub interface_name: String,
    pub enabled: bool,
    pub busy: bool,
    pub active_connection: Option<String>,
    pub access_points: HashMap<String, AccessPointState>,
    pub scanning: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectionProfile {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct EthernetAdapterState {
    pub path: String,
    pub interface_name: String,
    pub enabled: bool,
    pub carrier: bool,
    pub device_state: u32,
    pub active_connection: Option<String>,
    pub available_connections: Vec<ConnectionProfile>,
}

#[derive(Debug, Clone)]
pub struct VpnConnectionState {
    pub path: String,
    pub name: String,
    pub state: VpnState,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkState {
    pub available: bool,
    pub wifi_adapters: HashMap<String, WiFiAdapterState>,
    pub ethernet_adapters: HashMap<String, EthernetAdapterState>,
    pub vpn_connections: HashMap<String, VpnConnectionState>,
    pub any_vpn_active: bool,
}

#[derive(Debug, Clone)]
pub enum NetworkOp {
    SetAvailable(bool),
    AddWiFiAdapter(WiFiAdapterState),
    RemoveWiFiAdapter(String),
    SetWiFiEnabled(String, bool),
    SetWiFiAccessPoints(String, Vec<AccessPointState>),
    SetActiveWiFiConnection(String, Option<String>),
    SetWiFiBusy(String, bool),
    SetWiFiScanning(String, bool),
    AddEthernetAdapter(EthernetAdapterState),
    RemoveEthernetAdapter(String),
    SetEthernetDeviceState(String, u32),
    SetVpnConnections(Vec<VpnConnectionState>),
    SetVpnState(String, VpnState),
}

impl StoreOp for NetworkOp {}

impl StoreState for NetworkState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

impl NetworkState {
    fn update_vpn_active_state(&mut self) {
        self.any_vpn_active = self
            .vpn_connections
            .values()
            .any(|conn| matches!(conn.state, VpnState::Connected));
    }
}

pub type NetworkStore = PluginStore<NetworkOp, NetworkState>;

pub fn create_network_store() -> NetworkStore {
    PluginStore::new(|state: &mut NetworkState, op: NetworkOp| {
        match op {
            NetworkOp::SetAvailable(available) => set_field!(state.available, available),
            NetworkOp::AddWiFiAdapter(adapter) => {
                state.wifi_adapters.insert(adapter.path.clone(), adapter);
                true
            }
            NetworkOp::RemoveWiFiAdapter(path) => state.wifi_adapters.remove(&path).is_some(),
            NetworkOp::SetWiFiEnabled(path, enabled) => {
                if let Some(adapter) = state.wifi_adapters.get_mut(&path) {
                    set_field!(adapter.enabled, enabled)
                } else {
                    false
                }
            }
            NetworkOp::SetWiFiAccessPoints(path, access_points) => {
                if let Some(adapter) = state.wifi_adapters.get_mut(&path) {
                    adapter.access_points = access_points
                        .into_iter()
                        .map(|ap| (ap.path.clone(), ap))
                        .collect();
                    true
                } else {
                    false
                }
            }
            NetworkOp::SetActiveWiFiConnection(path, connection) => {
                if let Some(adapter) = state.wifi_adapters.get_mut(&path) {
                    set_field!(adapter.active_connection, connection)
                } else {
                    false
                }
            }
            NetworkOp::SetWiFiBusy(path, busy) => {
                if let Some(adapter) = state.wifi_adapters.get_mut(&path) {
                    set_field!(adapter.busy, busy)
                } else {
                    false
                }
            }
            NetworkOp::SetWiFiScanning(path, scanning) => {
                if let Some(adapter) = state.wifi_adapters.get_mut(&path) {
                    set_field!(adapter.scanning, scanning)
                } else {
                    false
                }
            }
            NetworkOp::AddEthernetAdapter(adapter) => {
                state
                    .ethernet_adapters
                    .insert(adapter.path.clone(), adapter);
                true
            }
            NetworkOp::RemoveEthernetAdapter(path) => {
                state.ethernet_adapters.remove(&path).is_some()
            }
            NetworkOp::SetEthernetDeviceState(path, device_state) => {
                if let Some(adapter) = state.ethernet_adapters.get_mut(&path) {
                    if adapter.device_state != device_state {
                        adapter.device_state = device_state;
                        // Derive enabled and carrier from device state
                        // - Unavailable (20) = no carrier (cable not connected)
                        // - Disconnected (30) or higher = carrier present
                        // - Activated (100) = connected
                        adapter.carrier = device_state >= 30;
                        adapter.enabled = device_state >= 20;
                        adapter.active_connection = if device_state == 100 {
                            Some(adapter.path.clone()) // Placeholder
                        } else {
                            None
                        };
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            NetworkOp::SetVpnConnections(connections) => {
                state.vpn_connections = connections
                    .into_iter()
                    .map(|conn| (conn.path.clone(), conn))
                    .collect();
                state.update_vpn_active_state();
                true
            }
            NetworkOp::SetVpnState(path, vpn_state) => {
                if let Some(conn) = state.vpn_connections.get_mut(&path) {
                    if set_field!(conn.state, vpn_state) {
                        state.update_vpn_active_state();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    })
}
