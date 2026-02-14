//! Bluetooth adapter and device state types.

use waft_protocol::entity::bluetooth::ConnectionState;

#[derive(Debug, Clone)]
pub struct DeviceState {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub connection_state: ConnectionState,
    pub battery_percentage: Option<u8>,
    pub paired: bool,
    pub trusted: bool,
    pub rssi: Option<i16>,
}

#[derive(Debug, Clone)]
pub struct AdapterState {
    pub path: String,
    pub name: String,
    pub powered: bool,
    pub discoverable: bool,
    pub discovering: bool,
    pub devices: Vec<DeviceState>,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub adapters: Vec<AdapterState>,
}
