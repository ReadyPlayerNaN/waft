//! Bluetooth store module.
//!
//! Manages bluetooth state with instance-based stores.

use std::collections::HashMap;

use crate::store::{PluginStore, StoreOp, StoreState};

/// Connection state for a device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DeviceConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

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
        BluetoothOp::SetPowered(powered) => {
            if state.powered != powered {
                state.powered = powered;
                true
            } else {
                false
            }
        }
        BluetoothOp::SetBusy(busy) => {
            if state.busy != busy {
                state.busy = busy;
                true
            } else {
                false
            }
        }
        BluetoothOp::SetAvailable(available) => {
            if state.available != available {
                state.available = available;
                true
            } else {
                false
            }
        }
        BluetoothOp::SetDevices(devices) => {
            let new_devices: HashMap<String, DeviceState> =
                devices.into_iter().map(|d| (d.path.clone(), d)).collect();
            if state.devices != new_devices {
                state.devices = new_devices;
                true
            } else {
                false
            }
        }
        BluetoothOp::SetDeviceConnection(path, connection) => {
            if let Some(device) = state.devices.get_mut(&path) {
                if device.connection != connection {
                    device.connection = connection;
                    return true;
                }
            }
            false
        }
    })
}
