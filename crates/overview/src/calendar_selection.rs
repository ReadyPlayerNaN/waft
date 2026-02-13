//! Calendar selection state management.
//!
//! Shared state for the calendar month-grid and agenda components.
//! Tracks the currently selected date (for agenda filtering) and
//! the currently viewed month (for month navigation).

use chrono::NaiveDate;
use waft_core::store::{PluginStore, StoreOp, StoreState};

/// State tracking the calendar selection and viewed month.
#[derive(Clone, Debug, Default)]
pub struct CalendarSelectionState {
    /// Currently selected date, or None for default agenda behavior (today+tomorrow).
    pub selected_date: Option<NaiveDate>,
    /// Currently viewed month as (year, month), or None for current month.
    pub viewed_month: Option<(i32, u32)>,
}

impl StoreState for CalendarSelectionState {
    type Config = ();

    fn configure(&mut self, _config: &Self::Config) {
        // No configuration needed
    }
}

/// Operations on calendar selection state.
#[derive(Clone, Debug)]
pub enum CalendarSelectionOp {
    /// Select a specific date. If the same date is already selected, deselects it (toggle).
    SelectDate(NaiveDate),
    /// Clear the date selection (return to default today+tomorrow agenda).
    ClearSelection,
    /// Navigate to view a specific month.
    ViewMonth(i32, u32),
}

impl StoreOp for CalendarSelectionOp {}

/// Store for calendar selection coordination.
pub type CalendarSelectionStore = PluginStore<CalendarSelectionOp, CalendarSelectionState>;

/// Create a new CalendarSelectionStore with toggle-select logic.
pub fn create_calendar_selection_store() -> CalendarSelectionStore {
    CalendarSelectionStore::new(|state, op| match op {
        CalendarSelectionOp::SelectDate(date) => {
            if state.selected_date == Some(date) {
                // Toggle: clicking same day deselects
                state.selected_date = None;
                true
            } else {
                state.selected_date = Some(date);
                true
            }
        }
        CalendarSelectionOp::ClearSelection => {
            if state.selected_date.is_some() {
                state.selected_date = None;
                true
            } else {
                false
            }
        }
        CalendarSelectionOp::ViewMonth(year, month) => {
            let new_val = Some((year, month));
            if state.viewed_month != new_val {
                state.viewed_month = new_val;
                true
            } else {
                false
            }
        }
    })
}
