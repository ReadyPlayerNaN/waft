//! Per-connection row widget.
//!
//! Dumb widget displaying a single Ethernet connection profile
//! as an `AdwActionRow` with active indicator and action button.

use std::sync::Arc;

use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VCustomButton, VIcon, VLabel};

use crate::i18n::t;

/// Props for creating or updating a connection row.
#[derive(Clone, PartialEq)]
pub struct ConnectionRowProps {
    pub name:   String,
    pub active: bool,
}

/// Output events from a connection row.
pub enum ConnectionRowOutput {
    /// Activate this connection profile.
    Activate,
    /// Deactivate this connection profile.
    Deactivate,
}

pub(crate) struct ConnectionRowRender;

impl RenderFn for ConnectionRowRender {
    type Props  = ConnectionRowProps;
    type Output = ConnectionRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let subtitle  = if props.active { t("wired-active") } else { String::new() };
        let btn_label = if props.active { t("wired-disconnect") } else { t("wired-connect") };
        let active    = props.active;

        VNode::action_row(
            VActionRow::new(&props.name)
                .subtitle(&subtitle)
                .prefix(VNode::icon(
                    VIcon::new(
                        vec![Icon::Themed(Arc::from("emblem-default-symbolic"))],
                        16,
                    )
                    .visible(props.active),
                ))
                .suffix(VNode::custom_button(
                    VCustomButton::new(VNode::label(VLabel::new(&btn_label)))
                        .css_class("flat")
                        .on_click({
                            let emit = emit.clone();
                            move || {
                                if let Some(ref cb) = *emit.borrow() {
                                    let ev = if active {
                                        ConnectionRowOutput::Deactivate
                                    } else {
                                        ConnectionRowOutput::Activate
                                    };
                                    cb(ev);
                                }
                            }
                        }),
                )),
        )
    }
}

pub type WiredConnectionRow = RenderComponent<ConnectionRowRender>;
