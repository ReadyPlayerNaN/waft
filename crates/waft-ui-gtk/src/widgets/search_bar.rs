//! SearchBarWidget -- search entry with output callbacks.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;

use crate::widget_base::WidgetBase;

/// Output events from the search bar.
#[derive(Debug, Clone)]
pub enum SearchBarOutput {
    /// Text in the entry changed.
    Changed(String),
    /// Enter pressed with no result explicitly selected (activate the top result).
    Activated,
    /// Escape pressed inside the search entry (stop-search signal).
    Stopped,
}

/// Wrapper around `gtk::SearchEntry`.
///
/// Applies project CSS classes and exposes output callbacks.
#[derive(Clone)]
pub struct SearchBarWidget {
    pub entry: gtk::SearchEntry,
    on_output: Callback<SearchBarOutput>,
}

impl SearchBarWidget {
    pub fn new(placeholder: &str) -> Self {
        let entry = gtk::SearchEntry::builder()
            .placeholder_text(placeholder)
            .hexpand(true)
            .css_classes(["launcher-search-bar"])
            .build();

        let on_output: Callback<SearchBarOutput> = Rc::new(RefCell::new(None));

        // Wire up changed signal
        let on_output_changed = on_output.clone();
        entry.connect_search_changed(move |e| {
            if let Some(ref cb) = *on_output_changed.borrow() {
                cb(SearchBarOutput::Changed(e.text().to_string()));
            }
        });

        // Wire up activate signal (Enter key)
        let on_output_activate = on_output.clone();
        entry.connect_activate(move |_| {
            if let Some(ref cb) = *on_output_activate.borrow() {
                cb(SearchBarOutput::Activated);
            }
        });

        // Wire up stop-search signal (Escape key inside the entry)
        let on_output_stop = on_output.clone();
        entry.connect_stop_search(move |_| {
            if let Some(ref cb) = *on_output_stop.borrow() {
                cb(SearchBarOutput::Stopped);
            }
        });

        Self { entry, on_output }
    }

    /// Register a callback for all output events.
    pub fn connect_output<F: Fn(SearchBarOutput) + 'static>(&self, cb: F) {
        *self.on_output.borrow_mut() = Some(Box::new(cb));
    }

    /// Current text value.
    pub fn text(&self) -> String {
        self.entry.text().to_string()
    }

    /// Clear the search entry text.
    pub fn clear(&self) {
        self.entry.set_text("");
    }

    /// Grab keyboard focus.
    pub fn grab_focus(&self) {
        self.entry.grab_focus();
    }
}

impl WidgetBase for SearchBarWidget {
    fn widget(&self) -> gtk::Widget {
        self.entry.clone().upcast()
    }
}
