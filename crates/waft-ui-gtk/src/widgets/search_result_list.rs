//! SearchResultListWidget -- scrollable, selectable list of app results.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;

use crate::widget_base::WidgetBase;
use crate::widgets::app_result_row::{AppResultRowProps, AppResultRowWidget};

/// Output events from the search result list.
#[derive(Debug, Clone)]
pub enum SearchResultListOutput {
    /// Keyboard or programmatic selection changed.
    SelectionChanged(usize),
    /// An item was activated (clicked or Enter pressed).
    Activated(usize),
}

struct SearchResultListState {
    rows: Vec<(gtk::Button, AppResultRowWidget)>,
    selected: Option<usize>,
}

/// Scrollable list of `AppResultRowWidget` items with single-selection tracking.
///
/// Use `set_items()` to replace the full item list.
/// Use `select_next()` / `select_prev()` for keyboard navigation.
/// Use `selected_index()` to read current selection.
#[derive(Clone)]
pub struct SearchResultListWidget {
    pub scroll: gtk::ScrolledWindow,
    list_box: gtk::Box,
    state: Rc<RefCell<SearchResultListState>>,
    on_output: Callback<SearchResultListOutput>,
}

impl SearchResultListWidget {
    pub fn new() -> Self {
        let list_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["search-result-list"])
            .build();

        let scroll = gtk::ScrolledWindow::builder()
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height(400)
            .propagate_natural_height(true)
            .child(&list_box)
            .build();

        let state = Rc::new(RefCell::new(SearchResultListState {
            rows: Vec::new(),
            selected: None,
        }));

        let on_output: Callback<SearchResultListOutput> = Rc::new(RefCell::new(None));

        Self {
            scroll,
            list_box,
            state,
            on_output,
        }
    }

    /// Replace all items in the list.
    pub fn set_items(&self, items: Vec<AppResultRowProps>) {
        // Remove all existing children
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let mut state = self.state.borrow_mut();
        state.rows.clear();
        state.selected = if items.is_empty() { None } else { Some(0) };

        for (index, props) in items.into_iter().enumerate() {
            let row_widget = AppResultRowWidget::new(props);
            let btn = gtk::Button::builder()
                .css_classes(["app-result-btn"])
                .build();

            // Show selection on the first item by default
            if index == 0 {
                btn.add_css_class("selected");
            }

            btn.set_child(Some(&row_widget.widget()));
            self.list_box.append(&btn);

            // Activate on click
            let on_output = self.on_output.clone();
            btn.connect_clicked(move |_| {
                if let Some(ref cb) = *on_output.borrow() {
                    cb(SearchResultListOutput::Activated(index));
                }
            });

            state.rows.push((btn, row_widget));
        }
    }

    /// Move selection one step down.
    pub fn select_next(&self) {
        let mut state = self.state.borrow_mut();
        let count = state.rows.len();
        if count == 0 {
            return;
        }
        let next = match state.selected {
            None => 0,
            Some(i) => (i + 1).min(count - 1),
        };
        Self::apply_selection(&mut state, next);
        if let Some(ref cb) = *self.on_output.borrow() {
            cb(SearchResultListOutput::SelectionChanged(next));
        }
    }

    /// Move selection one step up.
    pub fn select_prev(&self) {
        let mut state = self.state.borrow_mut();
        let count = state.rows.len();
        if count == 0 {
            return;
        }
        let prev = match state.selected {
            None | Some(0) => 0,
            Some(i) => i - 1,
        };
        Self::apply_selection(&mut state, prev);
        if let Some(ref cb) = *self.on_output.borrow() {
            cb(SearchResultListOutput::SelectionChanged(prev));
        }
    }

    /// Current selected index, if any.
    pub fn selected_index(&self) -> Option<usize> {
        self.state.borrow().selected
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(SearchResultListOutput) + 'static>(&self, cb: F) {
        *self.on_output.borrow_mut() = Some(Box::new(cb));
    }

    fn apply_selection(state: &mut SearchResultListState, index: usize) {
        // Remove selected class from old item
        if let Some(old) = state.selected {
            if let Some((btn, _)) = state.rows.get(old) {
                btn.remove_css_class("selected");
            }
        }
        // Add selected class to new item
        if let Some((btn, _)) = state.rows.get(index) {
            btn.add_css_class("selected");
        }
        state.selected = Some(index);
    }
}

impl WidgetBase for SearchResultListWidget {
    fn widget(&self) -> gtk::Widget {
        self.scroll.clone().upcast()
    }
}
