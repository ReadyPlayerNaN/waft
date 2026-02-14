//! Calendar agenda component with expandable cards and smart features.
//!
//! Subscribes to `calendar-event` entities and renders them as sophisticated
//! cards with expandable details, attendee lists, meeting link buttons, and
//! past/ongoing event detection.
//!
//! When a date is selected in the calendar grid, the agenda filters to that
//! single day only. When no date is selected, it shows today+tomorrow events.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::NaiveDate;
use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity;

use crate::calendar_selection::CalendarSelectionStore;
use crate::components::agenda_ui::agenda_card::{AgendaCard, AgendaCardOutput};
use crate::menu_state::{MenuOp, MenuStore};
use waft_client::EntityStore;

/// Displays upcoming calendar events with sophisticated UI.
///
/// Features:
/// - Expandable cards with location, attendees, description
/// - Past events are dimmed, ongoing events highlighted
/// - "Now" divider separates past from current/future
/// - Show/hide past events toggle with animation
/// - Smart meeting buttons (1 link = button, 2+ = popover)
/// - Incremental HashMap-based updates (no full rebuild)
/// - Future events grouped by day with date section headers
/// - Calendar selection filtering: single-day or today+tomorrow
pub struct AgendaComponent {
    container: gtk::Box,
    #[allow(dead_code)]
    content_box: gtk::Box,
    #[allow(dead_code)]
    empty_label: gtk::Label,
    #[allow(dead_code)]
    past_revealer: gtk::Revealer,
    #[allow(dead_code)]
    past_box: gtk::Box,
    show_past_btn: gtk::ToggleButton,
    /// Map of occurrence keys to card widgets
    #[allow(dead_code)]
    event_cards: Rc<RefCell<HashMap<String, Rc<AgendaCard>>>>,
    #[allow(dead_code)]
    now_divider: RefCell<Option<gtk::Separator>>,
    _store: Rc<EntityStore>,
    _menu_store: Rc<MenuStore>,
    _selection_store: Rc<CalendarSelectionStore>,
}

