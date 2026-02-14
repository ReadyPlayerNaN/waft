//! Toast queue manager with DND filtering and TTL expiry.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use gtk::prelude::*;
use serde_json::Value;
use waft_protocol::entity::notification::{Dnd, Notification, NotificationUrgency};
use waft_protocol::Urn;
use waft_ui_gtk::widgets::notification_card::{NotificationCard, NotificationCardOutput};

struct ToastItem {
    urn: Urn,
    entity: Notification,
    expires_at: Option<SystemTime>,
}

/// Default timeout for toasts (5 seconds).
const DEFAULT_TOAST_TTL_MS: u64 = 5000;

impl ToastItem {
    fn from_notification(urn: Urn, notification: Notification) -> Self {
        // Use default TTL since the notification entity doesn't include TTL
        // (deprioritization is handled by the notification plugin before sending)
        let expires_at = SystemTime::now()
            .checked_add(Duration::from_millis(DEFAULT_TOAST_TTL_MS));

        Self {
            urn,
            entity: notification,
            expires_at,
        }
    }
}

pub struct ToastManager {
    container: gtk::Box,
    active_toasts: RefCell<Vec<ToastItem>>,
    pending_queue: RefCell<VecDeque<ToastItem>>,
    widgets: RefCell<HashMap<Urn, Rc<NotificationCard>>>,
    action_tx: std::sync::mpsc::Sender<(Urn, String, Value)>,
    dnd_active: Cell<bool>,
    window_resize_callback: Rc<dyn Fn()>,
}

impl ToastManager {
    pub fn new(
        container: gtk::Box,
        action_tx: std::sync::mpsc::Sender<(Urn, String, Value)>,
        window_resize_callback: Rc<dyn Fn()>,
    ) -> Self {
        Self {
            container,
            active_toasts: RefCell::new(Vec::new()),
            pending_queue: RefCell::new(VecDeque::new()),
            widgets: RefCell::new(HashMap::new()),
            action_tx,
            dnd_active: Cell::new(false),
            window_resize_callback,
        }
    }

    /// Handle notification entity update.
    pub fn handle_notification(&self, urn: Urn, notification: Notification) {
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
    pub fn handle_entity_removed(&self, urn: &Urn) {
        self.dismiss_toast(urn);
        self.pending_queue.borrow_mut().retain(|item| &item.urn != urn);
        self.show_next_pending();
    }

    /// Show a toast (create widget, append to container, animate).
    fn show_toast(&self, item: ToastItem) {
        let card = Rc::new(NotificationCard::new(
            item.urn.clone(),
            &item.entity.title,
            &item.entity.description,
            &item.entity.icon_hints,
            &item.entity.actions,
            Some(self.window_resize_callback.clone()),
        ));

        // Connect output callbacks
        let action_tx = self.action_tx.clone();
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
        });

        self.container.append(card.widget());
        card.show();

        self.widgets
            .borrow_mut()
            .insert(item.urn.clone(), card);
        self.active_toasts.borrow_mut().push(item);
        (self.window_resize_callback)();
    }

    /// Dismiss toast (hide animation, remove from active).
    fn dismiss_toast(&self, urn: &Urn) {
        if let Some(card) = self.widgets.borrow_mut().remove(urn) {
            self.container.remove(card.widget());
            self.active_toasts
                .borrow_mut()
                .retain(|item| &item.urn != urn);
            (self.window_resize_callback)();
        }
    }

    /// Bump the oldest non-critical toast to make room for a critical one.
    fn bump_oldest_non_critical(&self, critical_item: ToastItem) {
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
    fn show_next_pending(&self) {
        if self.active_toasts.borrow().len() < 3 {
            if let Some(item) = self.pending_queue.borrow_mut().pop_front() {
                self.show_toast(item);
            }
        }
    }

    /// Calculate next TTL expiry deadline (for sleep-to-deadline).
    pub fn calculate_next_expiry(&self) -> Option<Duration> {
        let now = SystemTime::now();
        self.active_toasts
            .borrow()
            .iter()
            .filter_map(|item| item.expires_at)
            .filter(|expires| expires > &now)
            .min()
            .and_then(|expires| expires.duration_since(now).ok())
    }

    /// Expire toasts past their TTL.
    pub fn expire_toasts(&self) {
        let now = SystemTime::now();
        let expired: Vec<Urn> = self
            .active_toasts
            .borrow()
            .iter()
            .filter(|item| item.expires_at.map(|e| e <= now).unwrap_or(false))
            .map(|item| item.urn.clone())
            .collect();

        for urn in expired {
            self.dismiss_toast(&urn);
            self.show_next_pending();
        }
    }

    /// Check if there are any active toasts.
    pub fn has_active_toasts(&self) -> bool {
        !self.active_toasts.borrow().is_empty()
    }
}

/// DND filter logic.
fn should_show_toast(notification: &Notification, dnd_active: bool) -> bool {
    if !dnd_active {
        return true;
    }
    notification.urgency == NotificationUrgency::Critical
}
