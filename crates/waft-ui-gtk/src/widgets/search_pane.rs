//! SearchPaneWidget -- composite search bar + result list.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;

use crate::vdom::Component;
use crate::widget_base::WidgetBase;
use crate::widgets::app_result_row::AppResultRowProps;
use crate::widgets::empty_search_state::{EmptySearchStateProps, EmptySearchStateWidget};
use crate::widgets::search_bar::{SearchBarOutput, SearchBarWidget};
use crate::widgets::search_result_list::{
    SearchResultListOutput, SearchResultListProps, SearchResultListWidget,
};

/// Output events from the search pane.
#[derive(Debug, Clone)]
pub enum SearchPaneOutput {
    /// Query text changed.
    QueryChanged(String),
    /// Enter pressed with no explicit result selection.
    QueryActivated,
    /// Keyboard selection changed (index in result list).
    ResultSelected(usize),
    /// An item was activated (clicked or Enter on selection).
    ResultActivated(usize),
    /// Escape pressed — the pane requests dismissal.
    Stopped,
}

/// Composite search pane: search bar on top, results list or empty state below.
#[derive(Clone)]
pub struct SearchPaneWidget {
    root: gtk::Box,
    pub search_bar: SearchBarWidget,
    result_list: Rc<SearchResultListWidget>,
    empty_state: EmptySearchStateWidget,
    stack: gtk::Stack,
    on_output: Callback<SearchPaneOutput>,
    selected: Rc<Cell<usize>>,
    items: Rc<RefCell<Vec<AppResultRowProps>>>,
}

impl SearchPaneWidget {
    pub fn new(placeholder: &str) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["search-pane"])
            .build();

        let search_bar = SearchBarWidget::new(placeholder);
        root.append(&search_bar.widget());

        let result_list = Rc::new(SearchResultListWidget::build(&SearchResultListProps {
            items: Vec::new(),
            selected: 0,
        }));

        let empty_state = EmptySearchStateWidget::new(EmptySearchStateProps {
            query: String::new(),
        });

        // Loading child: spinner + label centred in a box
        let loading_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .css_classes(["launcher-loading-state"])
            .build();
        let spinner = gtk::Spinner::new();
        spinner.start();
        let loading_label = gtk::Label::builder()
            .label("Loading applications\u{2026}")
            .build();
        loading_box.append(&spinner);
        loading_box.append(&loading_label);

        // Wrap the result list's ScrolledWindow in the stack
        let scroll = gtk::ScrolledWindow::builder()
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height(400)
            .propagate_natural_height(true)
            .child(&result_list.widget())
            .build();

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::None)
            .build();
        stack.add_named(&scroll, Some("results"));
        stack.add_named(&empty_state.widget(), Some("empty"));
        stack.add_named(&loading_box, Some("loading"));
        stack.set_visible_child_name("results");
        root.append(&stack);

        let on_output: Callback<SearchPaneOutput> = Rc::new(RefCell::new(None));
        let selected = Rc::new(Cell::new(0usize));
        let items: Rc<RefCell<Vec<AppResultRowProps>>> = Rc::new(RefCell::new(Vec::new()));

        // Wire search bar output
        let on_output_bar = on_output.clone();
        search_bar.connect_output(move |event| {
            if let Some(ref cb) = *on_output_bar.borrow() {
                match event {
                    SearchBarOutput::Changed(text) => cb(SearchPaneOutput::QueryChanged(text)),
                    SearchBarOutput::Activated => cb(SearchPaneOutput::QueryActivated),
                    SearchBarOutput::Stopped => cb(SearchPaneOutput::Stopped),
                }
            }
        });

        // Wire result list output
        let on_output_list = on_output.clone();
        result_list.connect_output(move |event| {
            if let Some(ref cb) = *on_output_list.borrow() {
                match event {
                    SearchResultListOutput::Activated(i) => {
                        cb(SearchPaneOutput::ResultActivated(i))
                    }
                }
            }
        });

        Self {
            root,
            search_bar,
            result_list,
            empty_state,
            stack,
            on_output,
            selected,
            items,
        }
    }

    /// Show or hide the loading spinner. When `true`, the loading child is shown.
    /// The first call to `set_results` implicitly clears loading state.
    pub fn set_loading(&self, loading: bool) {
        if loading {
            self.stack.set_visible_child_name("loading");
        } else if self.stack.visible_child_name().as_deref() == Some("loading") {
            self.stack.set_visible_child_name("results");
        }
    }

    /// Update displayed results. Pass empty vec to show empty state.
    /// Implicitly clears any active loading state.
    pub fn set_results(&self, items: Vec<AppResultRowProps>, query: &str) {
        if items.is_empty() && !query.is_empty() {
            self.empty_state.set_query(query);
            self.stack.set_visible_child_name("empty");
        } else {
            self.selected.set(0);
            *self.items.borrow_mut() = items.clone();
            self.result_list.update(&SearchResultListProps {
                items,
                selected: 0,
            });
            self.stack.set_visible_child_name("results");
        }
    }

    /// Move keyboard selection down.
    pub fn select_next(&self) {
        let items = self.items.borrow();
        let count = items.len();
        if count == 0 {
            return;
        }
        let next = (self.selected.get() + 1).min(count - 1);
        self.selected.set(next);
        self.result_list.update(&SearchResultListProps {
            items: items.clone(),
            selected: next,
        });
        if let Some(ref cb) = *self.on_output.borrow() {
            cb(SearchPaneOutput::ResultSelected(next));
        }
    }

    /// Move keyboard selection up.
    pub fn select_prev(&self) {
        let items = self.items.borrow();
        let count = items.len();
        if count == 0 {
            return;
        }
        let prev = self.selected.get().saturating_sub(1);
        self.selected.set(prev);
        self.result_list.update(&SearchResultListProps {
            items: items.clone(),
            selected: prev,
        });
        if let Some(ref cb) = *self.on_output.borrow() {
            cb(SearchPaneOutput::ResultSelected(prev));
        }
    }

    /// Currently selected index in the result list.
    pub fn selected_index(&self) -> Option<usize> {
        let count = self.items.borrow().len();
        if count == 0 {
            None
        } else {
            Some(self.selected.get())
        }
    }

    /// Register output callback.
    pub fn connect_output<F: Fn(SearchPaneOutput) + 'static>(&self, cb: F) {
        *self.on_output.borrow_mut() = Some(Box::new(cb));
    }

    /// Grab focus into the search bar.
    pub fn grab_focus(&self) {
        self.search_bar.grab_focus();
    }
}

impl WidgetBase for SearchPaneWidget {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
