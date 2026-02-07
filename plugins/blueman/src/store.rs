//! Bluetooth store module.
//!
//! Manages bluetooth state with instance-based stores.

use std::collections::HashMap;

use waft_core::store::{PluginStore, StoreOp, StoreState};
use waft_plugin_api::common::ConnectionState;

// Re-export set_field! macro from waft-core
pub use waft_core::set_field;

/// Type alias for Bluetooth device connection state.
pub type DeviceConnectionState = ConnectionState;

/// State for a single Bluetooth device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceState {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub connection: DeviceConnectionState,
}

/// State for the bluetooth plugin.
#[derive(Clone, Default)]
pub struct BluetoothState {
    pub powered: bool,
    pub busy: bool,
    pub available: bool,
    /// Paired devices keyed by device path.
    pub devices: HashMap<String, DeviceState>,
}

/// Operations for the bluetooth store.
#[derive(Clone)]
pub enum BluetoothOp {
    Powered(bool),
    Busy(bool),
    Available(bool),
    /// Set the full list of paired devices.
    Devices(Vec<DeviceState>),
    /// Update a single device's connection state.
    DeviceConnection(String, DeviceConnectionState),
}

impl StoreOp for BluetoothOp {}

impl StoreState for BluetoothState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

/// Type alias for the bluetooth store.
pub type BluetoothStore = PluginStore<BluetoothOp, BluetoothState>;

/// Create a new bluetooth store instance.
pub fn create_bluetooth_store() -> BluetoothStore {
    PluginStore::new(|state: &mut BluetoothState, op: BluetoothOp| match op {
        BluetoothOp::Powered(powered) => set_field!(state.powered, powered),
        BluetoothOp::Busy(busy) => set_field!(state.busy, busy),
        BluetoothOp::Available(available) => set_field!(state.available, available),
        BluetoothOp::Devices(devices) => {
            let new_devices: HashMap<String, DeviceState> =
                devices.into_iter().map(|d| (d.path.clone(), d)).collect();
            set_field!(state.devices, new_devices)
        }
        BluetoothOp::DeviceConnection(path, connection) => {
            if let Some(device) = state.devices.get_mut(&path) {
                set_field!(device.connection, connection)
            } else {
                false
            }
        }
    })
}
