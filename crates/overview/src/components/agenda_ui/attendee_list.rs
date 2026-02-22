//! Attendee list widget — section icon + vertical list of attendee rows.

use gtk::prelude::*;

use waft_protocol::entity::calendar::CalendarEventAttendee;

use waft_ui_gtk::icons::IconWidget;

use super::attendee_row::AttendeeRow;

/// A list of attendees with a section icon.
pub struct AttendeeList {
    pub root: gtk::Box,
}

impl AttendeeList {
    pub fn new(attendees: &[CalendarEventAttendee]) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let icon = IconWidget::from_name("system-users-symbolic", 16);
        icon.widget().set_valign(gtk::Align::Start);

        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .build();

        for attendee in attendees {
            let row = AttendeeRow::new(attendee);
            list.append(&row.root);
        }

        root.append(icon.widget());
        root.append(&list);

        Self { root }
    }
}
