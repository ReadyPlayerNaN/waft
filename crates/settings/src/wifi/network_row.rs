//! Per-network row widget.
//!
//! Dumb widget displaying a single WiFi network as an `AdwActionRow`
//! with signal strength icon, security indicator, and connect button.

use std::sync::Arc;

use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VCustomButton, VIcon, VLabel};

use crate::i18n::t;

/// Props for creating or updating a network row.
#[derive(Clone, PartialEq)]
pub struct NetworkRowProps {
    pub ssid:      String,
    pub strength:  u8,
    pub secure:    bool,
    pub connected: bool,
}

/// Output events from a network row.
pub enum NetworkRowOutput {
    Connect,
    Disconnect,
}

fn signal_icon_name(strength: u8) -> &'static str {
    if strength > 75 {
        "network-wireless-signal-excellent-symbolic"
    } else if strength > 50 {
        "network-wireless-signal-good-symbolic"
    } else if strength > 25 {
        "network-wireless-signal-ok-symbolic"
    } else {
        "network-wireless-signal-weak-symbolic"
    }
}

pub(crate) struct NetworkRowRender;

impl RenderFn for NetworkRowRender {
    type Props  = NetworkRowProps;
    type Output = NetworkRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit_clone  = emit.clone();
        let signal_icon = signal_icon_name(props.strength);
        let subtitle    = if props.connected { t("wifi-connected") } else { String::new() };
        let btn_label   = if props.connected { t("wifi-disconnect") } else { t("wifi-connect") };

        let mut row = VActionRow::new(&props.ssid)
            .subtitle(&subtitle)
            .prefix(VNode::icon(VIcon::new(
                vec![Icon::Themed(Arc::from(signal_icon))],
                16,
            )));

        if props.secure {
            row = row.prefix(VNode::icon(VIcon::new(
                vec![Icon::Themed(Arc::from("network-wireless-encrypted-symbolic"))],
                16,
            )));
        }

        let connected = props.connected;
        row = row.suffix(VNode::custom_button(
            VCustomButton::new(VNode::label(VLabel::new(&btn_label)))
                .css_class("flat")
                .on_click(move || {
                    if let Some(ref cb) = *emit_clone.borrow() {
                        let ev = if connected {
                            NetworkRowOutput::Disconnect
                        } else {
                            NetworkRowOutput::Connect
                        };
                        cb(ev);
                    }
                }),
        ));

        VNode::action_row(row)
    }
}

pub type NetworkRow = RenderComponent<NetworkRowRender>;
