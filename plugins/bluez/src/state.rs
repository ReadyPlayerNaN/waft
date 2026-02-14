//! Bluetooth adapter and device state types.

use waft_protocol::entity::bluetooth::ConnectionState;

#[derive(Debug, Clone)]
pub struct DeviceState {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub connection_state: ConnectionState,
    pub battery_percentage: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct AdapterState {
    pub path: String,
    pub name: String,
    pub powered: bool,
    pub devices: Vec<DeviceState>,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub adapters: Vec<AdapterState>,
}
