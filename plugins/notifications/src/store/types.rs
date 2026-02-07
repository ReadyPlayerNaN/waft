//! Notification data types.

#![allow(dead_code)] // Many fields and methods are for future UI features

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use super::super::dbus::ingress::IngressedNotification;
use super::super::types::{AppIdent, NotificationAction, NotificationIcon, NotificationUrgency};
use indexmap::{IndexMap, indexmap};

/// A stored notification with all its metadata.
#[derive(Debug, Clone)]
pub struct Notification {
    pub actions: Vec<NotificationAction>,
    pub app: Option<AppIdent>,
    pub created_at: SystemTime,
    pub description: Arc<str>,
    pub icon_hints: Vec<NotificationIcon>,
    pub id: u64,
    pub replaces_id: Option<u64>,
    pub resident: bool,
    pub title: Arc<str>,
    pub ttl: Option<u64>,
    pub toast_ttl: Option<u64>,
    pub urgency: NotificationUrgency,
}

impl Notification {
    pub fn app_ident(&self) -> Arc<str> {
        match &self.app {
            Some(app) => app.ident.clone(),
            None => Arc::from("unknown"),
        }
    }

    pub fn app_title(&self) -> Arc<str> {
        match &self.app {
            Some(app) => match &app.title {
                Some(title) => title.clone(),
                None => Arc::from("Generic"),
            },
            None => Arc::from("Generic"),
        }
    }
}

impl Eq for Notification {}

impl PartialEq for Notification {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.urgency == other.urgency && self.created_at == other.created_at
    }
}

impl Ord for Notification {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.urgency
            .cmp(&other.urgency)
            .then(self.created_at.cmp(&other.created_at))
    }
}

impl PartialOrd for Notification {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Lifecycle state for notifications and groups.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ItemLifecycle {
    Appearing,
    Hiding,
    Hidden,
    Pending,
    Dismissing,
    Dismissed,
    Retracting,
    Retracted,
    Visible,
}

impl ItemLifecycle {
    pub fn is_hidden(&self) -> bool {
        matches!(
            self,
            ItemLifecycle::Hidden
                | ItemLifecycle::Hiding
                | ItemLifecycle::Retracting
                | ItemLifecycle::Retracted
                | ItemLifecycle::Dismissed
                | ItemLifecycle::Dismissing
                | ItemLifecycle::Pending
        )
    }
}

/// A group of notifications from the same application.
#[derive(Debug, Clone)]
pub struct Group {
    id: Arc<str>,
    title: Arc<str>,
    top: IndexMap<u64, ItemLifecycle>,
}

impl Group {
    pub fn new(id: Arc<str>, title: Arc<str>, initial_notif_id: u64) -> Self {
        Self {
            id,
            title,
            top: indexmap! { initial_notif_id => ItemLifecycle::Visible },
        }
    }

    pub fn get_id(&self) -> &Arc<str> {
        &self.id
    }

    pub fn get_title(&self) -> &Arc<str> {
        &self.title
    }

    pub fn get_top(&self) -> &IndexMap<u64, ItemLifecycle> {
        &self.top
    }

