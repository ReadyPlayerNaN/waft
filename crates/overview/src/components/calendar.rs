//! Calendar smart container component.
//!
//! Subscribes to the entity store for calendar events and the calendar
//! selection store for navigation/selection state. Renders a month grid
//! with navigation buttons and routes day clicks to the selection store.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Datelike, Local, NaiveDate};
use gtk::glib;
use gtk::prelude::*;

use waft_protocol::entity;

use crate::calendar_selection::{CalendarSelectionOp, CalendarSelectionStore};
use crate::ui::calendar::month_grid::{MonthGrid, MonthGridOutput, MonthGridProps};
use waft_client::EntityStore;

/// Calendar month-grid component with navigation and event dots.
///
/// Smart container that:
/// - Subscribes to `EntityStore` for calendar-event entities
/// - Subscribes to `CalendarSelectionStore` for viewed month and selected date
/// - Renders a `MonthGrid` with prev/next month navigation
/// - Routes day clicks to `CalendarSelectionStore` (toggle behavior)
pub struct CalendarComponent {
    container: gtk::Box,
    _entity_store: Rc<EntityStore>,
    _selection_store: Rc<CalendarSelectionStore>,
}

impl CalendarComponent {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        selection_store: &Rc<CalendarSelectionStore>,
    ) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["calendar-container"])
            .build();

        // Header row: prev button, month/year label, next button
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .halign(gtk::Align::Center)
            .build();

        let prev_btn = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .css_classes(["calendar-nav-btn", "flat"])
            .tooltip_text(crate::i18n::t("calendar-prev-month"))
            .build();

        let month_label = gtk::Label::builder()
            .css_classes(["calendar-month-label"])
            .hexpand(true)
            .halign(gtk::Align::Center)
            .build();

        let next_btn = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .css_classes(["calendar-nav-btn", "flat"])
            .tooltip_text(crate::i18n::t("calendar-next-month"))
            .build();

        header.append(&prev_btn);
        header.append(&month_label);
        header.append(&next_btn);

        // Grid placeholder -- will be replaced on each rebuild
        let grid_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();

        container.append(&header);
        container.append(&grid_container);

        // Shared state for the rebuild closure
        let grid_container_ref = Rc::new(grid_container);
        let month_label_ref = Rc::new(month_label);

        // Navigation: previous month
        let selection_store_prev = selection_store.clone();
        prev_btn.connect_clicked(move |_| {
            // Read state in a scoped block so the RwLockReadGuard is dropped
            // before emit() tries to acquire a write lock on the same RwLock.
            let (year, month) = {
                let state = selection_store_prev.get_state();
                let today = Local::now().date_naive();
                state.viewed_month.unwrap_or((today.year(), today.month()))
            };
            let (new_year, new_month) = if month == 1 {
                (year - 1, 12)
            } else {
                (year, month - 1)
            };
            selection_store_prev.emit(CalendarSelectionOp::ViewMonth(new_year, new_month));
        });

        // Navigation: next month
        let selection_store_next = selection_store.clone();
        next_btn.connect_clicked(move |_| {
            let (year, month) = {
                let state = selection_store_next.get_state();
                let today = Local::now().date_naive();
                state.viewed_month.unwrap_or((today.year(), today.month()))
            };
            let (new_year, new_month) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };
            selection_store_next.emit(CalendarSelectionOp::ViewMonth(new_year, new_month));
        });

        // Rebuild closure: shared between entity and selection subscriptions.
        // Defers actual grid rebuild to a GTK idle callback to avoid heavy widget
        // churn inside synchronous store-subscriber notifications (which would
        // freeze the main thread). The `rebuild_scheduled` flag coalesces multiple
        // rapid notifications into a single rebuild.
        let rebuild = {
            let entity_store_ref = entity_store.clone();
            let selection_store_ref = selection_store.clone();
            let grid_container_ref = grid_container_ref.clone();
            let month_label_ref = month_label_ref.clone();
            let rebuild_scheduled = Rc::new(Cell::new(false));

            Rc::new(move || {
                if rebuild_scheduled.get() {
                    return;
                }
                rebuild_scheduled.set(true);
                let entity_store_idle = entity_store_ref.clone();
                let selection_store_idle = selection_store_ref.clone();
                let grid_container_idle = grid_container_ref.clone();
                let month_label_idle = month_label_ref.clone();
                let rebuild_scheduled_idle = rebuild_scheduled.clone();
                glib::idle_add_local_once(move || {
                    rebuild_scheduled_idle.set(false);
                    Self::rebuild_grid(
                        &entity_store_idle,
                        &selection_store_idle,
                        &grid_container_idle,
                        &month_label_idle,
                    );
                });
            })
        };

        // Subscribe to calendar events
        let rebuild_entity = rebuild.clone();
        entity_store.subscribe_type(entity::calendar::ENTITY_TYPE, move || {
            rebuild_entity();
        });

        // Subscribe to selection/navigation changes
        let rebuild_selection = rebuild.clone();
        selection_store.subscribe(move || {
            rebuild_selection();
        });

        // Initial render (direct, not deferred -- grid must be visible immediately)
        Self::rebuild_grid(
            entity_store,
            selection_store,
            &grid_container_ref,
            &month_label_ref,
        );

        Self {
            container,
            _entity_store: entity_store.clone(),
            _selection_store: selection_store.clone(),
        }
    }

    /// Rebuild the month grid from current state.
    fn rebuild_grid(
        entity_store: &Rc<EntityStore>,
        selection_store: &Rc<CalendarSelectionStore>,
        grid_container: &Rc<gtk::Box>,
        month_label: &Rc<gtk::Label>,
    ) {
        let today = Local::now().date_naive();
        let state = selection_store.get_state();
        let (year, month) = state.viewed_month.unwrap_or((today.year(), today.month()));
        let selected_date = state.selected_date;

        // Update month label
        let month_name = month_name(month);
        month_label.set_label(&format!("{month_name} {year}"));

        // Compute event counts per day by bucketing events into local-timezone days
        let entities: Vec<(waft_protocol::Urn, entity::calendar::CalendarEvent)> =
            entity_store.get_entities_typed(entity::calendar::ENTITY_TYPE);

        let mut event_counts: HashMap<u32, usize> = HashMap::new();
        for (_urn, event) in &entities {
            // Bucket each event into the days it spans within the viewed month
            let start_dt = chrono::DateTime::from_timestamp(event.start_time, 0)
                .map(|dt| dt.with_timezone(&Local));
            let end_dt = chrono::DateTime::from_timestamp(event.end_time, 0)
                .map(|dt| dt.with_timezone(&Local));

            if let (Some(start), Some(end)) = (start_dt, end_dt) {
                // For each day the event spans, if it falls in our viewed month, count it
                let Some(first_of_month) = NaiveDate::from_ymd_opt(year, month, 1) else {
                    continue;
                };
                let (next_year, next_month) = if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                };
                let Some(first_of_next) = NaiveDate::from_ymd_opt(next_year, next_month, 1) else {
                    continue;
                };

                let event_start_date = start.date_naive();
                let event_end_date = end.date_naive();

                // Walk each day the event touches
                let range_start = event_start_date.max(first_of_month);
                let range_end = event_end_date.min(first_of_next - chrono::Duration::days(1));

                let mut day = range_start;
                while day <= range_end {
                    if day.month() == month && day.year() == year {
                        *event_counts.entry(day.day()).or_insert(0) += 1;
                    }
                    day += chrono::Duration::days(1);
                }
            }
        }

        // Build the month grid
        let props = MonthGridProps {
            year,
            month,
            today,
            selected_date,
            event_counts,
        };

        let grid = MonthGrid::new(&props);

        // Wire day clicks to selection store
        let selection_store_click = selection_store.clone();
        grid.connect_output(move |MonthGridOutput::DayClicked(date)| {
            selection_store_click.emit(CalendarSelectionOp::SelectDate(date));
        });

        // Replace grid content
        while let Some(child) = grid_container.first_child() {
            grid_container.remove(&child);
        }
        grid_container.append(&grid.root);
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

/// Return the English month name for a 1-based month number.
fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}
