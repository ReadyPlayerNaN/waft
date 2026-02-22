use std::sync::Arc;

use waft_ui_gtk::battery::resolve_battery_icon_name;
use waft_ui_gtk::bluetooth::resolve_device_type_icon;
use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

use super::menu_button::{
    FeatureToggleMenuButton, FeatureToggleMenuButtonProps,
};

/// Properties for a single Bluetooth device row.
#[derive(Clone, PartialEq)]
pub struct BluetoothDeviceRowProps {
    pub device_type:   String,
    pub name:          String,
    pub connected:     bool,
    pub power:         Option<u8>,
    pub transitioning: bool,
}

/// Output events from the Bluetooth device row.
pub enum BluetoothDeviceRowOutput {
    ToggleConnect,
}

pub(crate) struct BluetoothDeviceRowRender;

impl RenderFn for BluetoothDeviceRowRender {
    type Props  = BluetoothDeviceRowProps;
    type Output = BluetoothDeviceRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit_clone = emit.clone();

        let device_icon = resolve_device_type_icon(&props.device_type);
        let secondary_icon = props
            .power
            .map(|pwr| vec![Icon::parse(&Arc::from(resolve_battery_icon_name(pwr)))])
            .unwrap_or_default();

        VNode::with_output::<FeatureToggleMenuButton>(
            FeatureToggleMenuButtonProps {
                disabled:       false,
                name:           props.name.clone(),
                working:        props.transitioning,
                primary_icon:   vec![Icon::parse(&Arc::from(device_icon))],
                secondary_icon,
                visible:        true,
                switch_active:  Some(props.connected || props.transitioning),
            },
            move |_click| {
                if let Some(ref cb) = *emit_clone.borrow() {
                    cb(BluetoothDeviceRowOutput::ToggleConnect);
                }
            },
        )
    }
}

pub type BluetoothDeviceRow = RenderComponent<BluetoothDeviceRowRender>;
