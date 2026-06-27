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

use waft_client::EntityActionCallback;

struct ToastItem {
    urn: Urn,
    entity: Notification,
}

const DEFAULT_TOAST_TTL_MS: u64 = 5000;

impl ToastItem {
    fn from_notification(urn: Urn, notification: Notification) -> Self {
        Self {
            urn,
            entity: notification,
        }
    }
}

fn toast_ttl_for(notification: &Notification) -> Option<u64> {
    match notification.urgency {
        NotificationUrgency::Critical => None,
        _ => Some(
            notification
                .ttl
                .map(|sender_ttl| sender_ttl.min(DEFAULT_TOAST_TTL_MS))
                .unwrap_or(DEFAULT_TOAST_TTL_MS),
        ),
    }
}

fn urgency_priority(urgency: NotificationUrgency, has_ttl: bool) -> u8 {
    match (urgency, has_ttl) {
        (NotificationUrgency::Critical, _) => 4,
        (NotificationUrgency::Normal, true) => 3,
        (NotificationUrgency::Normal, false) => 2,
        (NotificationUrgency::Low, true) => 1,
        (NotificationUrgency::Low, false) => 0,
    }
}

pub struct ToastManager {
    container: gtk::Box,
    position: ToastPosition,
    active_toasts: Rc<RefCell<Vec<ToastItem>>>,
    pending_queue: RefCell<VecDeque<ToastItem>>,
    widgets: RefCell<HashMap<Urn, Rc<NotificationCard>>>,
    action_callback: EntityActionCallback,
    dnd_active: Cell<bool>,
    suppressed_by_overview: Cell<bool>,
    clear_on_overview_open: bool,
    window_resize_callback: Rc<dyn Fn()>,
    window_visibility_callback: Rc<dyn Fn(bool)>,
}

impl ToastManager {
    pub fn new(
        container: gtk::Box,
        action_callback: EntityActionCallback,
        window_resize_callback: Rc<dyn Fn()>,
        window_visibility_callback: Rc<dyn Fn(bool)>,
        position: ToastPosition,
        clear_on_overview_open: bool,
    ) -> Self {
        Self {
            container,
            position,
            active_toasts: Rc::new(RefCell::new(Vec::new())),
            pending_queue: RefCell::new(VecDeque::new()),
            widgets: RefCell::new(HashMap::new()),
            action_callback,
            dnd_active: Cell::new(false),
            suppressed_by_overview: Cell::new(false),
            clear_on_overview_open,
            window_resize_callback,
            window_visibility_callback,
        }
    }

    pub fn set_overview_visible(self: &Rc<Self>, visible: bool) {
        self.suppressed_by_overview.set(visible);
        if visible && self.clear_on_overview_open {
            self.clear_local_toasts();
        } else {
            self.update_window_visibility();
        }
    }

    pub fn handle_notification(self: &Rc<Self>, urn: Urn, notification: Notification) {
        if self.suppressed_by_overview.get()
            || !should_show_toast(&notification, self.dnd_active.get())
        {
            return;
        }

        let item = ToastItem::from_notification(urn, notification);
        if self.active_toasts.borrow().len() < 3 {
            self.show_toast(item);
        } else if item.entity.urgency == NotificationUrgency::Critical {
            self.bump_oldest_non_critical(item);
        } else {
            let item_has_ttl = item.entity.ttl.is_some();
            let item_urgency = item.entity.urgency;
            let mut queue = self.pending_queue.borrow_mut();
            let pos = queue.iter().position(|queued| {
                let queued_has_ttl = queued.entity.ttl.is_some();
                let queued_urgency = queued.entity.urgency;
                urgency_priority(queued_urgency, queued_has_ttl)
                    < urgency_priority(item_urgency, item_has_ttl)
            });
            match pos {
                Some(idx) => queue.insert(idx, item),
                None => queue.push_back(item),
            }
        }
    }

    pub fn handle_dnd(&self, dnd: &Dnd) {
        self.dnd_active.set(dnd.active);
    }

    pub fn handle_entity_removed(self: &Rc<Self>, urn: &Urn) {
        self.dismiss_toast(urn);
        self.pending_queue
            .borrow_mut()
            .retain(|item| &item.urn != urn);
        self.show_next_pending();
    }

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

