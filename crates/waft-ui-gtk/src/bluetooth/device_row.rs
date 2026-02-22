//! Bluetooth device row widget for menu device lists.
//!
//! A horizontal button row showing device icon, optional battery icon,
//! device name, and a spinner/switch indicator for connection state.

use crate::bluetooth::device_icon::resolve_device_type_icon;
use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VCustomButton, VIcon, VLabel, VSpinner, VSwitch};
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

/// Resolve device_type string to a themed icon name.
pub fn device_type_icon(device_type: &str) -> &'static str {
    resolve_device_type_icon(device_type)
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
#[derive(Clone, PartialEq)]
pub struct BluetoothDeviceRowProps {
    pub device_type: String,
    pub name: String,
    pub battery_icon: Option<String>,
    pub connected: bool,
    pub transitioning: bool,
}

/// Output events from the bluetooth device row.
pub enum BluetoothDeviceRowOutput {
    ToggleConnect,
}

pub struct BluetoothDeviceRowRender;

impl RenderFn for BluetoothDeviceRowRender {
    type Props = BluetoothDeviceRowProps;
    type Output = BluetoothDeviceRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit = emit.clone();
        let device_icon = resolve_device_type_icon(&props.device_type);

        let icon_box = VBox::horizontal(4)
            .valign(gtk::Align::Center)
            .child(VNode::icon(VIcon::new(
                vec![Icon::Themed(device_icon.to_string())],
                16,
            )))
            .child(VNode::icon(
                VIcon::new(
                    props
                        .battery_icon
                        .as_ref()
                        .map(|name| vec![Icon::Themed(name.clone())])
                        .unwrap_or_default(),
                    16,
                )
                .visible(props.battery_icon.is_some()),
            ));

        let right_box = VBox::horizontal(4)
            .valign(gtk::Align::Center)
            .child(VNode::spinner(
                VSpinner::new(props.transitioning).visible(props.transitioning),
            ))
            .child(VNode::switch(
                VSwitch::new(props.connected)
                    .sensitive(false)
                    .css_class("device-switch"),
            ));

        let inner = VBox::horizontal(8)
            .child(VNode::vbox(icon_box))
            .child(VNode::label(
                VLabel::new(&props.name)
                    .hexpand(true)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End),
            ))
            .child(VNode::vbox(right_box));

        VNode::custom_button(
            VCustomButton::new(VNode::vbox(inner))
                .css_classes(["flat", "device-row"])
                .sensitive(!props.transitioning)
                .on_click(move || {
                    if let Some(ref cb) = *emit.borrow() {
                        cb(BluetoothDeviceRowOutput::ToggleConnect);
                    }
                }),
        )
    }
}

pub type BluetoothDeviceRow = RenderComponent<BluetoothDeviceRowRender>;
