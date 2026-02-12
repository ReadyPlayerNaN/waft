//! Bluetooth adapter and device state types.

#[derive(Debug, Clone)]
pub struct DeviceState {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub connected: bool,
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
