//! Agenda GTK4 widget.
//!
//! Displays upcoming calendar events as styled cards with past-event dimming,
//! now/period separators, and meeting link buttons.

use gtk::prelude::*;
use log::{debug, warn};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::store::AgendaState;
use super::values::{AgendaEvent, MeetingLink, MeetingProvider, extract_meeting_links};

/// GTK4 widget for the agenda display.
pub struct AgendaWidget {
    pub root: gtk::Box,
    spinner: gtk::Spinner,
    content_box: gtk::Box,
    empty_label: gtk::Label,
    error_label: gtk::Label,
    /// Map of event occurrence keys to event card widgets for incremental updates
    event_cards: RefCell<HashMap<String, gtk::Box>>,
    /// Track the "now" divider to avoid duplicates
    now_divider: RefCell<Option<gtk::Separator>>,
    /// Track period separator to avoid duplicates
    period_separator: RefCell<Option<gtk::Box>>,
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
            .label(&crate::i18n::t("agenda-title"))
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
            .label(&crate::i18n::t("agenda-empty"))
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
            event_cards: RefCell::new(HashMap::new()),
            now_divider: RefCell::new(None),
            period_separator: RefCell::new(None),
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

        self.update_events_incremental(state);
    }

    /// Incrementally update events without rebuilding the entire widget tree.
    fn update_events_incremental(&self, state: &AgendaState) {
        // Sort events by start time, then by end time.
        // Filter out events that ended before the query range (e.g. recurring
        // event master instances delivered by EDS outside the requested window).
        let query_since = state.query_since.unwrap_or(0);
        let mut events: Vec<&AgendaEvent> = state
            .events
            .values()
            .filter(|e| e.end_time >= query_since)
            .collect();
        events.sort_by(|a, b| {
            a.start_time
                .cmp(&b.start_time)
                .then(a.end_time.cmp(&b.end_time))
        });

        // Current time for past/present detection
        let now = chrono::Local::now().timestamp();
        let next_period_start = state.next_period_start;

        // Calculate event keys for the new state
        let new_event_keys: HashSet<String> = events.iter().map(|e| e.occurrence_key()).collect();

        // Remove widgets for events no longer present
        let mut current_cards = self.event_cards.borrow_mut();
        let current_keys: Vec<String> = current_cards.keys().cloned().collect();
        for key in current_keys {
            if !new_event_keys.contains(&key) {
                if let Some(widget) = current_cards.remove(&key) {
                    self.content_box.remove(&widget);
                    debug!("[agenda/widget] Removed event: {}", key);
                }
            }
        }
        drop(current_cards);

        // Remove old dividers
        if let Some(divider) = self.now_divider.take() {
            self.content_box.remove(&divider);
        }
        if let Some(separator) = self.period_separator.take() {
            self.content_box.remove(&separator);
        }

        // Rebuild the widget tree with events in correct order
        while let Some(child) = self.content_box.first_child() {
            self.content_box.remove(&child);
        }

        let mut current_cards = self.event_cards.borrow_mut();
        current_cards.clear();

        let mut now_divider_inserted = false;
        let mut period_separator_inserted = false;

        for event in &events {
            let is_past = event.end_time <= now;
            let is_ongoing = !event.all_day && event.start_time <= now && now < event.end_time;

            debug!(
                "[agenda/widget] '{}': end_time={} now={} is_past={} ongoing={} desc={}chars alt_desc={}chars loc={}chars",
                event.summary,
                event.end_time,
                now,
                is_past,
                is_ongoing,
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
                    *self.now_divider.borrow_mut() = Some(divider);
                }
                now_divider_inserted = true;
            }

            // Insert period separator before the first event in the next period
            if !period_separator_inserted {
                if let Some(nps) = next_period_start {
                    if event.start_time >= nps {
                        let separator = build_period_separator(nps);
                        self.content_box.append(&separator);
                        *self.period_separator.borrow_mut() = Some(separator);
                        period_separator_inserted = true;
                    }
                }
            }

            // Create or reuse event card
            let event_key = event.occurrence_key();
            let card = if let Some(existing_card) = current_cards.get(&event_key) {
                // Update existing card's CSS classes based on past/ongoing state
                let mut css_classes = vec!["agenda-event-card"];
                if is_past {
                    css_classes.push("agenda-event-past");
                }
                if is_ongoing {
                    css_classes.push("agenda-event-ongoing");
                }
                existing_card.set_css_classes(&css_classes);
                existing_card.clone()
            } else {
                // Create new card
                let new_card = build_event_card(event, is_past, is_ongoing);
                current_cards.insert(event_key, new_card.clone());
                new_card
            };

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
        .unwrap_or_else(|_| crate::i18n::t("agenda-next-period"));

    let label = gtk::Label::builder()
        .label(&label_text)
        .xalign(0.0)
        .hexpand(true)
        .css_classes(["dim-label"])
        .build();

    container.append(&label);
    container
}

