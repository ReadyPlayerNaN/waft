//! Per-device row widget.
//!
//! Dumb widget displaying a single Bluetooth device as an `AdwActionRow`
//! with appropriate icon, status text, and action buttons.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::bluetooth::ConnectionState;
use waft_ui_gtk::bluetooth::device_icon::BluetoothDeviceIcon;

use crate::i18n::{t, t_args};

/// Props for creating or updating a device row.
pub struct DeviceRowProps {
    pub name: String,
    pub device_type: String,
    pub connection_state: ConnectionState,
    pub paired: bool,
    pub battery_percentage: Option<u8>,
    pub rssi: Option<i16>,
}

/// Output events from a device row.
pub enum DeviceRowOutput {
    /// Toggle connect/disconnect for a paired device.
    ToggleConnect,
    /// Request pairing with a discovered device.
    Pair,
    /// Remove a paired device.
    Remove,
}

/// Callback type for device row output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(DeviceRowOutput)>>>>;

/// A single Bluetooth device row.
pub struct DeviceRow {
    pub root: adw::ActionRow,
    icon: BluetoothDeviceIcon,
    connect_button: gtk::Button,
    remove_button: gtk::Button,
    pair_button: gtk::Button,
    output_cb: OutputCallback,
}

impl DeviceRow {
    pub fn new(props: &DeviceRowProps) -> Self {
        let icon = BluetoothDeviceIcon::new(&props.device_type, Some(32));
        let row = adw::ActionRow::builder().title(&props.name).build();

        row.add_prefix(icon.widget());

        // Connect/Disconnect button (for paired devices)
        let connect_button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();

        // Remove button (for paired devices)
        let remove_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat", "destructive-action"])
            .tooltip_text(t("bt-remove-device"))
            .build();

        // Pair button (for discovered devices)
        let pair_button = gtk::Button::builder()
            .label(t("bt-pair"))
            .valign(gtk::Align::Center)
            .css_classes(["flat", "suggested-action"])
            .build();

        row.add_suffix(&connect_button);
        row.add_suffix(&remove_button);
        row.add_suffix(&pair_button);

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Wire button signals
        let cb = output_cb.clone();
        connect_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                callback(DeviceRowOutput::ToggleConnect);
            }
        });

        let cb = output_cb.clone();
        remove_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                callback(DeviceRowOutput::Remove);
            }
        });

        let cb = output_cb.clone();
        pair_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                callback(DeviceRowOutput::Pair);
            }
        });

        let device_row = Self {
            root: row,
            icon,
            connect_button,
            remove_button,
            pair_button,
            output_cb,
        };

        device_row.apply_props(props);
        device_row
    }

    /// Update the row to reflect new device state.
    pub fn apply_props(&self, props: &DeviceRowProps) {
        self.root.set_title(&props.name);
        self.icon.set_device_type(&props.device_type);

        if props.paired {
            // Paired device: show connect/remove buttons, hide pair button
            self.pair_button.set_visible(false);
            self.connect_button.set_visible(true);
            self.remove_button.set_visible(true);

            let (subtitle, connect_label, sensitive) = match props.connection_state {
                ConnectionState::Connected => {
                    let text = if let Some(pct) = props.battery_percentage {
                        t_args("bt-battery-pct", &[("pct", &pct.to_string())])
                    } else {
                        t("bt-connected")
                    };
                    (text, t("bt-disconnect"), true)
                }
                ConnectionState::Connecting => (t("bt-connecting"), t("bt-cancel"), false),
                ConnectionState::Disconnecting => (t("bt-disconnecting"), t("bt-wait"), false),
                ConnectionState::Disconnected => (t("bt-disconnected"), t("bt-connect"), true),
            };

            self.root.set_subtitle(&subtitle);
            self.connect_button.set_label(&connect_label);
            self.connect_button.set_sensitive(sensitive);
            self.remove_button.set_sensitive(sensitive);
        } else {
            // Discovered device: show pair button, hide connect/remove
            self.pair_button.set_visible(true);
            self.connect_button.set_visible(false);
            self.remove_button.set_visible(false);

            let subtitle = match props.rssi {
                Some(rssi) if rssi > -50 => t("bt-signal-excellent"),
                Some(rssi) if rssi > -70 => t("bt-signal-good"),
                Some(rssi) if rssi > -85 => t("bt-signal-fair"),
                Some(_) => t("bt-signal-weak"),
                None => String::new(),
            };
            self.root.set_subtitle(&subtitle);
        }
    }

    /// Register a callback for device row output events.
    pub fn connect_output<F: Fn(DeviceRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
