//! Notification store manager.
//!
//! Channel-based state management using:
//! - `async-channel` for cross-thread message passing
//! - `once_cell` for lazy static initialization
//! - `RwLock` for thread-safe state access

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use indexmap::IndexMap;
use once_cell::sync::Lazy;

use super::super::dbus::ingress::IngressedNotification;
use super::super::types::{AppIdent, NotificationAction, NotificationIcon, NotificationUrgency};
use super::types::{Group, ItemLifecycle, Notification, NotificationOp, State};

/// Global notification store instance.
pub static STORE: Lazy<NotificationStore> = Lazy::new(NotificationStore::new);

// Thread-local storage for subscriber callbacks (must be on main thread)
thread_local! {
    static SUBSCRIBERS: RefCell<Vec<Rc<dyn Fn()>>> = RefCell::new(Vec::new());
}

/// The notification store - manages state and notifies subscribers of changes.
pub struct NotificationStore {
    state: RwLock<State>,
}

impl NotificationStore {
    fn new() -> Self {
        NotificationStore {
            state: RwLock::new(State::new()),
        }
    }

    /// Get read access to the current state.
    pub fn get_state(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.state.read().unwrap()
    }

    /// Dispatch an operation to modify state.
    pub fn dispatch(&self, op: NotificationOp) {
        let changed = {
            let mut state = self.state.write().unwrap();
            self.process_op(&mut state, op)
        };

        if changed {
            self.notify_subscribers();
        }
    }

    /// Emit an operation (alias for dispatch).
    pub fn emit(&self, op: NotificationOp) {
        self.dispatch(op);
    }

    /// Subscribe to state changes with a callback.
    /// The callback will be called on every state change.
    pub fn subscribe<F>(&'static self, callback: F)
    where
        F: Fn() + 'static,
    {
        SUBSCRIBERS.with(|subs| {
            subs.borrow_mut().push(Rc::new(callback));
        });
    }

    /// Notify all subscribers of a state change.
    fn notify_subscribers(&self) {
        SUBSCRIBERS.with(|subs| {
            for callback in subs.borrow().iter() {
                callback();
            }
        });
    }

    fn process_op(&self, state: &mut State, op: NotificationOp) -> bool {
        match op {
            NotificationOp::Tick => {
                let (follow_ups, tick_changed) = self.process_tick(state);
                let mut changed = tick_changed || !follow_ups.is_empty();
                for follow_up in follow_ups {
                    changed |= self.process_op(state, follow_up);
                }
                changed
            }
            NotificationOp::Ingress(n) => {
                self.process_ingress(state, n);
                true
            }
            NotificationOp::NotificationDismiss(id) => {
                self.process_dismiss(state, id);
                true
            }
            NotificationOp::NotificationDismissed(id) => {
                self.process_dismissed(state, id);
                true
            }
            NotificationOp::NotificationRetract(id) => {
                self.process_retract(state, id);
                true
            }
            NotificationOp::NotificationRetracted(id) => {
                self.process_retracted(state, id);
                true
            }
            NotificationOp::ToastHide(id) => {
                state.toasts.insert(id, ItemLifecycle::Hiding);
                state.hiding_timestamps.insert(id, SystemTime::now());
                true
            }
            NotificationOp::ToastHidden(id) => {
                state.toasts.insert(id, ItemLifecycle::Hidden);
                state.hiding_timestamps.shift_remove(&id);
                self.reconcile_toasts(state);
                true
            }
            NotificationOp::SetDnd(inhibited) => {
                state.dnd = inhibited;
                true
            }
            NotificationOp::ToastHoverEnter => {
                state.hover_paused = true;
                true
            }
            NotificationOp::ToastHoverLeave => {
                state.hover_paused = false;
                true
            }
            NotificationOp::Batch(ops) => {
                let all_ingress = ops.iter().all(|op| matches!(op, NotificationOp::Ingress(_)));
                if all_ingress {
                    self.process_ingress_batch(state, ops)
                } else {
                    let mut changed = false;
                    for op in ops {
                        changed |= self.process_op(state, op);
                    }
                    changed
                }
            }
        }
    }

    fn process_tick(&self, state: &mut State) -> (Vec<NotificationOp>, bool) {
        let now = SystemTime::now();
        let animation_duration = Duration::from_millis(250);
        let mut state_changed = false;

        let hiding_to_hidden: Vec<_> = state
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Hiding))
            .filter_map(|(toast_id, _)| {
                state.hiding_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        let appearing_to_visible: Vec<_> = state
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Appearing))
            .filter_map(|(toast_id, _)| {
                state.appearing_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        let dismissing_to_dismissed: Vec<_> = state
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Dismissing))
            .filter_map(|(toast_id, _)| {
                state.dismissing_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        let retracting_to_retracted: Vec<_> = state
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Retracting))
            .filter_map(|(toast_id, _)| {
                state.retracting_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        if !hiding_to_hidden.is_empty() {
            state_changed = true;
        }
        for toast_id in hiding_to_hidden {
            state.hiding_timestamps.shift_remove(&toast_id);
            state.visible_since_timestamps.shift_remove(&toast_id);

            // TTL-expired toasts are removed from queue entirely (freeing slot for others)
            // Non-TTL toasts (hidden due to slot limit) stay in queue as Hidden
            if state.ttl_expired_toasts.remove(&toast_id) {
                state.toasts.shift_remove(&toast_id);
            } else {
                state.toasts.insert(toast_id, ItemLifecycle::Hidden);
            }
        }

        for toast_id in appearing_to_visible {
            state.toasts.insert(toast_id, ItemLifecycle::Visible);
            state.appearing_timestamps.shift_remove(&toast_id);
            state.visible_since_timestamps.insert(toast_id, now);
        }

        // Skip TTL expiration when hover paused
        if state.hover_paused {
            self.reconcile_toasts(state);

            let mut follow_up_ops = Vec::new();
            for toast_id in dismissing_to_dismissed {
                state.dismissing_timestamps.shift_remove(&toast_id);
                follow_up_ops.push(NotificationOp::NotificationDismissed(toast_id));
            }
            for toast_id in retracting_to_retracted {
                state.retracting_timestamps.shift_remove(&toast_id);
                follow_up_ops.push(NotificationOp::NotificationRetracted(toast_id));
            }

            return (follow_up_ops, state_changed);
        }

        // Check for TTL expiration on visible toasts
        let timed_out_toasts: Vec<u64> = state
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Visible))
            .filter_map(|(toast_id, _)| {
                let notification = state.notifications.get(toast_id)?;

                // Skip resident notifications
                if notification.resident {
                    return None;
                }

                // Skip notifications without TTL (Critical urgency)
                let ttl_ms = notification.toast_ttl?;

                // Check if visible long enough to timeout
                let visible_since = state.visible_since_timestamps.get(toast_id)?;
                let elapsed = now.duration_since(*visible_since).unwrap_or_default();

                if elapsed >= Duration::from_millis(ttl_ms) {
                    Some(*toast_id)
                } else {
                    None
                }
            })
            .collect();

