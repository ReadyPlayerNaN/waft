//! Calendar agenda component.
//!
//! Subscribes to the `calendar-event` entity type and renders upcoming events
//! sorted by start time. Always visible, showing a placeholder when no events
//! are available.

use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::entity;
use waft_protocol::Urn;

use crate::entity_store::EntityStore;

/// Displays a list of upcoming calendar events.
///
/// Shows a "Calendar" header label followed by event cards sorted by start time.
/// When no events are present, shows a "No upcoming events" placeholder.
pub struct AgendaComponent {
    container: gtk::Box,
    events_container: gtk::Box,
}

impl AgendaComponent {
    pub fn new(store: &Rc<EntityStore>) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let header = gtk::Label::builder()
            .label("Calendar")
            .css_classes(["title-2"])
            .xalign(0.0)
            .build();
        container.append(&header);

        let events_container = gtk::Box::new(gtk::Orientation::Vertical, 4);
        container.append(&events_container);

        // Show placeholder initially
        let placeholder = gtk::Label::builder()
            .label("No upcoming events")
            .css_classes(["dim-label"])
            .xalign(0.0)
            .build();
        events_container.append(&placeholder);

        let store_ref = store.clone();
        let events_container_ref = events_container.clone();

        store.subscribe_type(entity::calendar::ENTITY_TYPE, move || {
            let mut entities: Vec<(Urn, entity::calendar::CalendarEvent)> =
                store_ref.get_entities_typed(entity::calendar::ENTITY_TYPE);

            // Sort by start_time ascending
            entities.sort_by_key(|(_urn, event)| event.start_time);

            // Clear existing children
            while let Some(child) = events_container_ref.first_child() {
                events_container_ref.remove(&child);
            }

            if entities.is_empty() {
                let placeholder = gtk::Label::builder()
                    .label("No upcoming events")
                    .css_classes(["dim-label"])
                    .xalign(0.0)
                    .build();
                events_container_ref.append(&placeholder);
                return;
            }

            for (_urn, event) in &entities {
                let card = build_event_card(event);
                events_container_ref.append(&card);
            }
        });

        Self {
            container,
            events_container,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

/// Build a GTK box representing a single calendar event.
fn build_event_card(event: &entity::calendar::CalendarEvent) -> gtk::Box {
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .css_classes(["agenda-event-card"])
        .build();

    let summary_label = gtk::Label::builder()
        .label(&event.summary)
        .css_classes(["title-4"])
        .xalign(0.0)
        .build();
    card.append(&summary_label);

    let time_label = gtk::Label::builder()
        .label(&format_time_range(event))
        .css_classes(["caption"])
        .xalign(0.0)
        .build();
    card.append(&time_label);

    if let Some(ref location) = event.location {
        let location_label = gtk::Label::builder()
            .label(location)
            .css_classes(["dim-label", "caption"])
            .xalign(0.0)
            .build();
        card.append(&location_label);
    }

    card
}

/// Format the time range for display.
///
/// For all-day events, returns "All day". Otherwise formats start and end
/// times as HH:MM - HH:MM using the local timezone.
fn format_time_range(event: &entity::calendar::CalendarEvent) -> String {
    if event.all_day {
        return "All day".to_string();
    }

    let start = chrono::DateTime::from_timestamp(event.start_time, 0);
    let end = chrono::DateTime::from_timestamp(event.end_time, 0);

    match (start, end) {
        (Some(s), Some(e)) => {
            let local_start = s.with_timezone(&chrono::Local);
            let local_end = e.with_timezone(&chrono::Local);
            format!(
                "{} - {}",
                local_start.format("%H:%M"),
                local_end.format("%H:%M"),
            )
        }
        _ => "Unknown time".to_string(),
    }
}
