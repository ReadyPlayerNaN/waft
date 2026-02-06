//! Pure GTK4 Toast List widget.
//!
//! Displays a list of toast notifications with automatic state synchronization.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use super::toast_widget::ToastWidget;
use crate::common::Callback;
use crate::features::notifications::store::{ItemLifecycle, NotificationOp, NotificationStore};
use crate::features::notifications::types::{NotificationAction, NotificationIcon};

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
    pub toast_ttl: Option<u64>,
}

/// Pure GTK4 toast list widget.
pub struct ToastListWidget {
    pub root: gtk::Box,
    container: gtk::Box,
    widgets: Rc<RefCell<HashMap<u64, ToastWidget>>>,
    on_output: Callback<ToastListOutput>,
    hover_count: Rc<RefCell<u32>>,
    store: Rc<NotificationStore>,
}

impl ToastListWidget {
    /// Create a new toast list widget with the given store.
    pub fn new(store: Rc<NotificationStore>) -> Self {
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
        let on_output: Callback<ToastListOutput> = Rc::new(RefCell::new(None));
        let hover_count: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        let widget = Self {
            root,
            container,
            widgets,
            on_output,
            hover_count,
            store,
        };

        // Subscribe to store for state changes
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
        let hover_count = self.hover_count.clone();
        let store = self.store.clone();
        let store_for_hover = self.store.clone();

        // Subscribe to store - the callback runs on the main thread via glib::spawn_future_local
        self.store.subscribe(move || {
            let state = store.get_state();

            // Get all toast IDs (including hidden ones) to know which toasts still exist
            let all_toast_ids: std::collections::HashSet<u64> =
                state.toasts.keys().copied().collect();

            // Get hover_paused state to check if new toasts should start paused
            let hover_paused = state.hover_paused;

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
                    toast_ttl: n.toast_ttl,
                })
                .collect();

            Self::handle_toasts_changed(
                &toasts,
                &all_toast_ids,
                &widgets,
                &container,
                &on_output,
                &hover_count,
                hover_paused,
                &store_for_hover,
            );
        });
    }

    fn handle_toasts_changed(
        toasts: &Vec<ToastStateData>,
        all_toast_ids: &std::collections::HashSet<u64>,
        widgets: &Rc<RefCell<HashMap<u64, ToastWidget>>>,
        container: &gtk::Box,
        on_output: &Callback<ToastListOutput>,
        hover_count: &Rc<RefCell<u32>>,
        hover_paused: bool,
        store: &Rc<NotificationStore>,
    ) {
        log::debug!("[toast_list] ToastsChanged received count={}", toasts.len());

        // Collect IDs of toasts that should be visible
        let visible_ids: std::collections::HashSet<u64> = toasts.iter().map(|t| t.id).collect();

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

        // Handle widgets that are not currently visible
        {
            let widgets_ref = widgets.borrow();
            for (id, widget) in widgets_ref.iter() {
                if !visible_ids.contains(id) && !widget.is_hidden() {
                    if all_toast_ids.contains(id) {
                        // Toast still exists but is hidden (slot limited) - hide but keep in container
                        log::debug!("[toast_list] hiding widget id={} (slot limited)", id);
                        widget.hide();
                    } else {
                        // Toast is gone from state (dismissed/expired) - hide and remove
                        log::debug!("[toast_list] hiding and removing widget id={}", id);
                        widget.hide_and_remove();
                    }
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
                if let std::collections::hash_map::Entry::Vacant(e) = widgets_ref.entry(toast.id) {
                    let id = toast.id;
                    let on_output_clone = on_output.clone();
                    let on_output_action = on_output.clone();
                    let hover_count_clone = hover_count.clone();
                    let widgets_for_hover = widgets.clone();
                    let store_for_hover = store.clone();

                    let widget = ToastWidget::new(
                        toast.id,
                        &toast.title,
                        &toast.description,
                        toast.icon_hints.clone(),
                        toast.actions.clone(),
                        toast.toast_ttl,
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
                        move |is_enter| {
                            Self::handle_hover_change(
                                is_enter,
                                &hover_count_clone,
                                &widgets_for_hover,
                                &store_for_hover,
                            );
                        },
                    );

                    // Show container and add widget (prepend for newest on top)
                    container.set_visible(true);
                    container.prepend(&widget.root);

                    // Animate in
                    widget.show();

                    // If currently hovering, pause the new widget's countdown
                    if hover_paused {
                        widget.pause_countdown();
                    }

                    e.insert(widget);
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

    /// Handle hover state changes with reference counting.
    /// When the first toast is hovered, pause all countdowns and emit ToastHoverEnter.
    /// When the last toast is un-hovered, resume all countdowns and emit ToastHoverLeave.
    fn handle_hover_change(
        is_enter: bool,
        hover_count: &Rc<RefCell<u32>>,
        widgets: &Rc<RefCell<HashMap<u64, ToastWidget>>>,
        store: &Rc<NotificationStore>,
    ) {
        let mut count = hover_count.borrow_mut();
        if is_enter {
            *count += 1;
            if *count == 1 {
                // First hover - pause all countdowns
                store.emit(NotificationOp::ToastHoverEnter);
                for w in widgets.borrow().values() {
                    w.pause_countdown();
                }
            }
        } else {
            *count = count.saturating_sub(1);
            if *count == 0 {
                // Last hover left - resume all countdowns
                store.emit(NotificationOp::ToastHoverLeave);
                for w in widgets.borrow().values() {
                    w.resume_countdown();
                }
            }
        }
    }
}
