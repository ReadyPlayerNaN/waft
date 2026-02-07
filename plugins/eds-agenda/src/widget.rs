//! Agenda GTK4 widget (smart container).
//!
//! Displays upcoming calendar events as styled cards with past-event dimming,
//! now/period separators, and meeting link buttons. Delegates presentational
//! concerns to extracted UI components in `super::ui`.

use gtk::prelude::*;
use log::debug;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use waft_core::menu_state::{MenuOp, MenuStore};
// Removed: trigger_window_resize not available in plugin context

use super::store::{AgendaOp, AgendaState, AgendaStore};
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
    menu_store: Rc<MenuStore>,
    /// Toggle button for showing/hiding past events
    show_past_btn: gtk::ToggleButton,
    /// Revealer wrapping past events for smooth slide animation
    past_revealer: gtk::Revealer,
    /// Container inside the revealer holding past event cards and the now divider
    past_box: gtk::Box,
}

impl AgendaWidget {
    /// Create a new agenda widget.
    pub fn new(menu_store: Rc<MenuStore>, agenda_store: Rc<AgendaStore>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["agenda-container"])
            .build();

        // Header row: title + show-past pill
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let header_label = gtk::Label::builder()
            .label(waft_plugin_api::i18n::t("agenda-title"))
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["title-3", "agenda-header"])
            .build();

        let show_past_btn = gtk::ToggleButton::builder()
            .icon_name("task-past-due-symbolic")
            .tooltip_text(waft_plugin_api::i18n::t("agenda-hide-past-tooltip"))
            .css_classes(["agenda-show-past-pill"])
            .active(false)
            .build();

        {
            let store = agenda_store.clone();
            let btn = show_past_btn.clone();
            show_past_btn.connect_toggled(move |_| {
                store.emit(AgendaOp::SetShowPast(!btn.is_active()));
            });
        }

        header.append(&header_label);
        header.append(&show_past_btn);

        // Revealer for past events with slide-down animation
        let past_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        let past_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(true)
            .build();
        past_revealer.set_child(Some(&past_box));

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
            .label(waft_plugin_api::i18n::t("agenda-empty"))
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

        // Trigger window resize after past-events animation completes
        // and show empty label if no future events remain visible.
        {
            let content_box_ref = content_box.clone();
            let empty_label_ref = empty_label.clone();
            past_revealer.connect_child_revealed_notify(move |rev| {
                // Removed: trigger_window_resize not available in plugin context
                if !rev.is_child_revealed() && rev.next_sibling().is_none() {
                    content_box_ref.set_visible(false);
                    empty_label_ref.set_visible(true);
                }
            });
        }

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
            show_past_btn,
            past_revealer,
            past_box,
        }
    }

    /// Update the widget to reflect current state.
    pub fn update(&self, state: &AgendaState) {
        self.show_past_btn.set_active(!state.show_past);

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

        // Build set of desired event keys for diffing
        let desired_keys: std::collections::HashSet<String> = events
            .iter()
            .map(|e| e.occurrence_key())
            .collect();

        let mut current_cards = self.event_cards.borrow_mut();

        // Remove cards for events that are no longer in the state
        let mut keys_to_remove = Vec::new();
        for (key, card) in current_cards.iter() {
            if !desired_keys.contains(key) {
                card.root.unparent();
                keys_to_remove.push(key.clone());
            }
        }
        for key in keys_to_remove {
            current_cards.remove(&key);
        }

        // Track state for dividers/separators
        let mut has_past_events = false;
        let mut has_future_events = false;
        let mut now_divider_needed = false;
        let mut period_separator_needed = false;
        let mut period_separator_position: Option<usize> = None;

        // Process events to determine which exist, which need creation, and ordering
        let mut past_cards_ordered = Vec::new();
        let mut future_cards_ordered = Vec::new();

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

            let event_key = event.occurrence_key();

            if is_past {
                has_past_events = true;
                past_cards_ordered.push((event_key, event, is_past, is_ongoing));
            } else {
                has_future_events = true;

                // Check if we need period separator before this event
                if period_separator_position.is_none()
                    && let Some(nps) = next_period_start
                        && event.start_time >= nps {
                            period_separator_needed = true;
                            period_separator_position = Some(future_cards_ordered.len());
                        }

                future_cards_ordered.push((event_key, event, is_past, is_ongoing));
            }
        }

        // Determine if we need the now divider
        if has_past_events && has_future_events {
            now_divider_needed = true;
        }

        // Update or create past event cards and reorder them in past_box
        let mut prev_widget: Option<gtk::Widget> = None;
        for (event_key, event, is_past, is_ongoing) in &past_cards_ordered {
            let card = if let Some(existing_card) = current_cards.get_mut(event_key) {
                // Update existing card in place
                existing_card.update_past_state(*is_past, *is_ongoing);
                existing_card
            } else {
                // Create new card
                let new_card = AgendaCard::new(event, *is_past, *is_ongoing, &self.menu_store);
                let menu_store_ref = self.menu_store.clone();
                new_card.connect_output(move |output| match output {
                    AgendaCardOutput::ToggleExpand(menu_id) => {
                        menu_store_ref.emit(MenuOp::OpenMenu(menu_id));
                    }
                });
                self.past_box.append(&new_card.root);
                current_cards.insert(event_key.clone(), new_card);
                current_cards.get_mut(event_key).unwrap()
            };

            // Reorder the card if needed
            if let Some(ref prev) = prev_widget {
                self.past_box.reorder_child_after(&card.root, Some(prev));
            } else {
                // First card - ensure it's at the top
                self.past_box.reorder_child_after(&card.root, None::<&gtk::Widget>);
            }

            prev_widget = Some(card.root.clone().upcast());
        }

        // Update or create/remove the now divider
        let mut now_divider = self.now_divider.borrow_mut();
        if now_divider_needed {
            if now_divider.is_none() {
                let divider = gtk::Separator::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .css_classes(["agenda-divider-now"])
                    .build();
                self.past_box.append(&divider);
                *now_divider = Some(divider);
            }
            // Reorder divider to be after all past events
            if let Some(ref div) = *now_divider
                && let Some(ref prev) = prev_widget {
                    self.past_box.reorder_child_after(div, Some(prev));
                }
        } else {
            if let Some(ref div) = *now_divider {
                div.unparent();
            }
            *now_divider = None;
        }

        // Update or create future event cards and reorder them in content_box
        // Need to account for past_revealer being first, and optional period_separator
        let past_revealer_in_box = self.past_revealer.parent().is_some();
        let mut prev_widget: Option<gtk::Widget> = if past_revealer_in_box {
            Some(self.past_revealer.clone().upcast())
        } else {
            None
        };

        for (idx, (event_key, event, is_past, is_ongoing)) in future_cards_ordered.iter().enumerate() {
            // Insert period separator before this card if needed
            if let Some(sep_pos) = period_separator_position
                && idx == sep_pos {
                    let mut period_separator = self.period_separator.borrow_mut();
                    if period_separator.is_none() {
                        let separator = build_period_separator(next_period_start.unwrap());
                        self.content_box.append(&separator);
                        *period_separator = Some(separator);
                    }
                    if let Some(ref sep) = *period_separator {
                        if let Some(ref prev) = prev_widget {
                            self.content_box.reorder_child_after(sep, Some(prev));
                        } else {
                            self.content_box.reorder_child_after(sep, None::<&gtk::Widget>);
                        }
                        prev_widget = Some(sep.clone().upcast());
                    }
                }

            let card = if let Some(existing_card) = current_cards.get_mut(event_key) {
                // Update existing card in place
                existing_card.update_past_state(*is_past, *is_ongoing);
                existing_card
            } else {
                // Create new card
                let new_card = AgendaCard::new(event, *is_past, *is_ongoing, &self.menu_store);
                let menu_store_ref = self.menu_store.clone();
                new_card.connect_output(move |output| match output {
                    AgendaCardOutput::ToggleExpand(menu_id) => {
                        menu_store_ref.emit(MenuOp::OpenMenu(menu_id));
                    }
                });
                self.content_box.append(&new_card.root);
                current_cards.insert(event_key.clone(), new_card);
                current_cards.get_mut(event_key).unwrap()
            };

            // Reorder the card if needed
            if let Some(ref prev) = prev_widget {
                self.content_box.reorder_child_after(&card.root, Some(prev));
            } else {
                self.content_box.reorder_child_after(&card.root, None::<&gtk::Widget>);
            }

            prev_widget = Some(card.root.clone().upcast());
        }

        // Remove period separator if no longer needed
        if !period_separator_needed {
            let mut period_separator = self.period_separator.borrow_mut();
            if let Some(ref sep) = *period_separator {
                sep.unparent();
            }
            *period_separator = None;
        }

        // Add or remove the past revealer from content_box
        if has_past_events {
            if !past_revealer_in_box {
                self.content_box.prepend(&self.past_revealer);
            }
            self.past_revealer.set_reveal_child(state.show_past);
        } else if past_revealer_in_box {
            self.past_revealer.unparent();
        }

        // If no future events and past is already collapsed, show empty immediately.
        // (When past is animating to collapse, child_revealed_notify handles it.)
        if !has_future_events && !state.show_past && !self.past_revealer.is_child_revealed() {
            self.content_box.set_visible(false);
            self.empty_label.set_visible(true);
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
