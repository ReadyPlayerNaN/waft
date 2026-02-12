//! Single attendee row widget with status icon and name.

use gtk::prelude::*;

use waft_protocol::entity::calendar::AttendeeStatus;
use waft_protocol::entity::calendar::CalendarEventAttendee;

use crate::ui::icon::IconWidget;

/// Map an attendee's participation status to an icon name.
pub fn attendee_status_icon_name(status: &AttendeeStatus) -> &'static str {
    match status {
        AttendeeStatus::Accepted => "object-select-symbolic",
        AttendeeStatus::Declined => "window-close-symbolic",
        AttendeeStatus::Tentative => "dialog-question-symbolic",
        AttendeeStatus::NeedsAction => "mail-unread-symbolic",
    }
}

/// A single attendee row: status icon + display name.
pub struct AttendeeRow {
    pub root: gtk::Box,
}

impl AttendeeRow {
    pub fn new(attendee: &CalendarEventAttendee) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();

        let icon_name = attendee_status_icon_name(&attendee.status);
        let icon = IconWidget::from_name(icon_name, 12);
        icon.widget().set_valign(gtk::Align::Center);

        let display_name = attendee.name.as_deref().unwrap_or(&attendee.email);

        let label = gtk::Label::builder()
            .label(display_name)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["dim-label"])
            .build();

        root.append(icon.widget());
        root.append(&label);

        Self { root }
    }
}
