//! Pure GTK4 Clock widget.
//!
//! Displays current date and time with automatic updates.

use std::rc::Rc;
use std::cell::RefCell;

use gtk::glib::{DateTime, GString};
use gtk::prelude::*;
use log::warn;

/// Output events from the clock widget.
#[derive(Debug, Clone)]
pub enum ClockOutput {
    Click,
}

/// Pure GTK4 clock widget - displays date and time.
pub struct ClockWidget {
    pub root: gtk::Button,
    date_label: gtk::Label,
    time_label: gtk::Label,
    on_click: Rc<RefCell<Option<Box<dyn Fn(ClockOutput)>>>>,
}

impl ClockWidget {
    /// Create a new clock widget with the given initial datetime.
    pub fn new(datetime: &DateTime) -> Self {
        let root = gtk::Button::builder()
            .css_classes(["clock-btn"])
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(["clock-container"])
            .build();

        let date_label = gtk::Label::builder()
            .label(&Self::format_date(datetime))
            .xalign(0.0)
            .css_classes(["title-3", "dim-label", "clock-date"])
            .build();

        let time_label = gtk::Label::builder()
            .label(&Self::format_time(datetime))
            .xalign(0.0)
            .css_classes(["title-1", "clock-time"])
            .build();

        content.append(&date_label);
        content.append(&time_label);
        root.set_child(Some(&content));

        let on_click: Rc<RefCell<Option<Box<dyn Fn(ClockOutput)>>>> = Rc::new(RefCell::new(None));

        // Connect click handler
        let on_click_ref = on_click.clone();
        root.connect_clicked(move |_| {
            if let Some(ref callback) = *on_click_ref.borrow() {
                callback(ClockOutput::Click);
            }
        });

        Self {
            root,
            date_label,
            time_label,
            on_click,
        }
    }

    /// Set the callback for click events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(ClockOutput) + 'static,
    {
        *self.on_click.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the displayed time.
    pub fn tick(&self, datetime: &DateTime) {
        self.date_label.set_label(&Self::format_date(datetime));
        self.time_label.set_label(&Self::format_time(datetime));
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Button {
        &self.root
    }

    fn format_datetime_str(d: &DateTime, format: &str) -> GString {
        match d.format(format) {
            Ok(s) => s,
            Err(_e) => {
                warn!("Failed to format datetime with format: {}", format);
                "".into()
            }
        }
    }

    fn format_date(d: &DateTime) -> String {
        Self::format_datetime_str(d, "%a, %d %b %Y").to_string()
    }

    fn format_time(d: &DateTime) -> String {
        Self::format_datetime_str(d, "%H:%M").to_string()
    }
}
