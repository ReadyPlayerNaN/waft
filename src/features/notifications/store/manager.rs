//! Notification store manager.
//!
//! Channel-based state management using:
//! - `async-channel` for cross-thread message passing
//! - `once_cell` for lazy static initialization
//! - `RwLock` for thread-safe state access

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use async_channel::{Receiver, Sender};
use indexmap::IndexMap;
use once_cell::sync::Lazy;

use super::super::dbus::ingress::IngressedNotification;
use super::super::types::{AppIdent, NotificationAction, NotificationIcon, NotificationUrgency};
use super::types::{Group, ItemLifecycle, Notification, NotificationOp, State};

/// Global notification store instance.
pub static STORE: Lazy<NotificationStore> = Lazy::new(NotificationStore::new);

/// The notification store - manages state and notifies subscribers of changes.
pub struct NotificationStore {
    state: RwLock<State>,
    tx: Sender<()>,
    rx: Receiver<()>,
}

impl NotificationStore {
    fn new() -> Self {
        let (tx, rx) = async_channel::unbounded();
        NotificationStore {
            state: RwLock::new(State::new()),
            tx,
            rx,
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
            let _ = self.tx.try_send(());
        }
    }

    /// Emit an operation (alias for dispatch).
    pub fn emit(&self, op: NotificationOp) {
        self.dispatch(op);
    }

    /// Subscribe to state changes with a callback.
    pub fn subscribe<F>(&'static self, callback: F)
    where
        F: Fn() + 'static,
    {
        let rx = self.rx.clone();

        glib::spawn_future_local(async move {
            while rx.recv().await.is_ok() {
                callback();
            }
        });
    }

    fn process_op(&self, state: &mut State, op: NotificationOp) -> bool {
        match op {
            NotificationOp::Tick => {
                let follow_ups = self.process_tick(state);
                let mut changed = !follow_ups.is_empty();
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

    fn process_tick(&self, state: &mut State) -> Vec<NotificationOp> {
        let now = SystemTime::now();
        let animation_duration = Duration::from_millis(250);

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

        for toast_id in hiding_to_hidden {
            state.toasts.insert(toast_id, ItemLifecycle::Hidden);
            state.hiding_timestamps.shift_remove(&toast_id);
        }

        for toast_id in appearing_to_visible {
            state.toasts.insert(toast_id, ItemLifecycle::Visible);
            state.appearing_timestamps.shift_remove(&toast_id);
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

        follow_up_ops
    }

    fn process_ingress(&self, state: &mut State, n: IngressedNotification) {
        let notification = create_notification(&n);
        let notif_id = notification.id;
        let group_id = notification.app_ident();
        let app_title = notification.app_title();
        state.notifications.insert(notif_id, notification);
        self.reconcile_group_on_ingress(state, notif_id, group_id, app_title);
        self.reconcile_toast_on_ingress(state, notif_id, true);
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
            state.dismissing_timestamps.insert(id, SystemTime::now());
        }
    }

    fn process_dismissed(&self, state: &mut State, id: u64) {
        let group_id = state.notifications.get(&id).map(|n| n.app_ident());

        for group in state.groups.values_mut() {
            group.get_top_mut().shift_remove(&id);
        }
        state.toasts.shift_remove(&id);
        state.appearing_timestamps.shift_remove(&id);
        state.dismissing_timestamps.shift_remove(&id);
        state.hiding_timestamps.shift_remove(&id);
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
            state.retracting_timestamps.insert(id, SystemTime::now());
        }
    }

    fn process_retracted(&self, state: &mut State, id: u64) {
        let group_id = state.notifications.get(&id).map(|n| n.app_ident());

        for group in state.groups.values_mut() {
            group.get_top_mut().shift_remove(&id);
        }
        state.toasts.shift_remove(&id);
        state.appearing_timestamps.shift_remove(&id);
        state.hiding_timestamps.shift_remove(&id);
        state.retracting_timestamps.shift_remove(&id);
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

fn create_notification(n: &IngressedNotification) -> Notification {
    Notification {
        actions: derive_actions(n),
        app: derive_app_ident(n),
        created_at: n.created_at,
        description: n.description.clone(),
        icon_hints: derive_icon_hints(n),
        id: n.id,
        replaces_id: n.replaces_id,
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
    if notification.ttl.is_some() {
        notification.ttl
    } else {
        match notification.hints.urgency {
            NotificationUrgency::Critical => None,
            NotificationUrgency::Normal => Some(10),
            NotificationUrgency::Low => Some(5),
        }
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
            if !matches!(lifecycle, ItemLifecycle::Visible | ItemLifecycle::Appearing) {
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