        // Transition timed-out toasts to Hiding and mark them for removal
        for toast_id in &timed_out_toasts {
            state.toasts.insert(*toast_id, ItemLifecycle::Hiding);
            state.visible_since_timestamps.shift_remove(toast_id);
            state.hiding_timestamps.insert(*toast_id, now);
            state.ttl_expired_toasts.insert(*toast_id);
            state_changed = true;
        }

        self.reconcile_toasts(state);

        let mut follow_up_ops = Vec::new();
        for toast_id in dismissing_to_dismissed {
            state.dismissing_timestamps.shift_remove(&toast_id);
            follow_up_ops.push(NotificationOp::NotificationDismissed(toast_id));
        }
        for toast_id in retracting_to_retracted {
            state.retracting_timestamps.shift_remove(&toast_id);
            follow_up_ops.push(NotificationOp::NotificationRetracted(toast_id));
        }

        (follow_up_ops, state_changed)
    }

    fn process_ingress(&self, state: &mut State, n: IngressedNotification) {
        let notification = create_notification(&n);
        let notif_id = notification.id;
        let group_id = notification.app_ident();
        let app_title = notification.app_title();
        state.notifications.insert(notif_id, notification);
        self.reconcile_group_on_ingress(state, notif_id, group_id, app_title);
        self.reconcile_toast_on_ingress(state, notif_id, true);
        // Add to panel notifications (unlimited)
        state.panel_notifications.insert(notif_id, ItemLifecycle::Visible);
        log::debug!(
            "[store] Added notification {} to panel_notifications, total: {}",
            notif_id,
            state.panel_notifications.len()
        );
    }

    fn process_ingress_batch(&self, state: &mut State, ops: Vec<NotificationOp>) -> bool {
        let mut changed = false;

        for op in ops {
            if let NotificationOp::Ingress(n) = op {
                let notification = create_notification(&n);
                let notif_id = notification.id;
                let group_id = notification.app_ident();
                let app_title = notification.app_title();
                state.notifications.insert(notif_id, notification);
                self.reconcile_group_on_ingress(state, notif_id, group_id, app_title);
                self.reconcile_toast_on_ingress(state, notif_id, false);
                // Add to panel notifications (unlimited)
                state.panel_notifications.insert(notif_id, ItemLifecycle::Visible);
                changed = true;
            }
        }

        if changed {
            self.reconcile_toasts(state);
        }

        changed
    }

    fn process_dismiss(&self, state: &mut State, id: u64) {
        if let Some(notification) = state.notifications.get(&id) {
            let group_id = notification.app_ident();
            if let Some(group) = state.groups.get_mut(group_id.as_ref()) {
                group.get_top_mut().insert(notification.id, ItemLifecycle::Dismissing);
            }
            state.toasts.insert(id, ItemLifecycle::Dismissing);
            state.panel_notifications.insert(id, ItemLifecycle::Dismissing);
            state.dismissing_timestamps.insert(id, SystemTime::now());
        }
    }

    fn process_dismissed(&self, state: &mut State, id: u64) {
        let group_id = state.notifications.get(&id).map(|n| n.app_ident());

        for group in state.groups.values_mut() {
            group.get_top_mut().shift_remove(&id);
        }
        state.toasts.shift_remove(&id);
        state.panel_notifications.shift_remove(&id);
        state.appearing_timestamps.shift_remove(&id);
        state.dismissing_timestamps.shift_remove(&id);
        state.hiding_timestamps.shift_remove(&id);
        state.visible_since_timestamps.shift_remove(&id);
        state.ttl_expired_toasts.remove(&id);
        let _ = state.notifications.remove(&id);

        self.reconcile_toasts(state);

        if let Some(group_id) = group_id {
            let group_has_any = state
                .notifications
                .values()
                .any(|n| n.app_ident() == group_id);

            if !group_has_any {
                state.archive.insert(group_id, ItemLifecycle::Dismissing);
            }
        }
    }

    fn process_retract(&self, state: &mut State, id: u64) {
        if let Some(notification) = state.notifications.get(&id) {
            let group_id = notification.app_ident();
            if let Some(group) = state.groups.get_mut(group_id.as_ref()) {
                group.get_top_mut().insert(notification.id, ItemLifecycle::Retracting);
            }
            state.toasts.insert(id, ItemLifecycle::Retracting);
            state.panel_notifications.insert(id, ItemLifecycle::Retracting);
            state.retracting_timestamps.insert(id, SystemTime::now());
        }
    }

    fn process_retracted(&self, state: &mut State, id: u64) {
        let group_id = state.notifications.get(&id).map(|n| n.app_ident());

        for group in state.groups.values_mut() {
            group.get_top_mut().shift_remove(&id);
        }
        state.toasts.shift_remove(&id);
        state.panel_notifications.shift_remove(&id);
        state.appearing_timestamps.shift_remove(&id);
        state.hiding_timestamps.shift_remove(&id);
        state.retracting_timestamps.shift_remove(&id);
        state.visible_since_timestamps.shift_remove(&id);
        state.ttl_expired_toasts.remove(&id);
        let _ = state.notifications.remove(&id);

        self.reconcile_toasts(state);

        if let Some(group_id) = group_id {
            let group_has_any = state
                .notifications
                .values()
                .any(|n| n.app_ident() == group_id);

            if !group_has_any {
                state.archive.insert(group_id, ItemLifecycle::Dismissing);
            }
        }
    }

    fn reconcile_toasts(&self, state: &mut State) {
        sort_notif_list(&state.notifications, &mut state.toasts);
        cut_notif_ids(
            &mut state.toasts,
            &mut state.appearing_timestamps,
            &mut state.hiding_timestamps,
            5,
        );
    }

    fn reconcile_group_on_ingress(
        &self,
        state: &mut State,
        notif_id: u64,
        group_id: Arc<str>,
        app_title: Arc<str>,
    ) {
        if let Some(group) = state.groups.get_mut(&group_id) {
            group.get_top_mut().insert(notif_id, ItemLifecycle::Visible);
            sort_notif_list(&state.notifications, group.get_top_mut());
            let mut stub_appearing = IndexMap::new();
            let mut stub_hiding = IndexMap::new();
            cut_notif_ids(group.get_top_mut(), &mut stub_appearing, &mut stub_hiding, 1);
        } else {
            state.groups.insert(
                group_id.clone(),
                Group::new(group_id.clone(), app_title, notif_id),
            );
        }
        state.archive.insert(group_id, ItemLifecycle::Visible);
    }

    fn reconcile_toast_on_ingress(&self, state: &mut State, notif_id: u64, do_sort: bool) {
        let notification = state.notifications.get(&notif_id);
        let should_toast = notification.map_or(false, |n| {
            should_toast(state.dnd, n.urgency, n.resident)
        });

        if !should_toast {
            return;
        }

        if state.toasts.is_empty() {
            state.toasts.insert(notif_id, ItemLifecycle::Appearing);
            state.appearing_timestamps.insert(notif_id, SystemTime::now());
        } else {
            state.toasts.insert(notif_id, ItemLifecycle::Pending);
            if do_sort {
                self.reconcile_toasts(state);
            }
        }
    }
}

