//! Audio device menu widget.
//!
//! Displays a list of audio devices with the ability to select a default.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use super::store::AudioDevice;
use crate::common::Callback;
use crate::ui::icon::{Icon, IconWidget};

/// Output events from the device menu.
#[derive(Debug, Clone)]
pub enum AudioDeviceMenuOutput {
    SelectDevice(String), // device id
}

/// Display information for a device in the menu.
#[derive(Clone, Debug)]
pub struct AudioDeviceDisplay {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub secondary_icon: Option<String>,
    pub is_default: bool,
    pub input: bool,
}

impl From<(&AudioDevice, bool, bool)> for AudioDeviceDisplay {
    fn from((device, is_default, input): (&AudioDevice, bool, bool)) -> Self {
        Self {
            id: device.id.clone(),
            name: device.name.clone(),
            icon: device.icon.clone(),
            secondary_icon: device.secondary_icon.clone(),
            is_default,
            input,
        }
    }
}

/// A single device row in the menu.
struct DeviceRow {
    root: gtk::Button,
    check_icon: gtk::Image,
    is_default: Rc<RefCell<bool>>,
}

impl DeviceRow {
    fn new(device: &AudioDeviceDisplay, on_output: Callback<AudioDeviceMenuOutput>) -> Self {
        let root = gtk::Button::builder()
            .css_classes(["audio-device-row"])
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();

        // Device icon with fallback
        let fallback_icon = if device.input {
            "audio-input-microphone-symbolic"
        } else {
            "audio-speakers-symbolic"
        };

        let icon_hints = vec![
            Icon::Themed(Arc::from(device.icon.as_str())),
            Icon::Themed(Arc::from(fallback_icon)),
        ];
        let icon_widget = IconWidget::new(icon_hints, 16);
        icon_widget.widget().add_css_class("audio-device-icon");

        // Device name
        let name_label = gtk::Label::builder()
            .label(&device.name)
            .hexpand(true)
            .xalign(0.0)
            .css_classes(["audio-device-name"])
            .build();

        // Checkmark (visible when default)
        let check_icon = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .pixel_size(16)
            .css_classes(["audio-device-check"])
            .build();

        content.append(icon_widget.widget());

        // Secondary icon (e.g., HDMI/Bluetooth indicator)
        if let Some(ref secondary) = device.secondary_icon {
            let secondary_icon_widget = IconWidget::from_name(secondary, 16);
            secondary_icon_widget.widget().add_css_class("audio-device-secondary-icon");
            content.append(secondary_icon_widget.widget());
        }

        content.append(&name_label);
        content.append(&check_icon);

        root.set_child(Some(&content));

        let is_default = Rc::new(RefCell::new(device.is_default));

        // Apply initial state
        Self::update_default_state(&root, &check_icon, device.is_default);

        // Connect click handler
        let device_id = device.id.clone();
        root.connect_clicked(move |_| {
            if let Some(ref callback) = *on_output.borrow() {
                callback(AudioDeviceMenuOutput::SelectDevice(device_id.clone()));
            }
        });

        Self {
            root,
            check_icon,
            is_default,
        }
    }

    fn set_default(&self, is_default: bool) {
        *self.is_default.borrow_mut() = is_default;
        Self::update_default_state(&self.root, &self.check_icon, is_default);
    }

    fn update_default_state(root: &gtk::Button, check_icon: &gtk::Image, is_default: bool) {
        if is_default {
            root.add_css_class("default");
            check_icon.set_visible(true);
        } else {
            root.remove_css_class("default");
            check_icon.set_visible(false);
        }
    }
}

/// Widget displaying a list of audio devices.
pub struct AudioDeviceMenuWidget {
    pub root: gtk::Box,
    rows: Rc<RefCell<HashMap<String, DeviceRow>>>,
    on_output: Callback<AudioDeviceMenuOutput>,
}

impl AudioDeviceMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["audio-device-menu"])
            .build();

        Self {
            root,
            rows: Rc::new(RefCell::new(HashMap::new())),
            on_output: Rc::new(RefCell::new(None)),
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(AudioDeviceMenuOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the list of devices.
    pub fn set_devices(&self, devices: Vec<AudioDeviceDisplay>) {
        let mut rows = self.rows.borrow_mut();

        // Remove rows for devices that no longer exist
        let current_ids: std::collections::HashSet<String> =
            devices.iter().map(|d| d.id.clone()).collect();

        let removed: Vec<String> = rows
            .keys()
            .filter(|id| !current_ids.contains(*id))
            .cloned()
            .collect();

        for id in removed {
            if let Some(row) = rows.remove(&id) {
                self.root.remove(&row.root);
            }
        }

        // Add or update rows
        for device in devices {
            if let Some(row) = rows.get(&device.id) {
                // Update existing row's default state
                row.set_default(device.is_default);
            } else {
                // Create new row
                let row = DeviceRow::new(&device, self.on_output.clone());
                self.root.append(&row.root);
                rows.insert(device.id.clone(), row);
            }
        }
    }
}

impl Default for AudioDeviceMenuWidget {
    fn default() -> Self {
        Self::new()
    }
}
