//! Discovered (unpaired) devices preferences group.
//!
//! Dumb widget displaying Bluetooth devices found during scanning.
//! Visible only when any adapter is actively discovering.

use std::collections::HashMap;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::bluetooth::BluetoothDevice;
use waft_ui_gtk::bluetooth::device_row::device_type_icon;

use super::device_row::{DeviceRow, DeviceRowOutput, DeviceRowProps};

/// Group displaying discovered (unpaired) Bluetooth devices.
pub struct DiscoveredDevicesGroup {
    pub root: adw::PreferencesGroup,
    rows: HashMap<String, DeviceRow>,
}

impl DiscoveredDevicesGroup {
    pub fn new() -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Available Devices")
            .visible(false)
            .build();

        Self {
            root: group,
            rows: HashMap::new(),
        }
    }

    /// Reconcile the discovered device list with new data.
    ///
    /// Adds, updates, or removes device rows to match the provided list.
    /// The `any_discovering` flag controls group visibility.
    pub fn reconcile(
        &mut self,
        devices: &[(Urn, BluetoothDevice)],
        any_discovering: bool,
        action_callback: &EntityActionCallback,
    ) {
        let mut seen = std::collections::HashSet::new();

        for (urn, device) in devices {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let props = DeviceRowProps {
                name: device.name.clone(),
                device_icon: device_type_icon(&device.device_type).to_string(),
                connection_state: device.connection_state,
                paired: false,
                battery_percentage: device.battery_percentage,
                rssi: device.rssi,
            };

            if let Some(existing) = self.rows.get(&urn_str) {
                existing.apply_props(&props);
            } else {
                let row = DeviceRow::new(&props);
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                row.connect_output(move |output| {
                    let (action, params) = match output {
                        DeviceRowOutput::Pair => ("pair-device", serde_json::Value::Null),
                        DeviceRowOutput::ToggleConnect | DeviceRowOutput::Remove => return,
                    };
                    cb(urn_clone.clone(), action.to_string(), params);
                });
                self.root.add(&row.root);
                self.rows.insert(urn_str, row);
            }
        }

        // Remove rows for devices no longer present
        let to_remove: Vec<String> = self
            .rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(row) = self.rows.remove(&key) {
                self.root.remove(&row.root);
            }
        }

        self.root
            .set_visible(any_discovering && !self.rows.is_empty());
    }
}
