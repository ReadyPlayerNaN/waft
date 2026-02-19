//! Paired devices preferences group.
//!
//! Dumb widget displaying a list of paired Bluetooth devices.
//! Visible only when there are paired devices.

use std::collections::HashMap;

use adw::prelude::*;
use waft_client::EntityActionCallback;

use crate::i18n::t;
use waft_protocol::Urn;
use waft_protocol::entity::bluetooth::BluetoothDevice;
use waft_ui_gtk::bluetooth::device_row::device_type_icon;

use super::device_row::{DeviceRow, DeviceRowOutput, DeviceRowProps};

/// Group displaying paired Bluetooth devices.
pub struct PairedDevicesGroup {
    pub root: adw::PreferencesGroup,
    rows: HashMap<String, DeviceRow>,
}

impl PairedDevicesGroup {
    pub fn new() -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("bt-paired-devices"))
            .visible(true)
            .description(t("bt-no-paired-devices"))
            .build();

        Self {
            root: group,
            rows: HashMap::new(),
        }
    }

    /// Reconcile the paired device list with new data.
    ///
    /// Adds, updates, or removes device rows to match the provided list.
    pub fn reconcile(
        &mut self,
        devices: &[(Urn, BluetoothDevice)],
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
                paired: true,
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
                        DeviceRowOutput::ToggleConnect => {
                            ("toggle-connect", serde_json::Value::Null)
                        }
                        DeviceRowOutput::Remove => ("remove-device", serde_json::Value::Null),
                        DeviceRowOutput::Pair => return, // Not applicable for paired devices
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

        if self.rows.is_empty() {
            self.root.set_description(Some(&t("bt-no-paired-devices")));
        } else {
            self.root.set_description(None::<&str>);
        }
    }
}
