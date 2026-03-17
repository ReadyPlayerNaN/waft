//! Search results list widget.
//!
//! Dumb widget that displays a list of search results as `adw::ActionRow`s.
//! Emits `SearchResultsOutput::Selected` when a result is activated.

use std::cell::RefCell;
use std::collections::HashMap;
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
        /// Section title for widget lookup after page construction.
        section_title: Option<String>,
        /// Input title for widget lookup after page construction.
        input_title: Option<String>,
    },
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(SearchResultsOutput)>>>>;

/// Per-row metadata for section/input identification.
type RowMeta = Rc<RefCell<HashMap<i32, (Option<String>, Option<String>)>>>;

/// Search results list widget.
pub struct SearchResults {
    pub root: gtk::ListBox,
    output_cb: OutputCallback,
    /// Per-row metadata (section_title, input_title) keyed by row index.
    row_meta: RowMeta,
}

impl SearchResults {
    pub fn new() -> Self {
        let root = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let row_meta: RowMeta = Rc::new(RefCell::new(HashMap::new()));

        let cb = output_cb.clone();
        let meta_ref = row_meta.clone();
        root.connect_row_activated(move |_, row| {
            let page_id = row.widget_name().to_string();
            if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
                let page_title = action_row.subtitle().map(|s| s.to_string()).unwrap_or_default();
                let (section_title, input_title) = meta_ref
                    .borrow()
                    .get(&row.index())
                    .cloned()
                    .unwrap_or((None, None));
                if let Some(ref callback) = *cb.borrow() {
                    callback(SearchResultsOutput::Selected {
                        page_id,
                        page_title,
                        section_title,
                        input_title,
                    });
                }
            }
        });

        Self { root, output_cb, row_meta }
    }

    /// Update the results list with new search entries.
    pub fn update(&self, entries: &[SearchResultRef]) {
        // Remove all existing rows
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        let mut meta = self.row_meta.borrow_mut();
        meta.clear();

        for (i, entry) in entries.iter().enumerate() {
            let row = adw::ActionRow::builder()
                .title(&entry.breadcrumb)
                .subtitle(&entry.page_title)
                .activatable(true)
                .build();
            row.set_widget_name(entry.page_id);
            self.root.append(&row);
            meta.insert(i as i32, (entry.section_title.clone(), entry.input_title.clone()));
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

    /// Move focus to the first result row (or selected row if one exists).
    pub fn focus_first(&self) {
        if let Some(selected) = self.root.selected_row() {
            selected.grab_focus();
        } else if let Some(first) = self.root.row_at_index(0) {
            self.root.select_row(Some(&first));
            first.grab_focus();
        }
    }

    /// Activate the currently selected result row.
    pub fn activate_selected(&self) {
        if let Some(selected) = self.root.selected_row() {
            selected.activate();
        } else if let Some(first) = self.root.row_at_index(0) {
            first.activate();
        }
    }

    /// Clear all search results.
    pub fn clear(&self) {
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        self.row_meta.borrow_mut().clear();
    }
}

/// Lightweight reference to a search result for the results widget.
pub struct SearchResultRef {
    pub page_id: &'static str,
    pub page_title: String,
    pub breadcrumb: String,
    pub section_title: Option<String>,
    pub input_title: Option<String>,
}
