//! Dumb widget for a single startup entry row.
//!
//! Renders command and arguments as an `adw::ActionRow` with edit and delete
//! suffix buttons.

use waft_ui_gtk::vdom::{RenderCallback, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VBox, VButton};

use crate::i18n::t;

/// Input data for constructing or updating a startup row.
#[derive(Clone, PartialEq)]
pub struct StartupRowProps {
    pub command: String,
    pub args: Vec<String>,
}

/// Output events from a startup row.
#[derive(Debug, Clone)]
pub enum StartupRowOutput {
    Edit,
    Delete,
}

pub(crate) struct StartupRowRender;

impl RenderFn for StartupRowRender {
    type Props = StartupRowProps;
    type Output = StartupRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<StartupRowOutput>) -> VNode {
        let subtitle = if props.args.is_empty() {
            String::new()
        } else {
            props.args.join(" ")
        };

        let edit_emit = emit.clone();
        let edit_btn = VButton::new(t("startup-edit")).on_click(move || {
            if let Some(ref cb) = *edit_emit.borrow() {
                cb(StartupRowOutput::Edit);
            }
        });

        let delete_emit = emit.clone();
        let delete_btn = VButton::new(t("startup-delete")).on_click(move || {
            if let Some(ref cb) = *delete_emit.borrow() {
                cb(StartupRowOutput::Delete);
            }
        });

        let mut row = VActionRow::new(&props.command);
        if !subtitle.is_empty() {
            row = row.subtitle(&subtitle);
        }

        VNode::action_row(
            row.suffix(VNode::vbox(
                VBox::horizontal(4)
                    .valign(gtk::Align::Center)
                    .child(VNode::button(edit_btn))
                    .child(VNode::button(delete_btn)),
            )),
        )
    }
}

pub type StartupRow = waft_ui_gtk::vdom::RenderComponent<StartupRowRender>;
