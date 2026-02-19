//! Search results list widget.
//!
//! Dumb widget that displays a list of search results as `adw::ActionRow`s.
//! Emits `SearchResultsOutput::Selected` when a result is activated.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

/// Output events from the search results widget.
pub enum SearchResultsOutput {
    /// A search result was selected.
    Selected {
        /// Stable page ID for stack routing.
        page_id: String,
        /// Human-readable page title for the header.
        page_title: String,
    },
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(SearchResultsOutput)>>>>;

/// Search results list widget.
pub struct SearchResults {
    pub root: gtk::ListBox,
    output_cb: OutputCallback,
}

impl SearchResults {
    pub fn new() -> Self {
        let root = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        let cb = output_cb.clone();
        root.connect_row_activated(move |_, row| {
            let page_id = row.widget_name().to_string();
            // Retrieve stored data from the row
            if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
                let page_title = action_row.subtitle().map(|s| s.to_string()).unwrap_or_default();
                // target_widget is stored as unsafe data on the row via the row index
                // We use widget_name for page_id, subtitle for page_title
                if let Some(ref callback) = *cb.borrow() {
                    callback(SearchResultsOutput::Selected {
                        page_id,
                        page_title,
                    });
                }
            }
        });

        Self { root, output_cb }
    }

    /// Update the results list with new search entries.
    pub fn update(&self, entries: &[SearchResultRef]) {
        // Remove all existing rows
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }

        for entry in entries {
            let row = adw::ActionRow::builder()
                .title(&entry.breadcrumb)
                .subtitle(&entry.page_title)
                .activatable(true)
                .build();
            row.set_widget_name(entry.page_id);
            self.root.append(&row);
        }

        // Select first row if present
        if let Some(first_row) = self.root.row_at_index(0) {
            self.root.select_row(Some(&first_row));
        }
    }

    /// Register a callback for search result output events.
    pub fn connect_output<F: Fn(SearchResultsOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Clear all search results.
    pub fn clear(&self) {
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
    }
}

/// Lightweight reference to a search result for the results widget.
pub struct SearchResultRef {
    pub page_id: &'static str,
    pub page_title: String,
    pub breadcrumb: String,
}
