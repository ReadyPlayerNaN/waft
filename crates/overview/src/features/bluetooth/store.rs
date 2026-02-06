//! Bluetooth store module.
//!
//! Manages bluetooth state with instance-based stores.

use std::collections::HashMap;

use crate::common::ConnectionState;
use crate::set_field;
use crate::store::{PluginStore, StoreOp, StoreState};

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
    SetPowered(bool),
    SetBusy(bool),
    SetAvailable(bool),
    /// Set the full list of paired devices.
    SetDevices(Vec<DeviceState>),
    /// Update a single device's connection state.
    SetDeviceConnection(String, DeviceConnectionState),
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
        BluetoothOp::SetPowered(powered) => set_field!(state.powered, powered),
        BluetoothOp::SetBusy(busy) => set_field!(state.busy, busy),
        BluetoothOp::SetAvailable(available) => set_field!(state.available, available),
        BluetoothOp::SetDevices(devices) => {
            let new_devices: HashMap<String, DeviceState> =
                devices.into_iter().map(|d| (d.path.clone(), d)).collect();
            set_field!(state.devices, new_devices)
        }
        BluetoothOp::SetDeviceConnection(path, connection) => {
            if let Some(device) = state.devices.get_mut(&path) {
                set_field!(device.connection, connection)
            } else {
                false
            }
        }
    })
}