    pub fn get_top_mut(&mut self) -> &mut IndexMap<u64, ItemLifecycle> {
        &mut self.top
    }
}

/// Operations that can be dispatched to the notification store.
#[derive(Debug, Clone)]
pub enum NotificationOp {
    Batch(Vec<NotificationOp>),
    Configure {
        toast_limit: usize,
        disable_toasts: bool,
    },
    Ingress(Box<IngressedNotification>),
    NotificationDismiss(u64),
    NotificationDismissed(u64),
    NotificationRetract(u64),
    NotificationRetracted(u64),
    SetDnd(bool),
    Tick,
    ToastHide(u64),
    ToastHidden(u64),
    ToastHoverEnter,
    ToastHoverLeave,
}

/// The notification state container.
#[derive(Debug, Clone)]
pub struct State {
    pub appearing_timestamps: IndexMap<u64, SystemTime>,
    pub archive: IndexMap<Arc<str>, ItemLifecycle>,
    pub dismissing_timestamps: IndexMap<u64, SystemTime>,
    /// Whether toasts are disabled entirely
    pub disable_toasts: bool,
    pub dnd: bool,
    pub groups: HashMap<Arc<str>, Group>,
    pub hiding_timestamps: IndexMap<u64, SystemTime>,
    /// Whether toast timers are paused due to hover
    pub hover_paused: bool,
    pub notifications: HashMap<u64, Notification>,
    pub panel_notifications: IndexMap<u64, ItemLifecycle>,
    /// Timestamp when each panel notification became visible (for TTL tracking)
    pub panel_visible_since_timestamps: IndexMap<u64, SystemTime>,
    pub retracting_timestamps: IndexMap<u64, SystemTime>,
    /// Maximum number of toasts to display at once
    pub toast_limit: usize,
    pub toasts: IndexMap<u64, ItemLifecycle>,
    /// Panel notifications that have expired their TTL (should be removed after animation)
    pub ttl_expired_panel_notifications: std::collections::HashSet<u64>,
    /// Toasts that are hiding due to TTL expiration (should be removed from queue after animation)
    pub ttl_expired_toasts: std::collections::HashSet<u64>,
    pub visible_since_timestamps: IndexMap<u64, SystemTime>,
}

impl State {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self {
            appearing_timestamps: IndexMap::new(),
            archive: IndexMap::new(),
            dismissing_timestamps: IndexMap::new(),
            disable_toasts: false,
            dnd: false,
            groups: HashMap::new(),
            hiding_timestamps: IndexMap::new(),
            hover_paused: false,
            notifications: HashMap::new(),
            panel_notifications: IndexMap::new(),
            panel_visible_since_timestamps: IndexMap::new(),
            retracting_timestamps: IndexMap::new(),
            toast_limit: 3,
            toasts: IndexMap::new(),
            ttl_expired_panel_notifications: std::collections::HashSet::new(),
            ttl_expired_toasts: std::collections::HashSet::new(),
            visible_since_timestamps: IndexMap::new(),
        }
    }

    pub fn get_notification(&self, id: &u64) -> Option<&Notification> {
        self.notifications.get(id)
    }

    pub fn get_toasts(&self) -> Vec<(&Notification, &ItemLifecycle)> {
        self.toasts
            .iter()
            .filter_map(|(k, l)| self.notifications.get(k).map(|n| (n, l)))
            .collect()
    }

    /// Get panel notifications with their lifecycle state.
    pub fn get_panel_notifications(&self) -> Vec<(&Notification, &ItemLifecycle)> {
        self.panel_notifications
            .iter()
            .filter_map(|(k, l)| self.notifications.get(k).map(|n| (n, l)))
            .collect()
    }

    /// Get notifications grouped by app identifier.
    pub fn get_grouped_notifications(
        &self,
    ) -> HashMap<Arc<str>, Vec<(&Notification, &ItemLifecycle)>> {
        let mut grouped: HashMap<Arc<str>, Vec<(&Notification, &ItemLifecycle)>> = HashMap::new();
        let mut missing_count = 0;

        for (id, lifecycle) in &self.panel_notifications {
            if let Some(notification) = self.notifications.get(id) {
                let app_ident = notification.app_ident();
                grouped
                    .entry(app_ident)
                    .or_default()
                    .push((notification, lifecycle));
            } else {
                log::warn!(
                    "[get_grouped_notifications] Notification {} in panel_notifications not found in notifications HashMap",
                    id
                );
                missing_count += 1;
            }
        }

        if missing_count > 0 {
            log::warn!(
                "[get_grouped_notifications] {} notifications missing from HashMap out of {} panel_notifications",
                missing_count,
                self.panel_notifications.len()
            );
        }

        // Sort notifications within each group by creation time (newest first)
        for notifications in grouped.values_mut() {
            notifications.sort_by(|a, b| b.0.created_at.cmp(&a.0.created_at));
        }

        grouped
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
