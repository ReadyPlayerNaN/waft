//! Per-device row widget.
//!
//! Dumb widget displaying a single Bluetooth device as an `AdwActionRow`
//! with appropriate icon, status text, and action buttons.

use waft_protocol::entity::bluetooth::ConnectionState;
use waft_ui_gtk::bluetooth::resolve_device_type_icon;
use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VCustomButton, VIcon, VLabel};

use crate::i18n::{t, t_args};

/// Props for creating or updating a device row.
#[derive(Clone, PartialEq)]
pub struct DeviceRowProps {
    pub name:               String,
    pub device_type:        String,
    pub connection_state:   ConnectionState,
    pub paired:             bool,
    pub battery_percentage: Option<u8>,
    pub rssi:               Option<i16>,
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

pub(crate) struct DeviceRowRender;

impl RenderFn for DeviceRowRender {
    type Props  = DeviceRowProps;
    type Output = DeviceRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let device_icon = resolve_device_type_icon(&props.device_type);
        let connected   = matches!(props.connection_state, ConnectionState::Connected);

        // Build subtitle and button label based on connection state and paired status
        let (subtitle, action_label, sensitive) = if props.paired {
            match props.connection_state {
                ConnectionState::Connected => {
                    let text = if let Some(pct) = props.battery_percentage {
                        t_args("bt-battery-pct", &[("pct", &pct.to_string())])
                    } else {
                        t("bt-connected")
                    };
                    (text, t("bt-disconnect"), true)
                }
                ConnectionState::Connecting    => (t("bt-connecting"),    t("bt-cancel"), false),
                ConnectionState::Disconnecting => (t("bt-disconnecting"), t("bt-wait"),   false),
                ConnectionState::Disconnected  => (t("bt-disconnected"),  t("bt-connect"), true),
            }
        } else {
            let sub = match props.rssi {
                Some(rssi) if rssi > -50 => t("bt-signal-excellent"),
                Some(rssi) if rssi > -70 => t("bt-signal-good"),
                Some(rssi) if rssi > -85 => t("bt-signal-fair"),
                Some(_)                  => t("bt-signal-weak"),
                None                     => String::new(),
            };
            (sub, t("bt-pair"), true)
        };

        let mut row = VActionRow::new(&props.name)
            .subtitle(&subtitle)
            .prefix(VNode::icon(VIcon::new(
                vec![Icon::Themed(device_icon.to_string())],
                24,
            )));

        // Battery icon (only when connected with battery info)
        if let Some(pct) = props.battery_percentage && connected {
            let batt_icon = resolve_battery_icon_name(pct);
            row = row.suffix(VNode::icon(VIcon::new(
                vec![Icon::Themed(batt_icon.to_string())],
                16,
            )));
        }

        // Action button: Connect/Disconnect for paired, Pair for unpaired
        if props.paired {
            let emit_connect = emit.clone();
            row = row.suffix(VNode::custom_button(
                VCustomButton::new(VNode::label(VLabel::new(&action_label)))
                    .css_class("flat")
                    .sensitive(sensitive)
                    .on_click(move || {
                        if let Some(ref cb) = *emit_connect.borrow() {
                            cb(DeviceRowOutput::ToggleConnect);
                        }
                    }),
            ));
        } else {
            let emit_pair = emit.clone();
            row = row.suffix(VNode::custom_button(
                VCustomButton::new(VNode::label(VLabel::new(&action_label)))
                    .css_classes(["flat", "suggested-action"])
                    .sensitive(sensitive)
                    .on_click(move || {
                        if let Some(ref cb) = *emit_pair.borrow() {
                            cb(DeviceRowOutput::Pair);
                        }
                    }),
            ));
        }

        // Remove button (always shown for paired; hidden for unpaired)
        if props.paired {
            let emit_remove = emit.clone();
            row = row.suffix(VNode::custom_button(
                VCustomButton::new(VNode::icon(VIcon::new(
                    vec![Icon::Themed("user-trash-symbolic".to_string())],
                    16,
                )))
                .css_classes(["flat", "destructive-action"])
                .sensitive(sensitive)
                .on_click(move || {
                    if let Some(ref cb) = *emit_remove.borrow() {
                        cb(DeviceRowOutput::Remove);
                    }
                }),
            ));
        }

        VNode::action_row(row)
    }
}

pub type DeviceRow = RenderComponent<DeviceRowRender>;

fn resolve_battery_icon_name(pct: u8) -> &'static str {
    match pct {
        0..=10 => "battery-level-0-symbolic",
        11..=30 => "battery-caution-symbolic",
        31..=50 => "battery-level-30-symbolic",
        51..=70 => "battery-level-50-symbolic",
        71..=90 => "battery-level-70-symbolic",
        _ => "battery-full-symbolic",
    }
}
