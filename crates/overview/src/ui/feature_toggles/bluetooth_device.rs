use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use waft_core::Callback;
use waft_ui_gtk::battery::resolve_battery_icon_name;
use waft_ui_gtk::bluetooth::resolve_device_type_icon;
use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::Component;

use crate::ui::feature_toggles::menu_button::{
    FeatureToggleMenuButton, FeatureToggleMenuButtonProps,
};

/// Properties for initializing a bluetooth device row.
#[derive(Clone, PartialEq)]
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

impl Component for BluetoothDeviceRow {
    type Props = BluetoothDeviceRowProps;
    type Output = BluetoothDeviceRowOutput;

    fn build(props: &Self::Props) -> Self {
        let root = FeatureToggleMenuButton::new(FeatureToggleMenuButtonProps {
            disabled: false,
            name: props.name.clone(),
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

    fn update(&self, props: &Self::Props) {
        self.root.set_name(&props.name);

        let device_icon = resolve_device_type_icon(&props.device_type);
        self.root
            .set_primary_icon(vec![Icon::parse(&Arc::from(device_icon))]);

        if let Some(value) = props.power {
            let battery_icon = resolve_battery_icon_name(value);
            self.root
                .set_secondary_icon(vec![Icon::parse(&Arc::from(battery_icon))]);
        } else {
            self.root.set_secondary_icon(vec![]);
        }

        self.switch.set_active(props.connected);
        self.root.set_working(props.transitioning);
    }

    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F) {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    fn widget(&self) -> gtk::Widget {
        self.root.widget()
    }
}