// Helper functions

/// Determines whether a notification should be shown as a toast.
///
/// When DND (Do Not Disturb) is enabled:
/// - Critical notifications always show as toasts
/// - Resident notifications always show as toasts
/// - All other notifications are suppressed
///
/// When DND is disabled, all notifications show as toasts.
fn should_toast(dnd: bool, urgency: NotificationUrgency, resident: bool) -> bool {
    if !dnd {
        return true;
    }
    matches!(urgency, NotificationUrgency::Critical) || resident
}

fn create_notification(n: &IngressedNotification) -> Notification {
    Notification {
        actions: derive_actions(n),
        app: derive_app_ident(n),
        created_at: n.created_at,
        description: n.description.clone(),
        icon_hints: derive_icon_hints(n),
        id: n.id,
        replaces_id: n.replaces_id,
        resident: n.hints.resident,
        title: n.title.clone(),
        ttl: n.ttl,
        toast_ttl: derive_toast_ttl(n),
        urgency: n.hints.urgency,
    }
}

fn normalize_app_ident(app_ident: &str) -> Arc<str> {
    Arc::from(app_ident.to_lowercase().replace(' ', "_"))
}

fn derive_app_ident(notification: &IngressedNotification) -> Option<AppIdent> {
    let app_ident = &notification.app_name;
    let desktop = &notification.hints.desktop_entry;

    if let Some(app_ident) = app_ident {
        Some(AppIdent {
            ident: normalize_app_ident(app_ident),
            title: Some(app_ident.clone()),
        })
    } else if let Some(desktop) = desktop {
        Some(AppIdent {
            ident: normalize_app_ident(desktop),
            title: Some(desktop.clone()),
        })
    } else {
        None
    }
}

