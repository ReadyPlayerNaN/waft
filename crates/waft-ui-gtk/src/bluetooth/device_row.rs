//! Bluetooth device row widget for menu device lists.
//!
//! A horizontal button row showing device icon, optional battery icon,
//! device name, and a spinner/switch indicator for connection state.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;

use crate::widgets::icon::IconWidget;

/// Resolve device_type string to a themed icon name.
pub fn device_type_icon(device_type: &str) -> &'static str {
    match device_type {
        "audio-headphones" => "audio-headphones-symbolic",
        "audio-headset" => "audio-headset-symbolic",
        "input-mouse" => "input-mouse-symbolic",
        "input-keyboard" => "input-keyboard-symbolic",
        "phone" => "phone-symbolic",
        "computer" => "computer-symbolic",
        _ => "bluetooth-symbolic",
    }
}

/// Pick a battery icon name based on percentage.
pub fn battery_icon_name(pct: u8) -> &'static str {
    match pct {
        0..=10 => "battery-level-0-symbolic",
        11..=30 => "battery-caution-symbolic",
        31..=50 => "battery-level-30-symbolic",
        51..=70 => "battery-level-50-symbolic",
        71..=90 => "battery-level-70-symbolic",
        _ => "battery-full-symbolic",
    }
}

/// Properties for initializing a bluetooth device row.
pub struct BluetoothDeviceRowProps {
    pub device_icon: String,
    pub name: String,
    pub battery_icon: Option<String>,
    pub connected: bool,
    pub transitioning: bool,
}

/// Output events from the bluetooth device row.
pub enum BluetoothDeviceRowOutput {
    ToggleConnect,
}

/// A horizontal button row for a single Bluetooth device.
///
/// Layout: `Button > Box(H) > [icon_box(device_icon + battery_icon), name_label(hexpand), right_box(spinner + switch)]`
pub struct BluetoothDeviceRow {
    pub root: gtk::Button,
    name_label: gtk::Label,
    device_icon: IconWidget,
    battery_icon: IconWidget,
    battery_icon_widget: gtk::Widget,
    spinner: gtk::Spinner,
    switch: gtk::Switch,
    on_output: Callback<BluetoothDeviceRowOutput>,
}

impl BluetoothDeviceRow {
    pub fn new(props: BluetoothDeviceRowProps) -> Self {
        let inner = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        // Left box: device type icon + battery icon
        let icon_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let device_icon = IconWidget::from_name(&props.device_icon, 16);
        icon_box.append(device_icon.widget());

        let battery_icon = IconWidget::from_name(
            props.battery_icon.as_deref().unwrap_or("battery-full-symbolic"),
            16,
        );
        let battery_icon_widget = battery_icon.widget().clone().upcast::<gtk::Widget>();
        battery_icon_widget.set_visible(props.battery_icon.is_some());
        icon_box.append(&battery_icon_widget);

        inner.append(&icon_box);

        // Center: device name (expands to fill)
        let name_label = gtk::Label::builder()
            .label(&props.name)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        inner.append(&name_label);

        // Right box: spinner (hidden by default) + connection switch
        let right_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let spinner = gtk::Spinner::builder()
            .visible(props.transitioning)
            .spinning(props.transitioning)
            .build();
        right_box.append(&spinner);

        let switch = gtk::Switch::builder()
            .active(props.connected)
            .sensitive(false) // display-only
            .valign(gtk::Align::Center)
            .css_classes(["device-switch"])
            .build();
        right_box.append(&switch);

        inner.append(&right_box);

        let button = gtk::Button::builder()
            .child(&inner)
            .css_classes(["flat", "device-row"])
            .sensitive(!props.transitioning)
            .build();

        let on_output: Callback<BluetoothDeviceRowOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(BluetoothDeviceRowOutput::ToggleConnect);
            }
        });

        Self {
            root: button,
            name_label,
            device_icon,
            battery_icon,
            battery_icon_widget,
            spinner,
            switch,
            on_output,
        }
    }

    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(BluetoothDeviceRowOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_name(&self, name: &str) {
        self.name_label.set_label(name);
    }

    pub fn set_device_icon(&self, icon_name: &str) {
        self.device_icon.set_icon(icon_name);
    }

    pub fn set_battery_icon(&self, icon_name: Option<&str>) {
        if let Some(name) = icon_name {
            self.battery_icon.set_icon(name);
            self.battery_icon_widget.set_visible(true);
        } else {
            self.battery_icon_widget.set_visible(false);
        }
    }

    pub fn set_connected(&self, connected: bool) {
        self.switch.set_active(connected);
    }

    pub fn set_transitioning(&self, transitioning: bool) {
        self.spinner.set_visible(transitioning);
        self.spinner.set_spinning(transitioning);
        self.root.set_sensitive(!transitioning);
    }
}
