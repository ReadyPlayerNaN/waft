//! Backup method row widget for menu method lists.
//!
//! A horizontal button row showing a method icon, method name, and
//! a switch indicator for enabled state.

use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VCustomButton, VIcon, VLabel, VSwitch};
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

/// Properties for initializing a backup method row.
#[derive(Clone, PartialEq)]
pub struct BackupMethodRowProps {
    pub icon: String,
    pub name: String,
    pub enabled: bool,
}

/// Output events from the backup method row.
pub enum BackupMethodRowOutput {
    ToggleMethod,
}

pub struct BackupMethodRowRender;

impl RenderFn for BackupMethodRowRender {
    type Props = BackupMethodRowProps;
    type Output = BackupMethodRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit = emit.clone();

        let inner = VBox::horizontal(8)
            .child(VNode::icon(VIcon::new(
                vec![Icon::Themed(props.icon.clone())],
                16,
            )))
            .child(VNode::label(
                VLabel::new(&props.name)
                    .hexpand(true)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End),
            ))
            .child(VNode::switch(
                VSwitch::new(props.enabled)
                    .sensitive(false)
                    .css_class("device-switch"),
            ));

        VNode::custom_button(
            VCustomButton::new(VNode::vbox(inner))
                .css_classes(["flat", "device-row"])
                .on_click(move || {
                    if let Some(ref cb) = *emit.borrow() {
                        cb(BackupMethodRowOutput::ToggleMethod);
                    }
                }),
        )
    }
}

pub type BackupMethodRow = RenderComponent<BackupMethodRowRender>;