fn derive_toast_ttl(notification: &IngressedNotification) -> Option<u64> {
    // Explicit TTL > 0: use it (already in milliseconds from DBus)
    if let Some(ttl) = notification.ttl {
        if ttl > 0 {
            return Some(ttl);
        }
    }
    // ttl=0 means "never expire" (from expire_timeout=-1 in DBus)
    // ttl=None means "use server default"
    match notification.hints.urgency {
        NotificationUrgency::Critical => None, // Never expire
        NotificationUrgency::Normal => Some(10_000), // 10 seconds
        NotificationUrgency::Low => Some(5_000), // 5 seconds
    }
}

/// Derive actions from an ingressed notification.
pub fn derive_actions(notification: &IngressedNotification) -> Vec<NotificationAction> {
    let actions = &notification.actions;
    let mut out = Vec::new();
    let mut it = actions.iter();
    loop {
        let Some(key) = it.next() else { break };
        let Some(label) = it.next() else { break };
        out.push(NotificationAction {
            key: key.clone(),
            label: label.clone(),
        });
    }
    out
}

fn normalize_icon_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_whitespace() {
            out.push('-');
        } else if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch.to_ascii_lowercase());
        }
    }
    if out.is_empty() {
        input.to_ascii_lowercase()
    } else {
        out
    }
}

