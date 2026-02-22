//! Connection row widget for toggleable connection lists.
//!
//! A horizontal button row showing a connection name and a spinner/switch
//! indicator for connection state. Reusable for VPN, ethernet profiles, etc.

use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VCustomButton, VIcon, VLabel, VSpinner, VSwitch};
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

/// Properties for initializing a connection row.
#[derive(Clone, PartialEq)]
pub struct ConnectionRowProps {
    pub name: String,
    pub active: bool,
    pub transitioning: bool,
    /// Optional leading icon name (e.g. "network-vpn-symbolic").
    pub icon: Option<String>,
}

/// Output events from the connection row.
pub enum ConnectionRowOutput {
    Toggle,
}

pub struct ConnectionRowRender;

impl RenderFn for ConnectionRowRender {
    type Props = ConnectionRowProps;
    type Output = ConnectionRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit = emit.clone();

        let icon_box = VBox::horizontal(4)
            .valign(gtk::Align::Center)
            .child(VNode::icon(
                VIcon::new(
                    props
                        .icon
                        .as_ref()
                        .map(|name| vec![Icon::Themed(name.clone())])
                        .unwrap_or_default(),
                    16,
                )
                .visible(props.icon.is_some()),
            ));

        let right_box = VBox::horizontal(4)
            .valign(gtk::Align::Center)
            .child(VNode::spinner(
                VSpinner::new(props.transitioning).visible(props.transitioning),
            ))
            .child(VNode::switch(
                VSwitch::new(props.active)
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
                        cb(ConnectionRowOutput::Toggle);
                    }
                }),
        )
    }
}

pub type ConnectionRow = RenderComponent<ConnectionRowRender>;
