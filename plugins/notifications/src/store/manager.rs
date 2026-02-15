//! Notification store manager.
//!
//! Simplified for daemon architecture: no toast reconciliation, no animation states.
//! Dismiss/retract are immediate removals. TTL expiration is handled by the external
//! `ttl` module which sends `TtlExpiry` operations.

use std::sync::Arc;
use std::time::SystemTime;

use super::super::dbus::ingress::IngressedNotification;
use super::super::types::{AppIdent, NotificationAction, NotificationIcon};
use super::types::{Group, ItemLifecycle, Notification, NotificationOp, State};

/// Process a notification operation on the given state.
///
/// Returns whether state changed.
pub fn process_op(state: &mut State, op: NotificationOp) -> bool {
    match op {
        NotificationOp::Batch(ops) => {
            let mut changed = false;
            for op in ops {
                changed |= process_op(state, op);
            }
            changed
        }
        NotificationOp::Ingress(n) => {
            process_ingress(state, *n);
            true
        }
        NotificationOp::NotificationDismiss(id) => process_dismiss(state, id),
        NotificationOp::NotificationRetract(id) => process_retract(state, id),
        NotificationOp::SetDnd(inhibited) => {
            state.dnd = inhibited;
            true
        }
        NotificationOp::TtlExpiry(ids) => process_ttl_expiry(state, ids),
    }
}

fn process_ingress(state: &mut State, n: IngressedNotification) {
    // Handle replaces_id: remove the old notification if it exists
    if let Some(old_id) = n.replaces_id
        && old_id != 0
        && state.notifications.contains_key(&old_id)
    {
        log::debug!("[store] Replacing notification {} with {}", old_id, n.id);
        remove_notification(state, old_id);
    }

    let notification = create_notification(&n);
    let notif_id = notification.id;
    let group_id = notification.app_ident();
    let app_title = notification.app_title();
    state.notifications.insert(notif_id, notification);
    log::trace!(
        "[store/ingress] Inserted notification {} into notifications HashMap, total: {}",
        notif_id,
        state.notifications.len()
    );
    reconcile_group_on_ingress(state, notif_id, group_id, app_title);
    // Add to panel notifications
    state
        .panel_notifications
        .insert(notif_id, ItemLifecycle::Visible);
    state
        .panel_visible_since_timestamps
        .insert(notif_id, SystemTime::now());
    log::trace!(
        "[store/ingress] Added notification {} to panel_notifications, total panel: {}, total in HashMap: {}",
        notif_id,
        state.panel_notifications.len(),
        state.notifications.len()
    );
}

/// Dismiss a notification (user action). Immediate removal.
fn process_dismiss(state: &mut State, id: u64) -> bool {
    if !state.notifications.contains_key(&id) {
        return false;
    }
    remove_notification(state, id);
    true
}

/// Retract a notification (CloseNotification D-Bus call). Immediate removal.
fn process_retract(state: &mut State, id: u64) -> bool {
    if !state.notifications.contains_key(&id) {
        return false;
    }
    remove_notification(state, id);
    true
}

/// Remove expired notifications by TTL.
fn process_ttl_expiry(state: &mut State, ids: Vec<u64>) -> bool {
    let mut changed = false;
    for id in ids {
        if state.notifications.contains_key(&id) {
            log::debug!("[store] Notification {} TTL expired", id);
            remove_notification(state, id);
            changed = true;
        }
    }
    changed
}

// Helper functions

/// Remove all traces of a notification from state.
fn remove_notification(state: &mut State, id: u64) {
    let group_id = state.notifications.get(&id).map(|n| n.app_ident());

    state.notifications.remove(&id);
    state.panel_notifications.shift_remove(&id);
    state.panel_visible_since_timestamps.shift_remove(&id);

    for group in state.groups.values_mut() {
        group.get_top_mut().shift_remove(&id);
    }

    if let Some(group_id) = group_id {
        let group_has_any = state
            .notifications
            .values()
            .any(|n| n.app_ident() == group_id);

        if !group_has_any {
            state.groups.remove(group_id.as_ref());
            state.archive.insert(group_id, ItemLifecycle::Dismissing);
        }
    }
}

