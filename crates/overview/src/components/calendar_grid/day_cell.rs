//! Day cell widget for the calendar month grid.
//!
//! A dumb presentational widget that renders a single day in the calendar.
//! Shows the day number, up to 3 event dots, and visual states for
//! today, selected, and other-month days.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

/// Input properties for a day cell.
pub struct DayCellProps {
    /// Day number (1-31).
    pub day: u32,
    /// Whether this day belongs to the currently viewed month.
    pub current_month: bool,
    /// Whether this day is today.
    pub today: bool,
    /// Whether this day is currently selected.
    pub selected: bool,
    /// Number of events on this day (dots shown for up to 3).
    pub event_count: usize,
}

/// Output events emitted by the day cell.
pub enum DayCellOutput {
    /// The day was clicked. Contains the day number.
    Clicked(u32),
}

/// A single day cell in the calendar grid.
pub struct DayCell {
    pub root: gtk::Button,
    on_output: Rc<RefCell<Option<Box<dyn Fn(DayCellOutput)>>>>,
}

impl DayCell {
    pub fn new(props: &DayCellProps) -> Self {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .spacing(2)
            .build();

        let label = gtk::Label::builder()
            .label(&props.day.to_string())
            .halign(gtk::Align::Center)
            .build();
        content.append(&label);

        // Event dots row (up to 3)
        let dots_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .halign(gtk::Align::Center)
            .spacing(2)
            .build();

        let dot_count = props.event_count.min(3);
        for _ in 0..dot_count {
            let dot = gtk::Box::builder()
                .width_request(4)
                .height_request(4)
                .css_classes(["calendar-event-dot"])
                .build();
            dots_box.append(&dot);
        }

        // Reserve space even if no dots, to keep consistent cell height
        dots_box.set_height_request(6);
        content.append(&dots_box);

        let button = gtk::Button::builder()
            .child(&content)
            .css_classes(["calendar-day-cell"])
            .build();

        // Apply state CSS classes
        if props.today {
            button.add_css_class("today");
        }
        if props.selected {
            button.add_css_class("selected");
        }
        if !props.current_month {
            button.add_css_class("other-month");
        }
        if props.event_count > 0 {
            button.add_css_class("has-events");
        }

        let on_output: Rc<RefCell<Option<Box<dyn Fn(DayCellOutput)>>>> =
            Rc::new(RefCell::new(None));

        let day = props.day;
        let on_output_ref = on_output.clone();
        button.connect_clicked(move |_| {
            if let Some(ref cb) = *on_output_ref.borrow() {
                cb(DayCellOutput::Clicked(day));
            }
        });

        Self {
            root: button,
            on_output,
        }
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(DayCellOutput) + 'static>(&self, callback: F) {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }
}
