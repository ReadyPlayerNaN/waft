//! Expanded details content for an agenda event card (RenderFn).

use waft_protocol::entity::calendar::AttendeeStatus;

use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VBox, VIcon, VLabel, VNode};

use super::attendee_list::{AttendeeListComponent, AttendeeListProps};
use super::format::strip_html_tags;

/// Properties for agenda event details.
#[derive(Clone, PartialEq)]
pub struct AgendaDetailsProps {
    pub location: Option<String>,
    pub attendees: Vec<(String, Option<String>, AttendeeStatus)>,
    pub description: Option<String>,
}

pub(crate) struct AgendaDetailsRender;

impl RenderFn for AgendaDetailsRender {
    type Props = AgendaDetailsProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let mut root = VBox::vertical(4).css_class("agenda-event-details");

        // Location row
        if let Some(ref location) = props.location {
            root = root.child(VNode::vbox(
                VBox::horizontal(8)
                    .child(
                        VNode::icon(
                            VIcon::new(vec![Icon::parse("mark-location-symbolic")], 16)
                        )
                    )
                    .child(
                        VNode::label(
                            VLabel::new(location)
                                .xalign(0.0)
                                .wrap(true)
                                .wrap_mode(gtk::pango::WrapMode::WordChar)
                                .css_class("dim-label")
                        )
                    ),
            ));
        }

        // Attendees section
        if !props.attendees.is_empty() {
            root = root.child(VNode::new::<AttendeeListComponent>(AttendeeListProps {
                attendees: props.attendees.clone(),
            }));
        }

        // Description - strip HTML if present
        if let Some(ref desc) = props.description && !desc.trim().is_empty() {
            let display_text = if desc.contains('<') && desc.contains('>') {
                strip_html_tags(desc)
            } else {
                desc.clone()
            };

            let truncated = if display_text.len() > 300 {
                let end = display_text
                    .char_indices()
                    .map(|(i, _)| i)
                    .find(|&i| i >= 300)
                    .unwrap_or(display_text.len());
                format!("{}…", &display_text[..end])
            } else {
                display_text
            };

            root = root.child(VNode::vbox(
                VBox::horizontal(8)
                    .child(
                        VNode::icon(
                            VIcon::new(vec![Icon::parse("text-x-generic-symbolic")], 16)
                        )
                    )
                    .child(
                        VNode::label(
                            VLabel::new(&truncated)
                                .xalign(0.0)
                                .wrap(true)
                                .wrap_mode(gtk::pango::WrapMode::WordChar)
                                .css_class("dim-label")
                        )
                    ),
            ));
        }

        VNode::vbox(root)
    }
}

pub type AgendaDetailsComponent = RenderComponent<AgendaDetailsRender>;