fn reconcile_group_on_ingress(
    state: &mut State,
    notif_id: u64,
    group_id: Arc<str>,
    app_title: Arc<str>,
) {
    if let Some(group) = state.groups.get_mut(&group_id) {
        group.get_top_mut().insert(notif_id, ItemLifecycle::Visible);
    } else {
        state.groups.insert(
            group_id.clone(),
            Group::new(group_id.clone(), app_title, notif_id),
        );
    }
    state.archive.insert(group_id, ItemLifecycle::Visible);
}

fn create_notification(n: &IngressedNotification) -> Notification {
    let mut notification = Notification {
        actions: derive_actions(n),
        app: derive_app_ident(n),
        created_at: n.created_at,
        description: n.description.clone(),
        icon_hints: derive_icon_hints(n),
        id: n.id,
        replaces_id: n.replaces_id,
        resident: n.hints.resident,
        suppress_toast: false,
        title: n.title.clone(),
        ttl: derive_panel_ttl(n),
        urgency: n.hints.urgency,
        workspace: None,
    };

    if let Some(app_name) = n.app_name.as_deref()
        && let Some(extraction) = super::workspace_extract::extract_workspace(app_name, &n.title)
    {
        notification.title = extraction.cleaned_title;
        notification.workspace = Some(extraction.workspace.clone());

        if let Some(ref mut app) = notification.app {
            let workspace_suffix =
                format!("_{}", extraction.workspace.to_lowercase().replace(' ', "_"));
            app.ident = Arc::from(format!("{}{}", app.ident, workspace_suffix));
            if let Some(ref title) = app.title {
                app.title = Some(Arc::from(format!("{} [{}]", title, extraction.workspace)));
            }
        }
    }

    notification
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
    } else {
        desktop.as_ref().map(|desktop| AppIdent {
            ident: normalize_app_ident(desktop),
            title: Some(desktop.clone()),
        })
    }
}

/// Derive the panel notification TTL.
///
/// - `expire_timeout > 0` → use it (already ms from D-Bus)
/// - `expire_timeout = 0` → None (never expire, from D-Bus -1)
/// - `expire_timeout = -1` → None (server default = no expiration for panel)
fn derive_panel_ttl(notification: &IngressedNotification) -> Option<u64> {
    // Explicit TTL > 0: use it
    if let Some(ttl) = notification.ttl
        && ttl > 0
    {
        return Some(ttl);
    }
    // ttl=0 means "never expire" (from expire_timeout=-1 in DBus)
    // ttl=None means "use server default" = no expiration for panel
    None
}

