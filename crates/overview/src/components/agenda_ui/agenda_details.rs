//! Expanded details content for an agenda event card.

use gtk::prelude::*;

use waft_protocol::entity::calendar::CalendarEvent;

use waft_ui_gtk::widgets::icon::IconWidget;

use super::attendee_list::AttendeeList;
use super::format::strip_html_tags;

/// Presentational widget showing event details: location, attendees, description.
pub struct AgendaDetails {
    pub root: gtk::Box,
}

impl AgendaDetails {
    pub fn new(event: &CalendarEvent) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["agenda-event-details"])
            .build();

        // Location row
        if let Some(ref location) = event.location {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();

            let icon = IconWidget::from_name("mark-location-symbolic", 16);
            icon.widget().set_valign(gtk::Align::Start);

            let label = gtk::Label::builder()
                .label(location)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .css_classes(["dim-label"])
                .build();

            row.append(icon.widget());
            row.append(&label);
            root.append(&row);
        }

        // Attendees section
        if !event.attendees.is_empty() {
            let attendee_list = AttendeeList::new(&event.attendees);
            root.append(&attendee_list.root);
        }

        // Description - strip HTML if present
        if let Some(ref desc) = event.description
            && !desc.trim().is_empty()
        {
            // Check if it looks like HTML
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

            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();

            let icon = IconWidget::from_name("text-x-generic-symbolic", 16);
            icon.widget().set_valign(gtk::Align::Start);

            let label = gtk::Label::builder()
                .label(&truncated)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .css_classes(["dim-label"])
                .build();

            row.append(icon.widget());
            row.append(&label);
            root.append(&row);
        }

        Self { root }
    }
}
