//! Month grid widget for the calendar.
//!
//! A dumb presentational widget that renders a 7-column, 6-row grid
//! of day cells for a given month. Includes weekday headers and
//! overflow days from adjacent months (dimmed).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::{Datelike, NaiveDate};
use gtk::prelude::*;

use super::day_cell::{DayCell, DayCellOutput, DayCellProps};

/// Type alias for output callback to reduce complexity.
type OutputCallback<T> = Rc<RefCell<Option<Box<dyn Fn(T)>>>>;

/// Input properties for the month grid.
pub struct MonthGridProps {
    /// Year to display.
    pub year: i32,
    /// Month to display (1-12).
    pub month: u32,
    /// Today's date (for highlighting).
    pub today: NaiveDate,
    /// Currently selected date, if any.
    pub selected_date: Option<NaiveDate>,
    /// Number of events per day-of-month in the viewed month.
    pub event_counts: HashMap<u32, usize>,
}

/// Output events emitted by the month grid.
pub enum MonthGridOutput {
    /// A day was clicked. Contains the full NaiveDate.
    DayClicked(NaiveDate),
}

/// A month grid showing 7 columns (Mon-Sun) and 6 rows of day cells.
pub struct MonthGrid {
    pub root: gtk::Box,
    on_output: OutputCallback<MonthGridOutput>,
}

impl MonthGrid {
    pub fn new(props: &MonthGridProps) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let grid = gtk::Grid::builder()
            .column_homogeneous(true)
            .row_homogeneous(true)
            .column_spacing(0)
            .row_spacing(0)
            .build();

        // Row 0: Weekday headers (Mon-Sun, locale-independent short labels)
        let weekday_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
        for (col, label) in weekday_labels.iter().enumerate() {
            let lbl = gtk::Label::builder()
                .label(*label)
                .css_classes(["calendar-weekday-header"])
                .halign(gtk::Align::Center)
                .build();
            grid.attach(&lbl, col as i32, 0, 1, 1);
        }

        let on_output: OutputCallback<MonthGridOutput> = Rc::new(RefCell::new(None));

        // Compute first day of the month and its weekday
        let first_of_month = match NaiveDate::from_ymd_opt(props.year, props.month, 1) {
            Some(d) => d,
            None => {
                // Fallback: return empty grid
                container.append(&grid);
                return Self {
                    root: container,
                    on_output,
                };
            }
        };

        // Monday = 0, Sunday = 6 (ISO weekday - 1)
        let start_weekday = first_of_month.weekday().num_days_from_monday();

        // Days in current month
        let current_month_days = days_in_month(props.year, props.month);

        // Previous month overflow
        let (prev_year, prev_month) = if props.month == 1 {
            (props.year - 1, 12)
        } else {
            (props.year, props.month - 1)
        };
        let prev_month_days = days_in_month(prev_year, prev_month);

        // Next month
        let (next_year, next_month) = if props.month == 12 {
            (props.year + 1, 1)
        } else {
            (props.year, props.month + 1)
        };

        // Build 6 rows x 7 cols = 42 cells
        let mut cell_index = 0u32;
        for row in 1..=6 {
            for col in 0..7 {
                let (day, year, month, current_month) = if cell_index < start_weekday {
                    // Previous month overflow
                    let d = prev_month_days - (start_weekday - cell_index - 1);
                    (d, prev_year, prev_month, false)
                } else if cell_index - start_weekday < current_month_days {
                    // Current month
                    let d = cell_index - start_weekday + 1;
                    (d, props.year, props.month, true)
                } else {
                    // Next month overflow
                    let d = cell_index - start_weekday - current_month_days + 1;
                    (d, next_year, next_month, false)
                };

                let cell_date = NaiveDate::from_ymd_opt(year, month, day);

                let event_count = if current_month {
                    props.event_counts.get(&day).copied().unwrap_or(0)
                } else {
                    0
                };

                let cell_props = DayCellProps {
                    day,
                    current_month,
                    today: cell_date == Some(props.today),
                    selected: cell_date.is_some()
                        && props.selected_date.is_some()
                        && cell_date == props.selected_date,
                    event_count,
                };

                let cell = DayCell::new(&cell_props);

                let on_output_ref = on_output.clone();
                if let Some(date) = cell_date {
                    cell.connect_output(move |DayCellOutput::Clicked(_)| {
                        if let Some(ref cb) = *on_output_ref.borrow() {
                            cb(MonthGridOutput::DayClicked(date));
                        }
                    });
                }

                grid.attach(&cell.root, col, row, 1, 1);
                cell_index += 1;
            }
        }

        container.append(&grid);

        Self {
            root: container,
            on_output,
        }
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(MonthGridOutput) + 'static>(&self, callback: F) {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }
}

/// Returns the number of days in a given month.
fn days_in_month(year: i32, month: u32) -> u32 {
    // The first day of the next month minus 1 day gives the last day of this month
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    match NaiveDate::from_ymd_opt(next_year, next_month, 1) {
        Some(first_of_next) => first_of_next
            .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default())
            .num_days() as u32,
        None => 30, // fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_in_month_february_leap() {
        assert_eq!(days_in_month(2024, 2), 29);
    }

    #[test]
    fn days_in_month_february_non_leap() {
        assert_eq!(days_in_month(2025, 2), 28);
    }

    #[test]
    fn days_in_month_january() {
        assert_eq!(days_in_month(2025, 1), 31);
    }

    #[test]
    fn days_in_month_april() {
        assert_eq!(days_in_month(2025, 4), 30);
    }

    #[test]
    fn days_in_month_december() {
        assert_eq!(days_in_month(2025, 12), 31);
    }
}