/// Derive actions from an ingressed notification.
pub fn derive_actions(notification: &IngressedNotification) -> Vec<NotificationAction> {
    let actions = &notification.actions;
    let mut out = Vec::new();
    let mut it = actions.iter();
    while let Some(key) = it.next() {
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
        out.push(NotificationIcon::parse(path));
    }
    if let Some(specific) = &notification.icon {
        out.push(NotificationIcon::parse(specific));
    }

    if let Some(de) = &notification.hints.desktop_entry {
        let trimmed = de.trim();
        if !trimmed.is_empty() {
            let without_suffix = trimmed.strip_suffix(".desktop").unwrap_or(trimmed);
            out.push(NotificationIcon::parse(&Arc::from(without_suffix)));
            out.push(NotificationIcon::parse(&Arc::from(normalize_icon_name(
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

/// Reorder icon hints to prioritize app-level icons for notification groups.
pub fn reorder_icon_hints_for_group(icon_hints: &[NotificationIcon]) -> Vec<NotificationIcon> {
    if icon_hints.is_empty() {
        return Vec::new();
    }

    let (mut app_icons, notif_icons): (Vec<_>, Vec<_>) =
        icon_hints.iter().enumerate().partition(|(idx, _)| {
            let from_end = icon_hints.len() - idx - 1;
            from_end < 3
        });

    app_icons.extend(notif_icons);
    app_icons
        .into_iter()
        .map(|(_, hint)| hint.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::hints::Hints;
    use crate::dbus::ingress::IngressedNotification;
    use crate::types::NotificationUrgency;

    fn make_hints(urgency: NotificationUrgency, resident: bool) -> Hints {
        Hints {
            action_icons: false,
            category: None,
            category_raw: None,
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

    fn make_notification(
        id: u64,
        urgency: NotificationUrgency,
        resident: bool,
    ) -> IngressedNotification {
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

    #[test]
    fn test_ingress_adds_notification_to_store() {
        let mut state = State::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        process_op(&mut state, NotificationOp::Ingress(Box::new(notif)));

        assert!(state.notifications.contains_key(&1));
    }

    #[test]
    fn test_ingress_adds_notification_to_panel() {
        let mut state = State::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        process_op(&mut state, NotificationOp::Ingress(Box::new(notif)));

        assert!(state.panel_notifications.contains_key(&1));
    }

    #[test]
    fn test_dismiss_removes_notification() {
        let mut state = State::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        process_op(&mut state, NotificationOp::Ingress(Box::new(notif)));
        process_op(&mut state, NotificationOp::NotificationDismiss(1));

        assert!(!state.notifications.contains_key(&1));
        assert!(!state.panel_notifications.contains_key(&1));
    }

    #[test]
    fn test_retract_removes_notification() {
        let mut state = State::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);

        process_op(&mut state, NotificationOp::Ingress(Box::new(notif)));
        process_op(&mut state, NotificationOp::NotificationRetract(1));

        assert!(!state.notifications.contains_key(&1));
        assert!(!state.panel_notifications.contains_key(&1));
    }

    #[test]
    fn test_batch_ingress_adds_multiple_notifications() {
        let mut state = State::new();
        let ops = vec![
            NotificationOp::Ingress(Box::new(make_notification(
                1,
                NotificationUrgency::Normal,
                false,
            ))),
            NotificationOp::Ingress(Box::new(make_notification(
                2,
                NotificationUrgency::Normal,
                false,
            ))),
            NotificationOp::Ingress(Box::new(make_notification(
                3,
                NotificationUrgency::Normal,
                false,
            ))),
        ];

        process_op(&mut state, NotificationOp::Batch(ops));

        assert!(state.notifications.contains_key(&1));
        assert!(state.notifications.contains_key(&2));
        assert!(state.notifications.contains_key(&3));
        assert_eq!(state.panel_notifications.len(), 3);
    }

    #[test]
    fn test_set_dnd_operation_updates_state() {
        let mut state = State::new();

        assert!(!state.dnd);

        process_op(&mut state, NotificationOp::SetDnd(true));
        assert!(state.dnd);

        process_op(&mut state, NotificationOp::SetDnd(false));
        assert!(!state.dnd);
    }

    #[test]
    fn test_ttl_expiry_removes_notifications() {
        let mut state = State::new();
        process_op(
            &mut state,
            NotificationOp::Ingress(Box::new(make_notification(
                1,
                NotificationUrgency::Normal,
                false,
            ))),
        );
        process_op(
            &mut state,
            NotificationOp::Ingress(Box::new(make_notification(
                2,
                NotificationUrgency::Normal,
                false,
            ))),
        );
        process_op(
            &mut state,
            NotificationOp::Ingress(Box::new(make_notification(
                3,
                NotificationUrgency::Normal,
                false,
            ))),
        );

        assert_eq!(state.notifications.len(), 3);

        process_op(&mut state, NotificationOp::TtlExpiry(vec![1, 3]));

        assert!(!state.notifications.contains_key(&1));
        assert!(state.notifications.contains_key(&2));
        assert!(!state.notifications.contains_key(&3));
        assert_eq!(state.panel_notifications.len(), 1);
    }

    #[test]
    fn test_ttl_expiry_nonexistent_id_is_noop() {
        let mut state = State::new();
        let changed = process_op(&mut state, NotificationOp::TtlExpiry(vec![999]));
        assert!(!changed);
    }

    #[test]
    fn test_dismiss_nonexistent_id_is_noop() {
        let mut state = State::new();
        let changed = process_op(&mut state, NotificationOp::NotificationDismiss(999));
        assert!(!changed);
    }

    #[test]
    fn test_replaces_id_removes_old_notification() {
        let mut state = State::new();
        process_op(
            &mut state,
            NotificationOp::Ingress(Box::new(make_notification(
                1,
                NotificationUrgency::Normal,
                false,
            ))),
        );

        let mut replacement = make_notification(2, NotificationUrgency::Normal, false);
        replacement.replaces_id = Some(1);
        process_op(&mut state, NotificationOp::Ingress(Box::new(replacement)));

        assert!(!state.notifications.contains_key(&1));
        assert!(state.notifications.contains_key(&2));
        assert_eq!(state.notifications.len(), 1);
    }

    #[test]
    fn test_dismiss_cleans_up_empty_group() {
        let mut state = State::new();
        process_op(
            &mut state,
            NotificationOp::Ingress(Box::new(make_notification(
                1,
                NotificationUrgency::Normal,
                false,
            ))),
        );

        assert!(!state.groups.is_empty());

        process_op(&mut state, NotificationOp::NotificationDismiss(1));

        assert!(state.groups.is_empty());
    }

    #[test]
    fn test_dismiss_keeps_group_with_remaining_notifications() {
        let mut state = State::new();
        process_op(
            &mut state,
            NotificationOp::Ingress(Box::new(make_notification(
                1,
                NotificationUrgency::Normal,
                false,
            ))),
        );
        process_op(
            &mut state,
            NotificationOp::Ingress(Box::new(make_notification(
                2,
                NotificationUrgency::Normal,
                false,
            ))),
        );

        process_op(&mut state, NotificationOp::NotificationDismiss(1));

        assert!(!state.groups.is_empty());
        assert!(state.notifications.contains_key(&2));
    }

    #[test]
    fn test_panel_ttl_respects_explicit_timeout() {
        let mut notif = make_notification(1, NotificationUrgency::Normal, false);
        notif.ttl = Some(5000);

        let mut state = State::new();
        process_op(&mut state, NotificationOp::Ingress(Box::new(notif)));

        let stored = state.notifications.get(&1).unwrap();
        assert_eq!(stored.ttl, Some(5000));
    }

    #[test]
    fn test_panel_ttl_none_for_default() {
        let mut state = State::new();
        let notif = make_notification(1, NotificationUrgency::Normal, false);
        process_op(&mut state, NotificationOp::Ingress(Box::new(notif)));

        let stored = state.notifications.get(&1).unwrap();
        assert_eq!(stored.ttl, None);
    }

    fn make_notification_with_app(
        id: u64,
        app_name: &str,
        urgency: NotificationUrgency,
    ) -> IngressedNotification {
        IngressedNotification {
            app_name: Some(Arc::from(app_name)),
            actions: vec![],
            created_at: SystemTime::now(),
            description: Arc::from("Test description"),
            icon: None,
            id,
            hints: make_hints(urgency, false),
            replaces_id: None,
            title: Arc::from("Test title"),
            ttl: None,
        }
    }

    #[test]
    fn test_ingress_stores_any_app_in_panel() {
        let mut state = State::new();
        let notif = make_notification_with_app(1, "firefox", NotificationUrgency::Normal);

        process_op(&mut state, NotificationOp::Ingress(Box::new(notif)));

        assert!(state.notifications.contains_key(&1));
        assert!(state.panel_notifications.contains_key(&1));
    }
}
