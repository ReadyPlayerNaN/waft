//! Pure GTK4 Toast List widget.
//!
//! Displays a list of toast notifications with automatic state synchronization.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use crate::features::notifications::store::{ItemLifecycle, STORE};
use crate::features::notifications::types::{NotificationAction, NotificationIcon};
use super::toast_widget::ToastWidget;

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
    pub icon_hints: Vec<NotificationIcon>,
    pub actions: Vec<NotificationAction>,
    #[allow(dead_code)]
    pub ttl: Option<u64>,
}

/// Pure GTK4 toast list widget.
pub struct ToastListWidget {
    pub root: gtk::Box,
    container: gtk::Box,
    widgets: Rc<RefCell<HashMap<u64, ToastWidget>>>,
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

        // Start hidden - will be shown when toasts are added
        container.set_visible(false);

        let widgets: Rc<RefCell<HashMap<u64, ToastWidget>>> = Rc::new(RefCell::new(HashMap::new()));
        let on_output: Rc<RefCell<Option<Box<dyn Fn(ToastListOutput)>>>> =
            Rc::new(RefCell::new(None));

        let widget = Self {
            root,
            container,
            widgets,
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
                    icon_hints: n.icon_hints.clone(),
                    actions: n.actions.clone(),
                    ttl: n.ttl,
                })
                .collect();

            Self::handle_toasts_changed(
                &toasts,
                &widgets,
                &container,
                &on_output,
            );
        });
    }

    fn handle_toasts_changed(
        toasts: &Vec<ToastStateData>,
        widgets: &Rc<RefCell<HashMap<u64, ToastWidget>>>,
        container: &gtk::Box,
        on_output: &Rc<RefCell<Option<Box<dyn Fn(ToastListOutput)>>>>,
    ) {
        log::debug!("[toast_list] ToastsChanged received count={}", toasts.len());

        // Collect IDs of toasts that should exist
        let known_ids: Vec<u64> = toasts.iter().map(|t| t.id).collect();

        // Clean up orphaned widgets (those that have removed themselves from the container)
        {
            let mut widgets_ref = widgets.borrow_mut();
            let orphaned: Vec<u64> = widgets_ref
                .iter()
                .filter(|(_, w)| w.root.parent().is_none())
                .map(|(id, _)| *id)
                .collect();

            for id in orphaned {
                widgets_ref.remove(&id);
                log::debug!("[toast_list] cleaned up orphaned widget id={}", id);
            }
        }

        // Hide widgets that are no longer in state (they will self-remove when animation completes)
        {
            let widgets_ref = widgets.borrow();
            for (id, widget) in widgets_ref.iter() {
                if !known_ids.contains(id) && !widget.is_hidden() {
                    log::debug!("[toast_list] hiding widget id={}", id);
                    widget.hide();
                }
            }
        }

        // Update container visibility based on whether we have any visible widgets
        {
            let widgets_ref = widgets.borrow();
            let has_visible = widgets_ref.values().any(|w| w.root.parent().is_some());
            container.set_visible(has_visible || !toasts.is_empty());
        }

        // Add new widgets
        {
            let mut widgets_ref = widgets.borrow_mut();
            for toast in toasts {
                if !widgets_ref.contains_key(&toast.id) {
                    let id = toast.id;
                    let on_output_clone = on_output.clone();
                    let on_output_action = on_output.clone();

                    let widget = ToastWidget::new(
                        toast.id,
                        &toast.title,
                        &toast.description,
                        toast.icon_hints.clone(),
                        toast.actions.clone(),
                        move |close_id| {
                            if let Some(ref callback) = *on_output_clone.borrow() {
                                callback(ToastListOutput::CardClose(close_id));
                            }
                        },
                        move |action_id, action_key| {
                            if let Some(ref callback) = *on_output_action.borrow() {
                                callback(ToastListOutput::ActionClick(action_id, action_key));
                            }
                        },
                    );

                    // Show container and add widget (prepend for newest on top)
                    container.set_visible(true);
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
