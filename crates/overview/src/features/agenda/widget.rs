//! Agenda GTK4 widget (smart container).
//!
//! Displays upcoming calendar events as styled cards with past-event dimming,
//! now/period separators, and meeting link buttons. Delegates presentational
//! concerns to extracted UI components in `super::ui`.

use gtk::prelude::*;
use log::debug;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use crate::menu_state::{MenuOp, MenuStore};

use super::store::AgendaState;
use super::ui::agenda_card::{AgendaCard, AgendaCardOutput};
use super::values::AgendaEvent;

/// GTK4 widget for the agenda display.
pub struct AgendaWidget {
    pub root: gtk::Box,
    spinner: gtk::Spinner,
    content_box: gtk::Box,
    empty_label: gtk::Label,
    error_label: gtk::Label,
    /// Map of event occurrence keys to event card widgets for incremental updates
    event_cards: Rc<RefCell<HashMap<String, AgendaCard>>>,
    /// Track the "now" divider to avoid duplicates
    now_divider: RefCell<Option<gtk::Separator>>,
    /// Track period separator to avoid duplicates
    period_separator: RefCell<Option<gtk::Box>>,
    /// MenuStore for tracking popover state
    menu_store: Arc<MenuStore>,
}

impl AgendaWidget {
    /// Create a new agenda widget.
    pub fn new(menu_store: Arc<MenuStore>) -> Self {
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

        let event_cards: Rc<RefCell<HashMap<String, AgendaCard>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Single MenuStore subscription for all cards
        let event_cards_ref = event_cards.clone();
        let menu_store_sub = menu_store.clone();
        menu_store.subscribe(move || {
            let state = menu_store_sub.get_state();
            for card in event_cards_ref.borrow().values() {
                let open = state.active_menu_id.as_deref() == Some(card.menu_id());
                card.set_expanded(open);
            }
        });

        Self {
            root,
            spinner,
            content_box,
            empty_label,
            error_label,
            event_cards,
            now_divider: RefCell::new(None),
            period_separator: RefCell::new(None),
            menu_store,
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
                if let Some(card) = current_cards.remove(&key) {
                    self.content_box.remove(&card.root);
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

            // Create new card
            let event_key = event.occurrence_key();
            let card = AgendaCard::new(event, is_past, is_ongoing, &self.menu_store);

            let menu_store_ref = self.menu_store.clone();
            card.connect_output(move |output| match output {
                AgendaCardOutput::ToggleExpand(menu_id) => {
                    menu_store_ref.emit(MenuOp::OpenMenu(menu_id));
                }
            });

            self.content_box.append(&card.root);
            current_cards.insert(event_key, card);
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