impl AgendaComponent {
    pub fn new(
        store: &Rc<EntityStore>,
        menu_store: &Rc<MenuStore>,
        selection_store: &Rc<CalendarSelectionStore>,
        show_header: bool,
    ) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["agenda-container"])
            .build();

        // Header row: title + show-past toggle
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let header_label = gtk::Label::builder()
            .label(crate::i18n::t("agenda-title"))
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["title-3", "agenda-header"])
            .build();

        let show_past_btn = gtk::ToggleButton::builder()
            .icon_name("task-past-due-symbolic")
            .tooltip_text(crate::i18n::t("agenda-show-past-tooltip"))
            .css_classes(["agenda-show-past-pill"])
            .active(true) // Start with past events visible
            .build();

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
            .reveal_child(true) // Start revealed
            .build();
        past_revealer.set_child(Some(&past_box));

        // Content box for current/future events
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        // Empty state label
        let empty_label = gtk::Label::builder()
            .label(crate::i18n::t("agenda-empty"))
            .xalign(0.0)
            .css_classes(["dim-label", "agenda-empty"])
            .visible(false)
            .build();

        if show_header {
            container.append(&header);
        }
        container.append(&past_revealer);
        container.append(&content_box);
        container.append(&empty_label);

        content_box.set_visible(false);

        let event_cards: Rc<RefCell<HashMap<String, Rc<AgendaCard>>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Toggle past events visibility
        let past_revealer_toggle = past_revealer.clone();
        let content_box_toggle = content_box.clone();
        let empty_label_toggle = empty_label.clone();
        show_past_btn.connect_toggled(move |btn| {
            let show_past = btn.is_active();
            past_revealer_toggle.set_reveal_child(show_past);

            // Update tooltip
            if show_past {
                btn.set_tooltip_text(Some(&crate::i18n::t("agenda-hide-past-tooltip")));
            } else {
                btn.set_tooltip_text(Some(&crate::i18n::t("agenda-show-past-tooltip")));
            }

            // If hiding past and no future events, show empty label
            if !show_past && content_box_toggle.first_child().is_none() {
                content_box_toggle.set_visible(false);
                empty_label_toggle.set_visible(true);
            }
        });

        // Subscribe to MenuStore for expansion state sync
        let event_cards_menu = event_cards.clone();
        let menu_store_sync = menu_store.clone();
        menu_store.subscribe(move || {
            let cards = event_cards_menu.borrow();
            let state = menu_store_sync.get_state();

            for card in cards.values() {
                let should_be_open = state
                    .active_menu_id
                    .as_ref()
                    .map(|id| id == card.menu_id())
                    .unwrap_or(false);
                card.set_expanded(should_be_open);
            }
        });

        // Shared rebuild closure for both entity and selection subscriptions
        let rebuild = {
            let store_ref = store.clone();
            let event_cards_ref = event_cards.clone();
            let past_box_ref = past_box.clone();
            let content_box_ref = content_box.clone();
            let empty_label_ref = empty_label.clone();
            let menu_store_ref = menu_store.clone();
            let selection_store_ref = selection_store.clone();
            let now_divider = Rc::new(RefCell::new(None::<gtk::Separator>));

            Rc::new(move || {
                let selected_date = selection_store_ref.get_state().selected_date;
                Self::update_events(
                    &store_ref,
                    &event_cards_ref,
                    &past_box_ref,
                    &content_box_ref,
                    &empty_label_ref,
                    &menu_store_ref,
                    &now_divider,
                    selected_date,
                );
            })
        };

        // Subscribe to calendar events
        let rebuild_entity = rebuild.clone();
        store.subscribe_type(entity::calendar::ENTITY_TYPE, move || {
            rebuild_entity();
        });

        // Subscribe to calendar selection changes
        let rebuild_selection = rebuild.clone();
        selection_store.subscribe(move || {
            rebuild_selection();
        });

        Self {
            container,
            content_box,
            empty_label,
            past_revealer,
            past_box,
            show_past_btn,
            event_cards,
            now_divider: RefCell::new(None),
            _store: store.clone(),
            _menu_store: menu_store.clone(),
            _selection_store: selection_store.clone(),
        }
    }

    /// Update event display based on current entities and optional date filter.
    ///
    /// Two modes:
    /// - **No selection** (today+tomorrow): Past/future split with revealer and
    ///   day group headers ("Today", "Tomorrow").
    /// - **Date selected**: Flat chronological list of ALL events for that day in
    ///   `content_box`. Past events are dimmed, ongoing highlighted, but no
    ///   past/future split. The past revealer is unused.
    #[allow(clippy::too_many_arguments)]
    fn update_events(
        store: &Rc<EntityStore>,
        event_cards: &Rc<RefCell<HashMap<String, Rc<AgendaCard>>>>,
        past_box: &gtk::Box,
        content_box: &gtk::Box,
        empty_label: &gtk::Label,
        menu_store: &Rc<MenuStore>,
        now_divider: &Rc<RefCell<Option<gtk::Separator>>>,
        selected_date: Option<NaiveDate>,
    ) {
        let entities: Vec<(Urn, entity::calendar::CalendarEvent)> =
            store.get_entities_typed(entity::calendar::ENTITY_TYPE);

        let local_now = chrono::Local::now();
        let now = local_now.timestamp();

        // Determine time window based on selection
        let (filter_start, filter_end) = if let Some(date) = selected_date {
            // Single day filter: [start_of_date, end_of_date)
            let start = date
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always valid")
                .and_local_timezone(chrono::Local)
                .earliest()
                .unwrap_or(local_now)
                .timestamp();
            let end = (date + chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always valid")
                .and_local_timezone(chrono::Local)
                .earliest()
                .unwrap_or(local_now)
                .timestamp();
            (start, end)
        } else {
            // Default: today+tomorrow
            let today_midnight = local_now
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always valid");
            let start_of_today = today_midnight
                .and_local_timezone(chrono::Local)
                .earliest()
                .unwrap_or(local_now)
                .timestamp();
            let day_after_tomorrow = (local_now.date_naive() + chrono::Duration::days(2))
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always valid");
            let end_of_tomorrow = day_after_tomorrow
                .and_local_timezone(chrono::Local)
                .earliest()
                .unwrap_or(local_now)
                .timestamp();
            (start_of_today, end_of_tomorrow)
        };

        let mut entities: Vec<_> = entities
            .into_iter()
            .filter(|(_, event)| event.start_time < filter_end && event.end_time > filter_start)
            .collect();

        // Sort by start_time, then end_time
        entities.sort_by(|a, b| {
            a.1.start_time
                .cmp(&b.1.start_time)
                .then(a.1.end_time.cmp(&b.1.end_time))
        });

        // Clear containers
        while let Some(child) = past_box.first_child() {
            past_box.remove(&child);
        }
        while let Some(child) = content_box.first_child() {
            content_box.remove(&child);
        }

        // Handle empty state
        if entities.is_empty() {
            content_box.set_visible(false);
            empty_label.set_visible(true);
            event_cards.borrow_mut().clear();
            *now_divider.borrow_mut() = None;
            return;
        }

        empty_label.set_visible(false);

        let mut cards_map = event_cards.borrow_mut();
        let mut new_cards = HashMap::new();

        // Treat selecting today the same as no selection (shows today+tomorrow
        // with past/future split, identical to the default view).
        let today_date = local_now.date_naive();
        let use_selected_mode = selected_date.is_some_and(|d| d != today_date);

        if use_selected_mode {
            // --- Selected date mode: flat chronological list in content_box ---
            // No past/future split, no day headers, no revealer usage.
            *now_divider.borrow_mut() = None;

            for (_urn, event) in &entities {
                let occurrence_key = format!("{}@{}", event.uid, event.start_time);
                let is_past = event.end_time <= now;
                let is_ongoing = event.start_time <= now && now < event.end_time;

                let card = if let Some(existing) = cards_map.get(&occurrence_key) {
                    existing.clone()
                } else {
                    let new_card = Rc::new(AgendaCard::new(event, is_past, is_ongoing, menu_store));

                    let menu_store_toggle = menu_store.clone();
                    new_card.connect_output(move |AgendaCardOutput::ToggleExpand(menu_id)| {
                        menu_store_toggle.emit(MenuOp::OpenMenu(menu_id));
                    });

                    new_card
                };

                content_box.append(&card.root);
                new_cards.insert(occurrence_key, card);
            }

            content_box.set_visible(true);
        } else {
            // --- Default mode: today+tomorrow with past/future split ---
            let (past_events, future_events): (Vec<_>, Vec<_>) = entities
                .into_iter()
                .partition(|(_, event)| event.end_time <= now);

            // Render past events into past_box (inside revealer)
            for (_urn, event) in &past_events {
                let occurrence_key = format!("{}@{}", event.uid, event.start_time);

                let card = if let Some(existing) = cards_map.get(&occurrence_key) {
                    existing.clone()
                } else {
                    let new_card = Rc::new(AgendaCard::new(event, true, false, menu_store));

                    let menu_store_toggle = menu_store.clone();
                    new_card.connect_output(move |AgendaCardOutput::ToggleExpand(menu_id)| {
                        menu_store_toggle.emit(MenuOp::OpenMenu(menu_id));
                    });

                    new_card
                };

                past_box.append(&card.root);
                new_cards.insert(occurrence_key, card);
            }

            // Add "now" divider if we have both past and future events
            if !past_events.is_empty() && !future_events.is_empty() {
                let divider = gtk::Separator::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .css_classes(["agenda-divider-now"])
                    .build();
                past_box.append(&divider);
                *now_divider.borrow_mut() = Some(divider);
            } else {
                *now_divider.borrow_mut() = None;
            }

            // Group future events by calendar day
            let tomorrow_date = today_date + chrono::Duration::days(1);

            let mut day_groups: Vec<(
                chrono::NaiveDate,
                Vec<(&Urn, &entity::calendar::CalendarEvent)>,
            )> = Vec::new();

            for (urn, event) in &future_events {
                let event_date = chrono::DateTime::from_timestamp(event.start_time, 0)
                    .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
                    .unwrap_or(today_date);

                if let Some(group) = day_groups.iter_mut().find(|(date, _)| *date == event_date) {
                    group.1.push((urn, event));
                } else {
                    day_groups.push((event_date, vec![(urn, event)]));
                }
            }

            // Render future events grouped by day
            for (date, day_events) in &day_groups {
                let day_label_text = if *date == today_date {
                    crate::i18n::t("agenda-today")
                } else if *date == tomorrow_date {
                    crate::i18n::t("agenda-tomorrow")
                } else {
                    date.format("%A, %b %e").to_string()
                };

                let day_label = gtk::Label::builder()
                    .label(&day_label_text)
                    .xalign(0.0)
                    .css_classes(["dim-label", "agenda-day-header"])
                    .build();
                content_box.append(&day_label);

                for (_urn, event) in day_events {
                    let occurrence_key = format!("{}@{}", event.uid, event.start_time);
                    let is_ongoing = event.start_time <= now && now < event.end_time;

                    let card = if let Some(existing) = cards_map.get(&occurrence_key) {
                        existing.clone()
                    } else {
                        let new_card =
                            Rc::new(AgendaCard::new(event, false, is_ongoing, menu_store));

                        let menu_store_toggle = menu_store.clone();
                        new_card.connect_output(move |AgendaCardOutput::ToggleExpand(menu_id)| {
                            menu_store_toggle.emit(MenuOp::OpenMenu(menu_id));
                        });

                        new_card
                    };

                    content_box.append(&card.root);
                    new_cards.insert(occurrence_key, card);
                }
            }

            content_box.set_visible(!future_events.is_empty());
        }

        // Update cards map
        *cards_map = new_cards;
    }

    pub fn past_events_button(&self) -> &gtk::ToggleButton {
        &self.show_past_btn
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}
