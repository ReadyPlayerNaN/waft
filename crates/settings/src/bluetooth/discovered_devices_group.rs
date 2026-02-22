//! Discovered (unpaired) devices preferences group.
//!
//! Dumb widget displaying Bluetooth devices found during scanning.
//! Always visible; scanning state controls spinner, search button,
//! and description text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityActionCallback;

use crate::i18n::t;
use waft_protocol::Urn;
use waft_protocol::entity::bluetooth::BluetoothDevice;

use waft_ui_gtk::vdom::Component;

use super::device_row::{DeviceRow, DeviceRowOutput, DeviceRowProps};

/// Output events from the discovered devices group.
pub enum DiscoveredDevicesGroupOutput {
    /// Start device discovery scanning on all powered adapters.
    StartDiscovery,
    /// Stop device discovery scanning on all discovering adapters.
    StopDiscovery,
}

/// Callback type for discovered devices group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(DiscoveredDevicesGroupOutput)>>>>;

/// Group displaying discovered (unpaired) Bluetooth devices.
pub struct DiscoveredDevicesGroup {
    pub root: adw::PreferencesGroup,
    spinner: gtk::Spinner,
    search_button: gtk::Button,
    discovering: Rc<RefCell<bool>>,
    rows: HashMap<String, DeviceRow>,
    output_cb: OutputCallback,
}

impl DiscoveredDevicesGroup {
    pub fn new() -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("bt-available-devices"))
            .build();

        let spinner = gtk::Spinner::new();

        let search_button = gtk::Button::builder()
            .icon_name("system-search-symbolic")
            .css_classes(["flat"])
            .tooltip_text(t("bt-adapter-start-scanning"))
            .build();

        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .valign(gtk::Align::Center)
            .build();
        header_box.append(&spinner);
        header_box.append(&search_button);

        group.set_header_suffix(Some(&header_box));

        let discovering = Rc::new(RefCell::new(false));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Wire search button click
        let cb = output_cb.clone();
        let disc = discovering.clone();
        search_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                if *disc.borrow() {
                    callback(DiscoveredDevicesGroupOutput::StopDiscovery);
                } else {
                    callback(DiscoveredDevicesGroupOutput::StartDiscovery);
                }
            }
        });

        Self {
            root: group,
            spinner,
            search_button,
            discovering,
            rows: HashMap::new(),
            output_cb,
        }
    }

    /// Register a callback for discovered devices group output events.
    pub fn connect_output<F: Fn(DiscoveredDevicesGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Reconcile the discovered device list with new data.
    ///
    /// Adds, updates, or removes device rows to match the provided list.
    /// The `any_discovering` flag controls the spinner and description text.
    /// The `any_powered` flag controls search button sensitivity.
    pub fn reconcile(
        &mut self,
        devices: &[(Urn, BluetoothDevice)],
        any_discovering: bool,
        any_powered: bool,
        action_callback: &EntityActionCallback,
    ) {
        *self.discovering.borrow_mut() = any_discovering;

        let mut seen = std::collections::HashSet::new();

        for (urn, device) in devices {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let props = DeviceRowProps {
                name: device.name.clone(),
                device_type: device.device_type.clone(),
                connection_state: device.connection_state,
                paired: false,
                battery_percentage: device.battery_percentage,
                rssi: device.rssi,
            };

            if let Some(existing) = self.rows.get(&urn_str) {
                existing.update(&props);
            } else {
                let row = DeviceRow::build(&props);
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                row.connect_output(move |output| {
                    let (action, params) = match output {
                        DeviceRowOutput::Pair => ("pair-device", serde_json::Value::Null),
                        DeviceRowOutput::ToggleConnect | DeviceRowOutput::Remove => return,
                    };
                    cb(urn_clone.clone(), action.to_string(), params);
                });
                self.root.add(&row.widget());
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
                self.root.remove(&row.widget());
            }
        }

        // Update spinner and description
        if any_discovering {
            self.spinner.start();
            self.search_button.set_icon_name("process-stop-symbolic");
            self.search_button
                .set_tooltip_text(Some(&t("bt-adapter-stop-scanning")));
            if self.rows.is_empty() {
                self.root.set_description(Some(&t("bt-searching-devices")));
            } else {
                self.root.set_description(None::<&str>);
            }
        } else {
            self.spinner.stop();
            self.search_button.set_icon_name("system-search-symbolic");
            self.search_button
                .set_tooltip_text(Some(&t("bt-adapter-start-scanning")));
            if self.rows.is_empty() {
                self.root
                    .set_description(Some(&t("bt-start-scanning-hint")));
            } else {
                self.root.set_description(None::<&str>);
            }
        }

        // Search button is only sensitive when at least one adapter is powered
        self.search_button.set_sensitive(any_powered);
    }
}