/// Derive icon hints from an ingressed notification.
pub fn derive_icon_hints(notification: &IngressedNotification) -> Vec<NotificationIcon> {
    let mut out = Vec::new();
    if let Some(bytes) = &notification.hints.image_data {
        out.push(NotificationIcon::Bytes(bytes.clone()));
    }
    // image-path hint can be a file path OR an icon name per freedesktop spec
    if let Some(path) = &notification.hints.image_path {
        out.push(NotificationIcon::from_str(path));
    }
    if let Some(specific) = &notification.icon {
        out.push(NotificationIcon::from_str(specific));
    }

    if let Some(de) = &notification.hints.desktop_entry {
        let trimmed = de.trim();
        if !trimmed.is_empty() {
            let without_suffix = trimmed.strip_suffix(".desktop").unwrap_or(trimmed);
            out.push(NotificationIcon::from_str(&Arc::from(without_suffix)));
            out.push(NotificationIcon::from_str(&Arc::from(normalize_icon_name(
                without_suffix,
            ))));
        }
    }

    if let Some(app_name) = &notification.app_name {
        let trimmed = app_name.trim();
        if !trimmed.is_empty() {
            out.push(NotificationIcon::Themed(
                normalize_icon_name(app_name).into(),
            ));
        }
    }

    out
}