        let action_callback = self.action_callback.clone();
        let self_weak = Rc::downgrade(self);
        card.connect_output(move |output| match output {
            NotificationCardOutput::ActionClick(urn, action) => {
                action_callback(
                    urn,
                    "invoke-action".into(),
                    serde_json::json!({ "key": action }),
                );
            }
            NotificationCardOutput::Close(urn) => {
                action_callback(urn, "dismiss".into(), Value::Null);
            }
            NotificationCardOutput::TimedOut(urn) => {
                if let Some(mgr) = self_weak.upgrade() {
                    if mgr
                        .active_toasts
                        .borrow()
                        .iter()
                        .any(|item| item.urn == urn && item.entity.ttl.is_none())
                    {
                        action_callback(urn.clone(), "expire".into(), Value::Null);
                    }
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
        self.update_window_visibility();
    }

    fn dismiss_toast(&self, urn: &Urn) {
        if let Some(card) = self.widgets.borrow_mut().remove(urn) {
            self.active_toasts
                .borrow_mut()
                .retain(|item| &item.urn != urn);
            let container = self.container.clone();
            let active_toasts = self.active_toasts.clone();
            let suppression = self.suppressed_by_overview.clone();
            let visibility_cb = self.window_visibility_callback.clone();
            let card_root = card.widget().clone();
            let handled = Rc::new(Cell::new(false));
            card.revealer().connect_child_revealed_notify(move |rev| {
                if !rev.is_child_revealed() && !handled.get() {
                    handled.set(true);
                    container.remove(&card_root);
                    visibility_cb(!suppression.get() && !active_toasts.borrow().is_empty());
                }
            });
            card.hide_and_remove();
            self.update_window_visibility();
        }
    }

    fn clear_local_toasts(&self) {
        let cards: Vec<_> = self
            .widgets
            .borrow_mut()
            .drain()
            .map(|(_, card)| card)
            .collect();
        self.active_toasts.borrow_mut().clear();
        self.pending_queue.borrow_mut().clear();
        for card in cards {
            self.container.remove(card.widget());
        }
        self.update_window_visibility();
    }

    fn update_window_visibility(&self) {
        update_window_visibility_with(
            &self.active_toasts,
            &self.suppressed_by_overview,
            self.window_visibility_callback.as_ref(),
        );
    }

    fn bump_oldest_non_critical(self: &Rc<Self>, critical_item: ToastItem) {
        let mut active = self.active_toasts.borrow_mut();
        if let Some(index) = active
            .iter()
            .position(|item| item.entity.urgency != NotificationUrgency::Critical)
        {
            let bumped = active.remove(index);
            drop(active);
            self.dismiss_toast(&bumped.urn);
            self.pending_queue.borrow_mut().push_front(bumped);
            self.show_toast(critical_item);
        } else {
            self.pending_queue.borrow_mut().push_back(critical_item);
        }
    }

    fn show_next_pending(self: &Rc<Self>) {
        if self.suppressed_by_overview.get() {
            return;
        }
        if self.active_toasts.borrow().len() < 3
            && let Some(item) = self.pending_queue.borrow_mut().pop_front()
        {
            self.show_toast(item);
        }
    }
}

fn should_show_toast(notification: &Notification, dnd_active: bool) -> bool {
    if notification.suppress_toast {
        return false;
    }
    if !dnd_active {
        return true;
    }
    notification.urgency == NotificationUrgency::Critical
}

fn update_window_visibility_with(
    active_toasts: &Rc<RefCell<Vec<ToastItem>>>,
    suppressed_by_overview: &Cell<bool>,
    visibility_cb: &dyn Fn(bool),
) {
    visibility_cb(!suppressed_by_overview.get() && !active_toasts.borrow().is_empty());
}

#[cfg(test)]
impl ToastManager {
    pub(crate) fn test_state(&self) -> (usize, usize, usize, bool, bool) {
        (
            self.active_toasts.borrow().len(),
            self.pending_queue.borrow().len(),
            self.widgets.borrow().len(),
            self.suppressed_by_overview.get(),
            self.clear_on_overview_open,
        )
    }
}
