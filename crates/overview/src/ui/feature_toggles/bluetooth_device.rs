use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use waft_core::Callback;
use waft_ui_gtk::battery::resolve_battery_icon_name;
use waft_ui_gtk::bluetooth::resolve_device_type_icon;
use waft_ui_gtk::icons::Icon;

use crate::ui::feature_toggles::menu_button::{
    FeatureToggleMenuButton, FeatureToggleMenuButtonProps,
};

/// Properties for initializing a bluetooth device row.
pub struct BluetoothDeviceRowProps {
    pub device_type: String,
    pub name: String,
    pub connected: bool,
    pub power: Option<u8>,
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
    root: FeatureToggleMenuButton,
    switch: gtk::Switch,
    on_output: Callback<BluetoothDeviceRowOutput>,
}

impl BluetoothDeviceRow {
    pub fn new(props: BluetoothDeviceRowProps) -> Self {
        let root = FeatureToggleMenuButton::new(FeatureToggleMenuButtonProps {
            disabled: false,
            name: props.name,
            working: false,
        });

        let device_icon = resolve_device_type_icon(&props.device_type);
        let battery_icon = resolve_battery_icon_name(props.power.unwrap_or(0));

        root.set_primary_icon(vec![Icon::parse(&Arc::from(device_icon))]);
        root.set_secondary_icon(vec![Icon::parse(&Arc::from(battery_icon))]);

        let switch = gtk::Switch::builder()
            .active(props.connected)
            .sensitive(false) // display-only
            .valign(gtk::Align::Center)
            .css_classes(["device-switch"])
            .build();

        root.get_right_box().append(&switch);

        let on_output: Callback<BluetoothDeviceRowOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        root.connect_output(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(BluetoothDeviceRowOutput::ToggleConnect);
            }
        });

        Self {
            root,
            switch,
            on_output,
        }
    }

    pub fn update(&self, props: BluetoothDeviceRowProps) {
        self.set_name(&props.name);
        self.set_device_type(&props.device_type);
        self.set_battery_power(props.power);
        self.set_connected(props.connected);
        self.set_transitioning(props.transitioning);
    }

    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(BluetoothDeviceRowOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_name(&self, name: &str) {
        self.root.set_name(name);
    }

    pub fn set_device_type(&self, device_type: &str) {
        let device_icon = resolve_device_type_icon(&device_type);
        self.root
            .set_primary_icon(vec![Icon::parse(&Arc::from(device_icon))]);
    }

    pub fn set_battery_power(&self, pct: Option<u8>) {
        if let Some(value) = pct {
            let device_icon = resolve_battery_icon_name(value);
            self.root
                .set_secondary_icon(vec![Icon::parse(&Arc::from(device_icon))]);
        } else {
            self.root.set_secondary_icon(vec![]);
        }
    }

    pub fn set_connected(&self, connected: bool) {
        self.switch.set_active(connected);
    }

    pub fn set_transitioning(&self, transitioning: bool) {
        self.root.set_working(transitioning);
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.widget()
    }
}
