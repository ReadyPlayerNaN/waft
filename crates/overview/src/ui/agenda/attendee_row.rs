//! Single attendee row widget with status icon and name (RenderFn).

use waft_protocol::entity::calendar::AttendeeStatus;

use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VBox, VIcon, VLabel, VNode};

/// Map an attendee's participation status to an icon name.
pub fn attendee_status_icon_name(status: &AttendeeStatus) -> &'static str {
    match status {
        AttendeeStatus::Accepted => "object-select-symbolic",
        AttendeeStatus::Declined => "window-close-symbolic",
        AttendeeStatus::Tentative => "dialog-question-symbolic",
        AttendeeStatus::NeedsAction => "mail-unread-symbolic",
    }
}

/// Properties for a single attendee row.
#[derive(Clone, PartialEq)]
pub struct AttendeeRowProps {
    pub name: Option<String>,
    pub email: String,
    pub status: AttendeeStatus,
}

pub(crate) struct AttendeeRowRender;

impl RenderFn for AttendeeRowRender {
    type Props = AttendeeRowProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let icon_name = attendee_status_icon_name(&props.status);
        let display_name = props.name.as_deref().unwrap_or(&props.email);

        VNode::vbox(
            VBox::horizontal(4)
                .child(
                    VNode::icon(
                        VIcon::new(vec![Icon::parse(icon_name)], 12)
                    )
                )
                .child(
                    VNode::label(
                        VLabel::new(display_name)
                            .xalign(0.0)
                            .ellipsize(gtk::pango::EllipsizeMode::End)
                            .css_class("dim-label")
                    )
                ),
        )
    }
}

pub type AttendeeRowComponent = RenderComponent<AttendeeRowRender>;
