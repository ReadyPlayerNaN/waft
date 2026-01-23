//! Pure GTK4 Toast List widget.
//!
//! Displays a list of toast notifications with automatic state synchronization.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

use crate::features::notifications::store::{ItemLifecycle, STORE};
use super::toast_widget::ToastWidget;

/// Delay before removing widgets to let GTK finish any internal processing
const REMOVAL_DELAY: Duration = Duration::from_millis(300);

/// Output events from the toast list.
#[derive(Debug, Clone)]
pub enum ToastListOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    CardTimedOut(u64),
}

/// Lightweight toast data for state updates
#[derive(Debug, Clone)]
pub struct ToastStateData {
    pub id: u64,
    pub lifecycle: ItemLifecycle,
    pub title: Arc<str>,
    pub description: Arc<str>,
    #[allow(dead_code)]
    pub ttl: Option<u64>,
}

/// Pure GTK4 toast list widget.
pub struct ToastListWidget {
    pub root: gtk::Box,
    container: gtk::Box,
    widgets: Rc<RefCell<HashMap<u64, ToastWidget>>>,
    pending_removal: Rc<RefCell<HashMap<u64, Instant>>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(ToastListOutput)>>>>,
}

impl ToastListWidget {
    /// Create a new toast list widget.
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        root.append(&container);

        // Spacer at bottom
        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&spacer);

        let widgets: Rc<RefCell<HashMap<u64, ToastWidget>>> = Rc::new(RefCell::new(HashMap::new()));
        let pending_removal: Rc<RefCell<HashMap<u64, Instant>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let on_output: Rc<RefCell<Option<Box<dyn Fn(ToastListOutput)>>>> =
            Rc::new(RefCell::new(None));

        let widget = Self {
            root,
            container,
            widgets,
            pending_removal,
            on_output,
        };

        // Subscribe to STORE for state changes
        widget.setup_subscription();

        widget
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(ToastListOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn setup_subscription(&self) {
        let widgets = self.widgets.clone();
        let pending_removal = self.pending_removal.clone();
        let container = self.container.clone();
        let on_output = self.on_output.clone();

        // Subscribe to STORE - the callback runs on the main thread via glib::spawn_future_local
        STORE.subscribe(move || {
            let state = STORE.get_state();
            let toasts: Vec<ToastStateData> = state
                .get_toasts()
                .into_iter()
                .filter(|(_, l)| {
                    !matches!(
                        l,
                        ItemLifecycle::Dismissed | ItemLifecycle::Hidden | ItemLifecycle::Retracted
                    )
                })
                .map(|(n, l)| ToastStateData {
                    id: n.id,
                    lifecycle: l.clone(),
                    title: n.title.clone(),
                    description: n.description.clone(),
                    ttl: n.ttl,
                })
                .collect();

            Self::handle_toasts_changed(
                &toasts,
                &widgets,
                &pending_removal,
                &container,
                &on_output,
            );
        });
    }

    fn handle_toasts_changed(
        toasts: &Vec<ToastStateData>,
        widgets: &Rc<RefCell<HashMap<u64, ToastWidget>>>,
        pending_removal: &Rc<RefCell<HashMap<u64, Instant>>>,
        container: &gtk::Box,
        on_output: &Rc<RefCell<Option<Box<dyn Fn(ToastListOutput)>>>>,
    ) {
        log::debug!("[toast_list] ToastsChanged received count={}", toasts.len());
        let now = Instant::now();

        // Collect IDs of toasts that should exist
        let known_ids: Vec<u64> = toasts.iter().map(|t| t.id).collect();

        // Mark widgets for removal (exist in UI but not in state)
        {
            let widgets_ref = widgets.borrow();
            let mut pending_ref = pending_removal.borrow_mut();
            for id in widgets_ref.keys() {
                if !known_ids.contains(id) {
                    pending_ref.entry(*id).or_insert(now);
                }
            }
        }

        // Cancel pending removal if widget reappears in state
        {
            let mut pending_ref = pending_removal.borrow_mut();
            for id in &known_ids {
                pending_ref.remove(id);
            }
        }

        // Remove widgets that have been pending long enough
        let ready_for_removal: Vec<u64> = {
            let pending_ref = pending_removal.borrow();
            pending_ref
                .iter()
                .filter(|(_, marked_at)| now.duration_since(**marked_at) > REMOVAL_DELAY)
                .map(|(id, _)| *id)
                .collect()
        };

        {
            let mut widgets_ref = widgets.borrow_mut();
            let mut pending_ref = pending_removal.borrow_mut();
            for id in &ready_for_removal {
                if let Some(widget) = widgets_ref.remove(id) {
                    log::debug!("[toast_list] removing widget id={}", id);
                    widget.prepare_removal();
                    container.remove(&widget.root);
                }
                pending_ref.remove(id);
            }
        }

        log::debug!(
            "[toast_list] removed {} widgets, {} pending",
            ready_for_removal.len(),
            pending_removal.borrow().len()
        );

        // Add new widgets
        {
            let mut widgets_ref = widgets.borrow_mut();
            for toast in toasts {
                if !widgets_ref.contains_key(&toast.id) {
                    let id = toast.id;
                    let on_output_clone = on_output.clone();

                    let widget = ToastWidget::new(
                        toast.id,
                        &toast.title,
                        &toast.description,
                        move |close_id| {
                            if let Some(ref callback) = *on_output_clone.borrow() {
                                callback(ToastListOutput::CardClose(close_id));
                            }
                        },
                    );

                    // Add to container (prepend for newest on top)
                    container.prepend(&widget.root);

                    // Animate in
                    widget.show();

                    widgets_ref.insert(toast.id, widget);
                    log::debug!("[toast_list] added widget id={}", id);
                }
            }
        }

        // Update visibility of existing widgets based on lifecycle
        {
            let widgets_ref = widgets.borrow();
            for toast in toasts {
                if let Some(widget) = widgets_ref.get(&toast.id) {
                    let should_show = !toast.lifecycle.is_hidden();
                    if should_show && widget.is_hidden() {
                        widget.show();
                    } else if !should_show && !widget.is_hidden() {
                        widget.hide();
                    }
                }
            }
        }

        log::debug!(
            "[toast_list] ToastsChanged done, {} widgets total",
            widgets.borrow().len()
        );
    }
}

impl Default for ToastListWidget {
    fn default() -> Self {
        Self::new()
    }
}
