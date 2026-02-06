//! Expanded details content for an agenda event card.

use gtk::prelude::*;

use crate::features::agenda::values::AgendaEvent;
use crate::ui::icon::IconWidget;

use super::attendee_list::AttendeeList;
use super::format::strip_html_tags;

/// Presentational widget showing event details: location, attendees, description.
pub struct AgendaDetails {
    pub root: gtk::Box,
}

impl AgendaDetails {
    pub fn new(event: &AgendaEvent) -> Self {
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

        // Description
        let desc_text = event
            .description
            .as_deref()
            .or_else(|| event.alt_description.as_deref().map(|_| ""))
            .and_then(|_| {
                if let Some(ref desc) = event.description {
                    Some(desc.clone())
                } else { event.alt_description.as_ref().map(|alt| strip_html_tags(alt)) }
            });

        if let Some(desc) = desc_text
            && !desc.trim().is_empty() {
                let truncated = if desc.len() > 300 {
                    format!("{}…", &desc[..300])
                } else {
                    desc
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
