//! Toast queue manager with DND filtering and per-card countdown expiry.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use gtk::prelude::*;
use serde_json::Value;
use waft_config::ToastPosition;
use waft_protocol::Urn;
use waft_protocol::entity::notification::{Dnd, Notification, NotificationUrgency};
use waft_ui_gtk::widgets::notification_card::{NotificationCard, NotificationCardOutput};

struct ToastItem {
    urn: Urn,
    entity: Notification,
}

/// Default timeout for toasts (5 seconds).
const DEFAULT_TOAST_TTL_MS: u64 = 5000;

impl ToastItem {
    fn from_notification(urn: Urn, notification: Notification) -> Self {
        Self {
            urn,
            entity: notification,
        }
    }
}

/// Determine TTL for a notification based on urgency.
/// Returns None for Critical (no expiry).
fn toast_ttl_for(notification: &Notification) -> Option<u64> {
    match notification.urgency {
        NotificationUrgency::Critical => None,
        _ => Some(DEFAULT_TOAST_TTL_MS),
    }
}

pub struct ToastManager {
    container: gtk::Box,
    position: ToastPosition,
    active_toasts: Rc<RefCell<Vec<ToastItem>>>,
    pending_queue: RefCell<VecDeque<ToastItem>>,
    widgets: RefCell<HashMap<Urn, Rc<NotificationCard>>>,
    action_tx: std::sync::mpsc::Sender<(Urn, String, Value)>,
    dnd_active: Cell<bool>,
    window_resize_callback: Rc<dyn Fn()>,
    window_visibility_callback: Rc<dyn Fn(bool)>,
}

impl ToastManager {
    pub fn new(
        container: gtk::Box,
        action_tx: std::sync::mpsc::Sender<(Urn, String, Value)>,
        window_resize_callback: Rc<dyn Fn()>,
        window_visibility_callback: Rc<dyn Fn(bool)>,
        position: ToastPosition,
    ) -> Self {
        Self {
            container,
            position,
            active_toasts: Rc::new(RefCell::new(Vec::new())),
            pending_queue: RefCell::new(VecDeque::new()),
            widgets: RefCell::new(HashMap::new()),
            action_tx,
            dnd_active: Cell::new(false),
            window_resize_callback,
            window_visibility_callback,
        }
    }

    /// Handle notification entity update.
    pub fn handle_notification(self: &Rc<Self>, urn: Urn, notification: Notification) {
        if !should_show_toast(&notification, self.dnd_active.get()) {
            return; // Filtered by DND
        }

        let item = ToastItem::from_notification(urn, notification);

        if self.active_toasts.borrow().len() < 3 {
            self.show_toast(item);
        } else if item.entity.urgency == NotificationUrgency::Critical {
            self.bump_oldest_non_critical(item);
        } else {
            self.pending_queue.borrow_mut().push_back(item);
        }
    }

    /// Handle DND entity update.
    pub fn handle_dnd(&self, dnd: &Dnd) {
        self.dnd_active.set(dnd.active);
    }

    /// Handle entity removal (notification retracted).
    pub fn handle_entity_removed(self: &Rc<Self>, urn: &Urn) {
        self.dismiss_toast(urn);
        self.pending_queue
            .borrow_mut()
            .retain(|item| &item.urn != urn);
        self.show_next_pending();
    }

    /// Show a toast (create widget, append to container, animate).
    fn show_toast(self: &Rc<Self>, item: ToastItem) {
        let ttl = toast_ttl_for(&item.entity);
        let card = Rc::new(NotificationCard::new(
            item.urn.clone(),
            &item.entity.title,
            &item.entity.description,
            &item.entity.icon_hints,
            &item.entity.actions,
            ttl,
            Some(self.window_resize_callback.clone()),
        ));

        // Connect output callbacks
        let action_tx = self.action_tx.clone();
        let self_weak = Rc::downgrade(self);
        card.connect_output(move |output| match output {
            NotificationCardOutput::ActionClick(urn, action) => {
                if action_tx.send((urn, action, Value::Null)).is_err() {
                    log::warn!("[toast-manager] action channel closed");
                }
            }
            NotificationCardOutput::Close(urn) => {
                if action_tx
                    .send((urn, "dismiss".into(), Value::Null))
                    .is_err()
                {
                    log::warn!("[toast-manager] action channel closed");
                }
            }
            NotificationCardOutput::TimedOut(urn) => {
                if action_tx
                    .send((urn.clone(), "dismiss".into(), Value::Null))
                    .is_err()
                {
                    log::warn!("[toast-manager] action channel closed on timeout");
                }
                if let Some(mgr) = self_weak.upgrade() {
                    mgr.dismiss_toast(&urn);
                    mgr.show_next_pending();
                }
            }
        });

        if self.position.newest_on_top() {
            self.container.prepend(card.widget());
        } else {
            self.container.append(card.widget());
        }
        card.show();

        self.widgets.borrow_mut().insert(item.urn.clone(), card);
        self.active_toasts.borrow_mut().push(item);
        (self.window_visibility_callback)(true);
    }

    /// Dismiss toast with slide-out animation.
    ///
    /// Removes the toast from tracking immediately. The widget slides out via
    /// the revealer and is removed from the container when the animation
    /// completes. Window visibility is updated after the slide-out finishes
    /// to avoid a visual cut.
    fn dismiss_toast(&self, urn: &Urn) {
        if let Some(card) = self.widgets.borrow_mut().remove(urn) {
            self.active_toasts
                .borrow_mut()
                .retain(|item| &item.urn != urn);

            // Defer container removal and visibility update until the
            // revealer animation completes (200ms slide-out).
            let container = self.container.clone();
            let visibility_cb = self.window_visibility_callback.clone();
            let active_toasts = self.active_toasts.clone();
            let card_root = card.widget().clone();

            // Use a one-shot flag to avoid firing on the initial reveal_child(true) call.
            let handled = Rc::new(Cell::new(false));
            card.revealer().connect_child_revealed_notify(move |rev| {
                if !rev.is_child_revealed() && !handled.get() {
                    handled.set(true);
                    container.remove(&card_root);
                    let has_toasts = !active_toasts.borrow().is_empty();
                    (visibility_cb)(has_toasts);
                }
            });

            card.hide_and_remove();
        }
    }

    /// Bump the oldest non-critical toast to make room for a critical one.
    fn bump_oldest_non_critical(self: &Rc<Self>, critical_item: ToastItem) {
        let mut active = self.active_toasts.borrow_mut();

        // Find the oldest non-critical toast
        if let Some(index) = active
            .iter()
            .position(|item| item.entity.urgency != NotificationUrgency::Critical)
        {
            let bumped = active.remove(index);
            drop(active);

            // Dismiss the bumped toast
            self.dismiss_toast(&bumped.urn);

            // Move bumped toast to pending queue
            self.pending_queue.borrow_mut().push_front(bumped);

            // Show critical toast
            self.show_toast(critical_item);
        } else {
            // All active toasts are critical, queue the new one
            self.pending_queue.borrow_mut().push_back(critical_item);
        }
    }

    /// Show next pending toast if queue has items and space is available.
    fn show_next_pending(self: &Rc<Self>) {
        if self.active_toasts.borrow().len() < 3
            && let Some(item) = self.pending_queue.borrow_mut().pop_front()
        {
            self.show_toast(item);
        }
    }

}

/// Filter logic: suppress toast if the notification is flagged or DND is active.
fn should_show_toast(notification: &Notification, dnd_active: bool) -> bool {
    if notification.suppress_toast {
        return false;
    }
    if !dnd_active {
        return true;
    }
    notification.urgency == NotificationUrgency::Critical
}
