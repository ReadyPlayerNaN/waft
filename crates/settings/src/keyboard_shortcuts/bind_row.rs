//! Dumb widget for a single keyboard shortcut row.
//!
//! Renders key chord and action as an `adw::ActionRow` with edit and delete
//! suffix buttons (hidden for read-only entries).

use waft_ui_gtk::vdom::{RenderCallback, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VBox, VButton, VLabel};

use crate::i18n::t;

/// Input data for constructing or updating a bind row.
#[derive(Clone, PartialEq)]
pub struct BindRowProps {
    pub key_chord: String,
    pub action_label: String,
    pub title: Option<String>,
    pub editable: bool,
}

/// Output events from a bind row.
#[derive(Debug, Clone)]
pub enum BindRowOutput {
    Edit,
    Delete,
}

pub(crate) struct BindRowRender;

impl RenderFn for BindRowRender {
    type Props = BindRowProps;
    type Output = BindRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<BindRowOutput>) -> VNode {
        let title = props
            .title
            .as_deref()
            .unwrap_or(&props.key_chord);

        let edit_emit = emit.clone();
        let edit_btn = VButton::new(t("startup-edit"))
            .sensitive(props.editable)
            .on_click(move || {
                if let Some(ref cb) = *edit_emit.borrow() {
                    cb(BindRowOutput::Edit);
                }
            });

        let delete_emit = emit.clone();
        let delete_btn = VButton::new(t("startup-delete"))
            .sensitive(props.editable)
            .on_click(move || {
                if let Some(ref cb) = *delete_emit.borrow() {
                    cb(BindRowOutput::Delete);
                }
            });

        let mut row = VActionRow::new(title).subtitle(&props.action_label);

        if props.editable {
            // Show key chord as a dim label on the right when we have a title
            if props.title.is_some() {
                row = row.suffix(VNode::label(
                    VLabel::new(&props.key_chord).css_class("dim-label"),
                ));
            }

            row = row.suffix(VNode::vbox(
                VBox::horizontal(4)
                    .valign(gtk::Align::Center)
                    .child(VNode::button(edit_btn))
                    .child(VNode::button(delete_btn)),
            ));
        }

        VNode::action_row(row)
    }
}

pub type BindRow = waft_ui_gtk::vdom::RenderComponent<BindRowRender>;
