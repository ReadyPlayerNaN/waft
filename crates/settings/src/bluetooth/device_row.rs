//! Per-device row widget.
//!
//! Dumb widget displaying a single Bluetooth device as an `AdwActionRow`
//! with appropriate icon, status text, and action buttons.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::bluetooth::ConnectionState;
use waft_ui_gtk::widgets::icon::IconWidget;

/// Props for creating or updating a device row.
pub struct DeviceRowProps {
    pub name: String,
    pub device_icon: String,
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
    icon: IconWidget,
    connect_button: gtk::Button,
    remove_button: gtk::Button,
    pair_button: gtk::Button,
    output_cb: OutputCallback,
}

impl DeviceRow {
    pub fn new(props: &DeviceRowProps) -> Self {
        let icon = IconWidget::from_name(&props.device_icon, 16);

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
            .tooltip_text("Remove device")
            .build();

        // Pair button (for discovered devices)
        let pair_button = gtk::Button::builder()
            .label("Pair")
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
        self.icon.set_icon(&props.device_icon);

        if props.paired {
            // Paired device: show connect/remove buttons, hide pair button
            self.pair_button.set_visible(false);
            self.connect_button.set_visible(true);
            self.remove_button.set_visible(true);

            let (subtitle, connect_label, sensitive) = match props.connection_state {
                ConnectionState::Connected => {
                    let mut text = "Connected".to_string();
                    if let Some(pct) = props.battery_percentage {
                        text.push_str(&format!(" \u{00B7} Battery {pct}%"));
                    }
                    (text, "Disconnect", true)
                }
                ConnectionState::Connecting => ("Connecting\u{2026}".to_string(), "Cancel", false),
                ConnectionState::Disconnecting => {
                    ("Disconnecting\u{2026}".to_string(), "Wait", false)
                }
                ConnectionState::Disconnected => ("Disconnected".to_string(), "Connect", true),
            };

            self.root.set_subtitle(&subtitle);
            self.connect_button.set_label(connect_label);
            self.connect_button.set_sensitive(sensitive);
            self.remove_button.set_sensitive(sensitive);
        } else {
            // Discovered device: show pair button, hide connect/remove
            self.pair_button.set_visible(true);
            self.connect_button.set_visible(false);
            self.remove_button.set_visible(false);

            let subtitle = match props.rssi {
                Some(rssi) if rssi > -50 => "Excellent signal".to_string(),
                Some(rssi) if rssi > -70 => "Good signal".to_string(),
                Some(rssi) if rssi > -85 => "Fair signal".to_string(),
                Some(_) => "Weak signal".to_string(),
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
