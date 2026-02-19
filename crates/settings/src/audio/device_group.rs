//! Audio device group widget.
//!
//! Dumb widget that renders a titled `adw::PreferencesGroup` containing
//! a list of `AudioDeviceRow` widgets for one device kind (output or input).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::Urn;
use waft_protocol::entity::audio::AudioDevice;

use super::device_row::{AudioDeviceRow, AudioDeviceRowOutput, AudioDeviceRowProps};

/// Output events from a device group.
pub struct AudioDeviceGroupOutput {
    pub urn: Urn,
    pub action: AudioDeviceRowOutput,
}

/// Callback type for group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AudioDeviceGroupOutput)>>>>;

/// A group of audio devices of the same kind.
pub struct AudioDeviceGroup {
    pub root: adw::PreferencesGroup,
    rows: RefCell<HashMap<String, AudioDeviceRow>>,
    output_cb: OutputCallback,
}

impl AudioDeviceGroup {
    pub fn new(title: &str) -> Self {
        let root = adw::PreferencesGroup::builder()
            .title(title)
            .build();

        Self {
            root,
            rows: RefCell::new(HashMap::new()),
            output_cb: Rc::new(RefCell::new(None)),
        }
    }

    /// Reconcile the group with current device data.
    pub fn reconcile(&self, devices: &[(Urn, AudioDevice)]) {
        let mut rows = self.rows.borrow_mut();
        let mut seen = std::collections::HashSet::new();

        for (urn, device) in devices {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let props = AudioDeviceRowProps {
                name: device.name.clone(),
                icon: device.icon.clone(),
                connection_icon: device.connection_icon.clone(),
                volume: device.volume,
                muted: device.muted,
                default: device.default,
            };

            if let Some(existing) = rows.get(&urn_str) {
                existing.apply_props(&props);
            } else {
                let row = AudioDeviceRow::new(&props);
                let urn_clone = urn.clone();
                let cb = self.output_cb.clone();
                row.connect_output(move |action| {
                    if let Some(ref callback) = *cb.borrow() {
                        callback(AudioDeviceGroupOutput {
                            urn: urn_clone.clone(),
                            action,
                        });
                    }
                });
                self.root.add(&row.root);
                rows.insert(urn_str, row);
            }
        }

        // Remove rows no longer present
        let to_remove: Vec<String> = rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(row) = rows.remove(&key) {
                self.root.remove(&row.root);
            }
        }

        // Show group only when it has devices
        self.root.set_visible(!devices.is_empty());
    }

    /// Register a callback for group output events.
    pub fn connect_output<F: Fn(AudioDeviceGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
