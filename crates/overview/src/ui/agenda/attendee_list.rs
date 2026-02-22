//! Attendee list widget -- section icon + vertical list of attendee rows (RenderFn).

use waft_protocol::entity::calendar::AttendeeStatus;

use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VBox, VIcon, VNode};

use super::attendee_row::{AttendeeRowComponent, AttendeeRowProps};

/// Properties for an attendee list section.
#[derive(Clone, PartialEq)]
pub struct AttendeeListProps {
    pub attendees: Vec<(String, Option<String>, AttendeeStatus)>,
}

pub(crate) struct AttendeeListRender;

impl RenderFn for AttendeeListRender {
    type Props = AttendeeListProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let mut list = VBox::vertical(2);

        for (email, name, status) in &props.attendees {
            list = list.child(
                VNode::new::<AttendeeRowComponent>(AttendeeRowProps {
                    name: name.clone(),
                    email: email.clone(),
                    status: *status,
                })
                .key(email),
            );
        }

        VNode::vbox(
            VBox::horizontal(8)
                .child(
                    VNode::icon(
                        VIcon::new(vec![Icon::parse("system-users-symbolic")], 16)
                    )
                )
                .child(VNode::vbox(list)),
        )
    }
}

pub type AttendeeListComponent = RenderComponent<AttendeeListRender>;