fn sort_notif_list(
    notifications: &HashMap<u64, Notification>,
    top: &mut IndexMap<u64, ItemLifecycle>,
) {
    top.sort_by(
        |k1, _v1, k2, _v2| match (notifications.get(k1), notifications.get(k2)) {
            (Some(n1), Some(n2)) => n1.cmp(n2),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
    );
}

fn cut_notif_ids(
    top: &mut IndexMap<u64, ItemLifecycle>,
    appearing_timestamps: &mut IndexMap<u64, SystemTime>,
    hiding_timestamps: &mut IndexMap<u64, SystemTime>,
    limit: usize,
) {
    if top.is_empty() || limit == 0 {
        return;
    }

    let mut selected: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut filled = 0usize;

    for (id, lifecycle) in top.iter().rev() {
        if filled >= limit {
            break;
        }
        match lifecycle {
            ItemLifecycle::Dismissed | ItemLifecycle::Retracted => {}
            ItemLifecycle::Dismissing | ItemLifecycle::Retracting => {}
            _ => {
                selected.insert(*id);
                filled += 1;
            }
        }
    }

    let mut to_remove: Vec<u64> = Vec::new();

    for (id, lifecycle) in top.iter_mut() {
        if matches!(lifecycle, ItemLifecycle::Dismissed) {
            to_remove.push(*id);
            continue;
        }

        if matches!(lifecycle, ItemLifecycle::Dismissing)
            || matches!(lifecycle, ItemLifecycle::Retracting)
        {
            continue;
        }

        if selected.contains(id) {
            // Don't reset toasts that are visible, appearing, or in the process of hiding
            // Hidden toasts CAN be promoted back to Appearing when slots free up
            // Keep promoted toasts at their original position so they appear at the bottom
            // when the UI reverses the order for display (newest first)
            if !matches!(lifecycle, ItemLifecycle::Visible | ItemLifecycle::Appearing | ItemLifecycle::Hiding) {
                *lifecycle = ItemLifecycle::Appearing;
                appearing_timestamps.insert(*id, SystemTime::now());
            }
            continue;
        }

        match lifecycle {
            ItemLifecycle::Hiding => {}
            ItemLifecycle::Hidden | ItemLifecycle::Pending => {}
            ItemLifecycle::Visible | ItemLifecycle::Appearing => {
                *lifecycle = ItemLifecycle::Hiding;
                appearing_timestamps.shift_remove(id);
                hiding_timestamps.insert(*id, SystemTime::now());
            }
            ItemLifecycle::Dismissing | ItemLifecycle::Retracting => {}
            ItemLifecycle::Dismissed | ItemLifecycle::Retracted => {
                to_remove.push(*id);
            }
        }
    }

    for id in to_remove {
        top.shift_remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::notifications::dbus::hints::Hints;
    use crate::features::notifications::dbus::ingress::IngressedNotification;

    fn make_hints(urgency: NotificationUrgency, resident: bool) -> Hints {
        Hints {
            action_icons: false,
            category: None,
            desktop_entry: None,
            image_data: None,
            image_path: None,
            resident,
            sound_file: None,
            sound_name: None,
            suppress_sound: false,
            transient: false,
            urgency,
            x: 0,
            y: 0,
        }
    }

    fn make_notification(id: u64, urgency: NotificationUrgency, resident: bool) -> IngressedNotification {
        IngressedNotification {
            app_name: Some(Arc::from("test-app")),
            actions: vec![],
            created_at: SystemTime::now(),
            description: Arc::from("Test description"),
            icon: None,
            id,
            hints: make_hints(urgency, resident),
            replaces_id: None,
            title: Arc::from("Test title"),
            ttl: None,
        }
    }

    // Phase 1: Baseline tests for current store behavior

    #[test]
    fn test_ingress_adds_notification_to_store() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        {
            let mut state = store.state.write().unwrap();
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        assert!(state.notifications.contains_key(&1));
    }

    #[test]
    fn test_ingress_adds_notification_to_panel() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        {
            let mut state = store.state.write().unwrap();
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        assert!(state.panel_notifications.contains_key(&1));
    }

    #[test]
    fn test_ingress_adds_notification_to_toasts() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        {
            let mut state = store.state.write().unwrap();
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        assert!(state.toasts.contains_key(&1));
    }

    #[test]
    fn test_dismiss_removes_from_toasts() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        {
            let mut state = store.state.write().unwrap();
            store.process_ingress(&mut state, notif);
            store.process_dismiss(&mut state, 1);
            store.process_dismissed(&mut state, 1);
        }

        let state = store.state.read().unwrap();
        assert!(!state.toasts.contains_key(&1));
    }

    #[test]
    fn test_dismiss_removes_from_panel() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        {
            let mut state = store.state.write().unwrap();
            store.process_ingress(&mut state, notif);
            store.process_dismiss(&mut state, 1);
            store.process_dismissed(&mut state, 1);
        }

        let state = store.state.read().unwrap();
        assert!(!state.panel_notifications.contains_key(&1));
    }

    #[test]
    fn test_batch_ingress_adds_multiple_notifications() {
        let store = NotificationStore::new();
        let ops = vec![
            NotificationOp::Ingress(make_notification(1, NotificationUrgency::Normal, false)),
            NotificationOp::Ingress(make_notification(2, NotificationUrgency::Normal, false)),
            NotificationOp::Ingress(make_notification(3, NotificationUrgency::Normal, false)),
        ];

        {
            let mut state = store.state.write().unwrap();
            store.process_ingress_batch(&mut state, ops);
        }

        let state = store.state.read().unwrap();
        assert!(state.notifications.contains_key(&1));
        assert!(state.notifications.contains_key(&2));
        assert!(state.notifications.contains_key(&3));
        assert_eq!(state.panel_notifications.len(), 3);
    }

    // Phase 2: DND feature tests (these will fail until we implement DND)

    #[test]
    fn test_dnd_blocks_normal_notification_toast() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        {
            let mut state = store.state.write().unwrap();
            state.dnd = true;
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        // Notification should be stored
        assert!(state.notifications.contains_key(&1));
        // But NOT in toasts due to DND
        assert!(!state.toasts.contains_key(&1));
    }

    #[test]
    fn test_dnd_blocks_low_notification_toast() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Low, false);

        {
            let mut state = store.state.write().unwrap();
            state.dnd = true;
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        assert!(state.notifications.contains_key(&1));
        assert!(!state.toasts.contains_key(&1));
    }

    #[test]
    fn test_dnd_allows_critical_notification_toast() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Critical, false);

        {
            let mut state = store.state.write().unwrap();
            state.dnd = true;
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        assert!(state.notifications.contains_key(&1));
        // Critical notifications SHOULD show as toasts even during DND
        assert!(state.toasts.contains_key(&1));
    }

    #[test]
    fn test_dnd_allows_resident_notification_toast() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, true);

        {
            let mut state = store.state.write().unwrap();
            state.dnd = true;
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        assert!(state.notifications.contains_key(&1));
        // Resident notifications SHOULD show as toasts even during DND
        assert!(state.toasts.contains_key(&1));
    }

    #[test]
    fn test_dnd_always_adds_to_panel() {
        let store = NotificationStore::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        {
            let mut state = store.state.write().unwrap();
            state.dnd = true;
            store.process_ingress(&mut state, notif);
        }

        let state = store.state.read().unwrap();
        // Panel should ALWAYS receive notifications regardless of DND
        assert!(state.panel_notifications.contains_key(&1));
    }

    #[test]
    fn test_set_dnd_operation_updates_state() {
        let store = NotificationStore::new();

        {
            let mut state = store.state.write().unwrap();
            assert!(!state.dnd);
            store.process_op(&mut state, NotificationOp::SetDnd(true));
            assert!(state.dnd);
            store.process_op(&mut state, NotificationOp::SetDnd(false));
            assert!(!state.dnd);
        }
    }

    // Toast slot limit and promotion tests

    #[test]
    fn test_toast_slot_limit_hides_older_toasts() {
        // Push 10 notifications, only 5 should be visible, rest hidden
        let store = NotificationStore::new();

        {
            let mut state = store.state.write().unwrap();

            // Push notifications 1-5
            for id in 1..=5 {
                store.process_ingress(&mut state, make_notification(id, NotificationUrgency::Normal, false));
            }

            // All 5 should be in toasts
            assert_eq!(state.toasts.len(), 5);

            // Push notifications 6-10 (these should push out 1-5)
            for id in 6..=10 {
                store.process_ingress(&mut state, make_notification(id, NotificationUrgency::Normal, false));
            }

            // Should have 10 toasts total (5 visible + 5 hidden)
            assert_eq!(state.toasts.len(), 10);

            // Count visible vs hidden
            let visible_count = state.toasts.values()
                .filter(|l| matches!(l, ItemLifecycle::Visible | ItemLifecycle::Appearing))
                .count();
            let hidden_count = state.toasts.values()
                .filter(|l| matches!(l, ItemLifecycle::Hidden | ItemLifecycle::Hiding | ItemLifecycle::Pending))
                .count();

            assert_eq!(visible_count, 5, "Should have 5 visible toasts");
            assert_eq!(hidden_count, 5, "Should have 5 hidden toasts");
        }
    }

    #[test]
    fn test_dismiss_promotes_hidden_toast_to_visible() {
        // Push 10 notifications, dismiss one visible, hidden should be promoted
        let store = NotificationStore::new();

        {
            let mut state = store.state.write().unwrap();

            // Push notifications 1-10
            for id in 1..=10 {
                store.process_ingress(&mut state, make_notification(id, NotificationUrgency::Normal, false));
            }

            // Verify initial state: 10 toasts
            assert_eq!(state.toasts.len(), 10);

            // Simulate time passing and tick to complete Hiding → Hidden transitions
            // Set hiding timestamps to the past so they complete
            let past = SystemTime::now() - Duration::from_millis(500);
            let hiding_ids: Vec<u64> = state.toasts.iter()
                .filter(|(_, l)| matches!(l, ItemLifecycle::Hiding))
                .map(|(id, _)| *id)
                .collect();
            for id in hiding_ids {
                state.hiding_timestamps.insert(id, past);
            }
            // Run tick to process Hiding → Hidden
            store.process_tick(&mut state);

            // Debug: print all toast states before dismiss
            eprintln!("Before dismiss:");
            for (id, lifecycle) in state.toasts.iter() {
                eprintln!("  Toast {}: {:?}", id, lifecycle);
            }

            // Count visible vs hidden
            let visible_before = state.toasts.values()
                .filter(|l| matches!(l, ItemLifecycle::Visible | ItemLifecycle::Appearing))
                .count();
            let hidden_before = state.toasts.values()
                .filter(|l| matches!(l, ItemLifecycle::Hidden))
                .count();
            eprintln!("Visible: {}, Hidden: {}", visible_before, hidden_before);

            // Find a visible toast to dismiss (should be one of the newer ones: 6-10)
            let visible_toast_id = state.toasts.iter()
                .find(|(_, l)| matches!(l, ItemLifecycle::Visible | ItemLifecycle::Appearing))
                .map(|(id, _)| *id)
                .expect("Should have a visible toast");

            eprintln!("Dismissing toast {}", visible_toast_id);

            // Dismiss the visible toast
            store.process_dismiss(&mut state, visible_toast_id);
            store.process_dismissed(&mut state, visible_toast_id);

            // Debug: print all toast states after dismiss
            eprintln!("After dismiss:");
            for (id, lifecycle) in state.toasts.iter() {
                eprintln!("  Toast {}: {:?}", id, lifecycle);
            }

            // Should now have 9 toasts
            assert_eq!(state.toasts.len(), 9, "Should have 9 toasts after dismiss");

            // Should still have 5 visible (a hidden one was promoted)
            let visible_count = state.toasts.values()
                .filter(|l| matches!(l, ItemLifecycle::Visible | ItemLifecycle::Appearing))
                .count();

            assert_eq!(visible_count, 5, "Should still have 5 visible toasts after promotion");
        }
    }

    #[test]
    fn test_promoted_toast_maintains_position_order() {
        // This test verifies that when a hidden toast is promoted, it maintains its
        // original position in the IndexMap (before the newer visible toasts).
        // This is critical for correct UI ordering: the UI reverses the state order,
        // so a promoted toast at position 5 (before 7,8,9,10) will appear at the
        // bottom of the display (after 10,9,8,7 from top).
        let store = NotificationStore::new();

        {
            let mut state = store.state.write().unwrap();

            // Push notifications 1-10
            for id in 1..=10 {
                store.process_ingress(&mut state, make_notification(id, NotificationUrgency::Normal, false));
            }

            // Simulate time passing for Hiding → Hidden transitions
            let past = SystemTime::now() - Duration::from_millis(500);
            let hiding_ids: Vec<u64> = state.toasts.iter()
                .filter(|(_, l)| matches!(l, ItemLifecycle::Hiding))
                .map(|(id, _)| *id)
                .collect();
            for id in hiding_ids {
                state.hiding_timestamps.insert(id, past);
            }
            store.process_tick(&mut state);

            // Verify initial state order: [1,2,3,4,5,6,7,8,9,10]
            // with 1-5 Hidden and 6-10 Visible/Appearing
            let initial_order: Vec<u64> = state.toasts.keys().copied().collect();
            assert_eq!(initial_order, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

            // Dismiss toast 6 (the oldest visible toast)
            store.process_dismiss(&mut state, 6);
            store.process_dismissed(&mut state, 6);

            // After dismissal:
            // - Toast 6 should be removed
            // - Toast 5 should be promoted (Appearing) but stay at its original position
            // - Order should be: [1,2,3,4,5,7,8,9,10]
            let order_after_dismiss: Vec<u64> = state.toasts.keys().copied().collect();
            assert_eq!(
                order_after_dismiss,
                vec![1, 2, 3, 4, 5, 7, 8, 9, 10],
                "Promoted toast 5 should maintain its original position before 7,8,9,10"
            );

            // Verify toast 5 is now visible (Appearing)
            assert!(
                matches!(state.toasts.get(&5), Some(ItemLifecycle::Appearing)),
                "Toast 5 should be promoted to Appearing"
            );

            // Verify the visible toasts order (filtered, in state order)
            let visible_order: Vec<u64> = state.toasts.iter()
                .filter(|(_, l)| matches!(l, ItemLifecycle::Visible | ItemLifecycle::Appearing))
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(
                visible_order,
                vec![5, 7, 8, 9, 10],
                "Visible toasts should be in order [5,7,8,9,10] for correct UI display"
            );

            // When UI reverses this for display (newest first), it becomes [10,9,8,7,5]
            // which is the correct order: newest at top, promoted toast at bottom
        }
    }

    #[test]
    fn test_multiple_promotions_maintain_order() {
        // Test that multiple consecutive promotions maintain correct order
        let store = NotificationStore::new();

        {
            let mut state = store.state.write().unwrap();

            // Push notifications 1-10
            for id in 1..=10 {
                store.process_ingress(&mut state, make_notification(id, NotificationUrgency::Normal, false));
            }

            // Simulate time for Hidden transitions
            let past = SystemTime::now() - Duration::from_millis(500);
            let hiding_ids: Vec<u64> = state.toasts.iter()
                .filter(|(_, l)| matches!(l, ItemLifecycle::Hiding))
                .map(|(id, _)| *id)
                .collect();
            for id in hiding_ids {
                state.hiding_timestamps.insert(id, past);
            }
            store.process_tick(&mut state);

            // Dismiss toast 6, promoting toast 5
            store.process_dismiss(&mut state, 6);
            store.process_dismissed(&mut state, 6);

            // Verify order: [1,2,3,4,5,7,8,9,10] with visible [5,7,8,9,10]
            let visible_order: Vec<u64> = state.toasts.iter()
                .filter(|(_, l)| matches!(l, ItemLifecycle::Visible | ItemLifecycle::Appearing))
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(visible_order, vec![5, 7, 8, 9, 10]);

            // Dismiss toast 7, promoting toast 4
            store.process_dismiss(&mut state, 7);
            store.process_dismissed(&mut state, 7);

            // Verify order: [1,2,3,4,5,8,9,10] with visible [4,5,8,9,10]
            let visible_order: Vec<u64> = state.toasts.iter()
                .filter(|(_, l)| matches!(l, ItemLifecycle::Visible | ItemLifecycle::Appearing))
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(
                visible_order,
                vec![4, 5, 8, 9, 10],
                "After second promotion, visible order should be [4,5,8,9,10]"
            );

            // UI would display as [10,9,8,5,4] - newest first, promoted toasts at bottom in their original order
        }
    }
}
