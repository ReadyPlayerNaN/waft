//! Agenda GTK4 widget.
//!
//! Displays upcoming calendar events as styled cards with past-event dimming,
//! now/period separators, and meeting link buttons.

use gtk::prelude::*;
use log::{debug, warn};

use super::store::AgendaState;
use super::values::{extract_meeting_links, AgendaEvent, MeetingProvider};

/// GTK4 widget for the agenda display.
pub struct AgendaWidget {
    pub root: gtk::Box,
    spinner: gtk::Spinner,
    content_box: gtk::Box,
    empty_label: gtk::Label,
    error_label: gtk::Label,
}

impl AgendaWidget {
    /// Create a new agenda widget.
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["agenda-container"])
            .build();

        // Header
        let header = gtk::Label::builder()
            .label("Agenda")
            .xalign(0.0)
            .css_classes(["title-3", "agenda-header"])
            .build();

        // Loading spinner
        let spinner = gtk::Spinner::builder()
            .spinning(true)
            .halign(gtk::Align::Center)
            .build();

        // Content box for event rows
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        // Empty state label
        let empty_label = gtk::Label::builder()
            .label("No upcoming events")
            .xalign(0.0)
            .css_classes(["dim-label", "agenda-empty"])
            .visible(false)
            .build();

        // Error label
        let error_label = gtk::Label::builder()
            .label("")
            .xalign(0.0)
            .css_classes(["error", "agenda-error"])
            .visible(false)
            .build();

        root.append(&header);
        root.append(&spinner);
        root.append(&content_box);
        root.append(&empty_label);
        root.append(&error_label);

        content_box.set_visible(false);

        Self {
            root,
            spinner,
            content_box,
            empty_label,
            error_label,
        }
    }

    /// Update the widget to reflect current state.
    pub fn update(&self, state: &AgendaState) {
        if state.loading {
            self.spinner.set_visible(true);
            self.spinner.set_spinning(true);
            self.content_box.set_visible(false);
            self.empty_label.set_visible(false);
            self.error_label.set_visible(false);
            return;
        }

        self.spinner.set_visible(false);
        self.spinner.set_spinning(false);

        if let Some(ref err) = state.error {
            self.content_box.set_visible(false);
            self.empty_label.set_visible(false);
            self.error_label.set_visible(true);
            self.error_label.set_label(err);
            return;
        }

        if state.events.is_empty() {
            self.content_box.set_visible(false);
            self.empty_label.set_visible(true);
            self.error_label.set_visible(false);
            return;
        }

        self.error_label.set_visible(false);
        self.empty_label.set_visible(false);
        self.content_box.set_visible(true);

        // Clear existing rows
        while let Some(child) = self.content_box.first_child() {
            self.content_box.remove(&child);
        }

        // Sort events by start time, then by end time.
        // Filter out events that ended before the query range (e.g. recurring
        // event master instances delivered by EDS outside the requested window).
        let query_since = state.query_since.unwrap_or(0);
        let mut events: Vec<&AgendaEvent> = state
            .events
            .values()
            .filter(|e| e.end_time >= query_since)
            .collect();
        events.sort_by(|a, b| a.start_time.cmp(&b.start_time).then(a.end_time.cmp(&b.end_time)));

        // Current time for past/present detection (use chrono to match event timestamps)
        let now = chrono::Local::now().timestamp();

        let next_period_start = state.next_period_start;

        // Track whether we've inserted the "now" divider and the period separator
        let mut now_divider_inserted = false;
        let mut period_separator_inserted = false;

        for event in &events {
            let is_past = event.end_time <= now;
            debug!(
                "[agenda/widget] '{}': end_time={} now={} is_past={} desc={}chars alt_desc={}chars loc={}chars",
                event.summary,
                event.end_time,
                now,
                is_past,
                event.description.as_ref().map(|d| d.len()).unwrap_or(0),
                event.alt_description.as_ref().map(|d| d.len()).unwrap_or(0),
                event.location.as_ref().map(|d| d.len()).unwrap_or(0),
            );

            // Insert "now" divider: before the first non-past event
            if !now_divider_inserted && !is_past {
                // Only insert if there were past events before
                if self.content_box.first_child().is_some() {
                    let divider = gtk::Separator::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .css_classes(["agenda-divider-now"])
                        .build();
                    self.content_box.append(&divider);
                }
                now_divider_inserted = true;
            }

            // Insert period separator before the first event in the next period
            if !period_separator_inserted {
                if let Some(nps) = next_period_start {
                    if event.start_time >= nps {
                        let separator = build_period_separator(nps);
                        self.content_box.append(&separator);
                        period_separator_inserted = true;
                    }
                }
            }

            let card = build_event_card(event, is_past);
            self.content_box.append(&card);
        }
    }
}

/// Build a period separator with a date label.
fn build_period_separator(timestamp: i64) -> gtk::Box {
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(["agenda-period-separator"])
        .build();

    let label_text = glib::DateTime::from_unix_local(timestamp)
        .and_then(|dt| dt.format("%A, %B %-e"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "Next period".to_string());

    let label = gtk::Label::builder()
        .label(&label_text)
        .xalign(0.0)
        .hexpand(true)
        .css_classes(["dim-label"])
        .build();

    container.append(&label);
    container
}

/// Build a single event card widget.
fn build_event_card(event: &AgendaEvent, is_past: bool) -> gtk::Box {
    let css_classes: Vec<&str> = if is_past {
        vec!["agenda-event-card", "agenda-event-past"]
    } else {
        vec!["agenda-event-card"]
    };

    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .css_classes(css_classes)
        .build();

    // Top line: time range
    let time_text = if event.all_day {
        "All day".to_string()
    } else {
        format_time_range(event.start_time, event.end_time)
    };

    let time_label = gtk::Label::builder()
        .label(&time_text)
        .xalign(0.0)
        .css_classes(["dim-label", "agenda-event-time", "caption"])
        .build();

    // Summary label (ellipsized)
    let summary_label = gtk::Label::builder()
        .label(&event.summary)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["agenda-event-summary"])
        .build();

    card.append(&time_label);
    card.append(&summary_label);

    // Meeting link buttons
    let links = extract_meeting_links(event);
    if !links.is_empty() {
        let btn_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .margin_top(2)
            .build();

        for link in links {
            let label = match link.provider {
                MeetingProvider::GoogleMeet => "Meet",
                MeetingProvider::Zoom => "Zoom",
                MeetingProvider::Teams => "Teams",
            };

            let btn = gtk::Button::builder()
                .label(label)
                .css_classes(["agenda-meeting-btn"])
                .build();

            let url = link.url.clone();
            btn.connect_clicked(move |_| {
                if let Err(e) = gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE) {
                    warn!("[agenda] failed to open meeting URL: {e}");
                }
            });

            btn_row.append(&btn);
        }

        card.append(&btn_row);
    }

    card
}

/// Format a time range as "HH:MM \u{2013} HH:MM" using glib::DateTime.
fn format_time_range(start: i64, end: i64) -> String {
    let start_str = format_timestamp(start);
    let end_str = format_timestamp(end);
    format!("{} \u{2013} {}", start_str, end_str)
}

/// Format a unix timestamp as "HH:MM" in local time using glib::DateTime.
fn format_timestamp(ts: i64) -> String {
    glib::DateTime::from_unix_local(ts)
        .and_then(|dt| dt.format("%H:%M"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "--:--".to_string())
}