/// Build a single event card widget as a single horizontal row.
fn build_event_card(event: &AgendaEvent, is_past: bool, is_ongoing: bool) -> gtk::Box {
    let mut css_classes: Vec<&str> = vec!["agenda-event-card"];
    if is_past {
        css_classes.push("agenda-event-past");
    }
    if is_ongoing {
        css_classes.push("agenda-event-ongoing");
    }

    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .css_classes(css_classes)
        .build();

    // Time label (fixed width for alignment)
    let time_text = if event.all_day {
        crate::i18n::t("agenda-all-day")
    } else {
        format_time_range(event.start_time, event.end_time)
    };

    let time_label = gtk::Label::builder()
        .label(&time_text)
        .xalign(0.0)
        .width_chars(13)
        .css_classes(["dim-label", "agenda-event-time", "caption"])
        .build();

    // Summary label (ellipsized, takes remaining space)
    let summary_label = gtk::Label::builder()
        .label(&event.summary)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["agenda-event-summary"])
        .build();

    card.append(&time_label);
    card.append(&summary_label);

    // Meeting link action widget
    let links = extract_meeting_links(event);
    if let Some(action) = build_meeting_action(&links) {
        card.append(&action);
    }

    card
}

/// Map a MeetingProvider to a short display label.
fn provider_label(provider: &MeetingProvider) -> &'static str {
    match provider {
        MeetingProvider::GoogleMeet => "Meet",
        MeetingProvider::Zoom => "Zoom",
        MeetingProvider::Teams => "Teams",
    }
}

/// Build the meeting action widget for an event card.
///
/// - 0 links: returns `None`
/// - 1 link: returns a direct button
/// - 2+ links: returns a three-dot menu button with a popover
fn build_meeting_action(links: &[MeetingLink]) -> Option<gtk::Widget> {
    match links.len() {
        0 => None,
        1 => {
            let link = &links[0];
            let btn = gtk::Button::builder()
                .label(provider_label(&link.provider))
                .css_classes(["agenda-meeting-btn"])
                .build();

            let url = link.url.clone();
            btn.connect_clicked(move |_| {
                if let Err(e) =
                    gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                {
                    warn!("[agenda] failed to open meeting URL: {e}");
                }
            });

            Some(btn.upcast())
        }
        _ => {
            let popover_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .css_classes(["agenda-meeting-popover"])
                .build();

            let popover = gtk::Popover::builder().child(&popover_box).build();

            for link in links {
                let btn = gtk::Button::builder()
                    .label(provider_label(&link.provider))
                    .css_classes(["agenda-meeting-btn"])
                    .build();

                let url = link.url.clone();
                let popover_ref = popover.clone();
                btn.connect_clicked(move |_| {
                    if let Err(e) =
                        gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                    {
                        warn!("[agenda] failed to open meeting URL: {e}");
                    }
                    popover_ref.popdown();
                });

                popover_box.append(&btn);
            }

            let menu_btn = gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .popover(&popover)
                .css_classes(["agenda-more-btn"])
                .build();

            Some(menu_btn.upcast())
        }
    }
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
