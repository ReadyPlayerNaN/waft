//! Agenda widget with meeting items and action buttons.

use adw::prelude::*;

/// Represents a single meeting item with its data
#[derive(Clone, Debug)]
pub struct MeetingItem {
    pub time: String,
    pub title: String,
    pub has_google_meet: bool,
    pub has_zoom: bool,
    pub has_teams: bool,
}

/// Build a complete agenda section with meeting items and action buttons
pub fn build_agenda_section(meetings: Vec<MeetingItem>) -> gtk::Widget {
    let agenda_group = adw::PreferencesGroup::builder()
        .title("Today's Agenda")
        .build();

    // Helper to add a meeting item
    let add_meeting = |group: &adw::PreferencesGroup, meeting: &MeetingItem| {
        let row = adw::ActionRow::builder()
            .title(&meeting.title)
            .subtitle(&meeting.time)
            .build();
        row.set_activatable(false);

        // Create action buttons container
        let actions_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        // Add action buttons based on available meeting types
        if meeting.has_google_meet {
            let google_btn = gtk::Button::builder()
                .label("Open Google Meet")
                .css_classes(["pill", "meeting-action"])
                .build();
            actions_box.append(&google_btn);
        }
        if meeting.has_zoom {
            let zoom_btn = gtk::Button::builder()
                .label("Open Zoom Meeting")
                .css_classes(["pill", "meeting-action"])
                .build();
            actions_box.append(&zoom_btn);
        }
        if meeting.has_teams {
            let teams_btn = gtk::Button::builder()
                .label("Open Teams Meeting")
                .css_classes(["pill", "meeting-action"])
                .build();
            actions_box.append(&teams_btn);
        }

        row.add_suffix(&actions_box);
        group.add(&row);
    };

    for meeting in &meetings {
        add_meeting(&agenda_group, meeting);
    }

    agenda_group.upcast::<gtk::Widget>()
}
