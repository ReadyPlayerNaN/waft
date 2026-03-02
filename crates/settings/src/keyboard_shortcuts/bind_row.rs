//! Dumb widget for a single keyboard shortcut row.
//!
//! Renders a three-column layout: human-readable label (left), key chord
//! (subtitle), action type badge + icon buttons (right).

use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VBox, VCustomButton, VIcon, VLabel};



/// Input data for constructing or updating a bind row.
#[derive(Clone, PartialEq)]
pub struct BindRowProps {
    pub key_chord: String,
    pub action_label: String,
    /// Optional action type badge (e.g. "spawn"). None for niri actions.
    pub action_type: Option<String>,
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
        // Title: human-readable label (hotkey_overlay_title) or action label
        let title = props
            .title
            .as_deref()
            .unwrap_or(&props.action_label);

        let edit_emit = emit.clone();
        let edit_btn = VCustomButton::new(VNode::icon(VIcon::new(
            vec![Icon::Themed("document-edit-symbolic".to_string())],
            16,
        )))
        .css_class("flat")
        .sensitive(props.editable)
        .on_click(move || {
            if let Some(ref cb) = *edit_emit.borrow() {
                cb(BindRowOutput::Edit);
            }
        });

        let delete_emit = emit.clone();
        let delete_btn = VCustomButton::new(VNode::icon(VIcon::new(
            vec![Icon::Themed("user-trash-symbolic".to_string())],
            16,
        )))
        .css_classes(["flat", "destructive-action"])
        .sensitive(props.editable)
        .on_click(move || {
            if let Some(ref cb) = *delete_emit.borrow() {
                cb(BindRowOutput::Delete);
            }
        });

        // Subtitle always shows the key chord for consistent positioning
        let mut row = VActionRow::new(title).subtitle(&props.key_chord);

        if props.editable {
            let mut suffix_box = VBox::horizontal(4).valign(gtk::Align::Center);

            // Action type badge (e.g. "spawn") shown when present
            if let Some(ref action_type) = props.action_type {
                suffix_box = suffix_box.child(VNode::label(
                    VLabel::new(action_type).css_class("dim-label"),
                ));
            }

            suffix_box = suffix_box
                .child(VNode::custom_button(edit_btn))
                .child(VNode::custom_button(delete_btn));

            row = row.suffix(VNode::vbox(suffix_box));
        }

        VNode::action_row(row)
    }
}

pub type BindRow = waft_ui_gtk::vdom::RenderComponent<BindRowRender>;
