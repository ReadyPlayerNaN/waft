use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use indexmap::{IndexMap, indexmap};
use relm4::{AsyncReducer, AsyncReducible};

use super::dbus::ingress::IngressedNotification;
use super::types::{AppIdent, NotificationAction, NotificationIcon, NotificationUrgency};

#[derive(Debug, Clone)]
pub struct Notification {
    pub actions: Vec<NotificationAction>,
    pub app: Option<AppIdent>,
    pub created_at: SystemTime,
    pub description: Arc<str>,
    pub icon_hints: Vec<NotificationIcon>,
    pub id: u64,
    pub replaces_id: Option<u64>,
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

#[derive(Debug, Clone)]
pub struct Group {
    id: Arc<str>,
    title: Arc<str>,
    top: IndexMap<u64, ItemLifecycle>,
}

impl Group {
    pub fn get_id(&self) -> &Arc<str> {
        &self.id
    }
    pub fn get_title(&self) -> &Arc<str> {
        &self.title
    }
    pub fn get_top(&self) -> &IndexMap<u64, ItemLifecycle> {
        &self.top
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub appearing_timestamps: IndexMap<u64, SystemTime>,
    pub archive: IndexMap<Arc<str>, ItemLifecycle>,
    pub dismissing_timestamps: IndexMap<u64, SystemTime>,
    pub groups: HashMap<Arc<str>, Group>,
    pub hiding_timestamps: IndexMap<u64, SystemTime>,
    pub notifications: HashMap<u64, Notification>,
    pub retracting_timestamps: IndexMap<u64, SystemTime>,
    pub toasts: IndexMap<u64, ItemLifecycle>,
}

impl State {
    pub fn get_notification(&self, id: &u64) -> Option<&Notification> {
        self.notifications.get(&id)
    }

    pub fn get_notification_lifecycle(
        &self,
        group_id: &Option<Arc<str>>,
        id: &u64,
    ) -> Option<&ItemLifecycle> {
        if let Some(group_id) = group_id {
            self.groups
                .get(group_id)
                .and_then(|group| group.get_top().get(id).or(Some(&ItemLifecycle::Visible)))
        } else {
            self.toasts.get(id)
        }
    }

    pub fn get_toasts(&self) -> Vec<(&Notification, &ItemLifecycle)> {
        self.toasts
            .iter()
            .map(|(k, l)| (self.notifications.get(&k), l))
            .filter(|(n, _)| n.is_some())
            .map(|(n, l)| (n.unwrap(), l))
            .collect::<Vec<(&Notification, &ItemLifecycle)>>()
    }

    pub fn get_groups(&self) -> Vec<(&Group, &ItemLifecycle)> {
        self.archive
            .iter()
            .map(|(k, l)| (self.groups.get(k), l))
            .filter(|(n, _)| n.is_some())
            .map(|(n, l)| (n.unwrap(), l))
            .collect::<Vec<(&Group, &ItemLifecycle)>>()
    }

    pub fn get_group(&self, id: &Arc<str>) -> Option<&Group> {
        self.groups.get(id)
    }

    pub fn get_group_top(&self, group_id: &Arc<str>) -> Vec<(&Notification, &ItemLifecycle)> {
        if let Some(group) = self.groups.get(group_id) {
            group
                .get_top()
                .iter()
                .map(|(k, l)| (self.notifications.get(&k), l))
                .filter(|(n, _)| n.is_some())
                .map(|(n, l)| (n.unwrap(), l))
                .collect::<Vec<(&Notification, &ItemLifecycle)>>()
        } else {
            vec![]
        }
    }

    pub fn get_group_bottom(&self, group_id: &Arc<str>) -> Vec<(&Notification, &ItemLifecycle)> {
        let gid = group_id.clone();
        if let Some(group) = self.groups.get(group_id) {
            let top = group.get_top();
            let mut list = self
                .notifications
                .values()
                .filter(|n| n.app_ident() == gid && !top.contains_key(&n.id))
                .map(|n| (n, &ItemLifecycle::Visible))
                .collect::<Vec<(&Notification, &ItemLifecycle)>>();
            list.sort_by(|(a, _), (b, _)| a.cmp(&b));
            list
        } else {
            vec![]
        }
    }

    pub fn get_hiding_toasts(&self) -> Vec<(u64, SystemTime)> {
        self.toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Hiding))
            .filter_map(|(id, _)| {
                self.hiding_timestamps
                    .get(id)
                    .map(|timestamp| (*id, *timestamp))
            })
            .collect()
    }

    fn process_tick(&mut self) -> Vec<NotificationOp> {
        let now = SystemTime::now();
        self.toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Hiding))
            .filter_map(|(id, _)| {
                self.hiding_timestamps.get(id).and_then(|hiding_since| {
                    if now.duration_since(*hiding_since).unwrap_or_default()
                        >= Duration::from_millis(250)
                    {
                        Some(NotificationOp::ToastHidden(*id))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

pub struct Reducer(State);

#[derive(Debug, Clone)]
pub enum NotificationOp {
    Ingress(IngressedNotification),
    NotificationDismiss(u64),
    NotificationDismissed(u64),
    NotificationRetract(u64),
    NotificationRetracted(u64),
    Tick,
    ToastHide(u64),
    ToastHidden(u64),
    Batch(Vec<NotificationOp>),
}

fn normalize_app_ident(app_ident: &str) -> Arc<str> {
    Arc::from(app_ident.to_lowercase().replace(' ', "_"))
}

fn derive_app_ident(notification: &IngressedNotification) -> Option<AppIdent> {
    let app_ident = &notification.app_name;
    let desktop = &notification.hints.desktop_entry;

    if let Some(app_ident) = app_ident {
        Some(AppIdent {
            ident: normalize_app_ident(&app_ident),
            // @TODO: Try to read the app title based on identification
            title: Some(app_ident.clone()),
        })
    } else if let Some(desktop) = desktop {
        // @TODO: Try to resolve the desktop entry
        Some(AppIdent {
            ident: normalize_app_ident(&desktop),
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

pub fn derive_actions(notification: &IngressedNotification) -> Vec<NotificationAction> {
    let actions = &notification.actions;
    let mut out = Vec::new();
    let mut it = actions.into_iter();
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
        } else {
            // drop punctuation and other symbols
        }
    }
    if out.is_empty() {
        input.to_ascii_lowercase()
    } else {
        out
    }
}

pub fn derive_icon_hints(notification: &IngressedNotification) -> Vec<NotificationIcon> {
    let mut out = Vec::new();
    if let Some(bytes) = &notification.hints.image_data {
        out.push(NotificationIcon::Bytes(bytes.clone()));
    }
    if let Some(path) = &notification.hints.image_path {
        out.push(NotificationIcon::FilePath(Arc::new(PathBuf::from(
            path.as_ref(),
        ))));
    }
    if let Some(specific) = &notification.icon {
        out.push(NotificationIcon::from_str(specific));
    }

    if let Some(de) = &notification.hints.desktop_entry {
        let trimmed = de.trim();
        if !trimmed.is_empty() {
            // Typical desktop-entry: "org.gnome.Nautilus.desktop" -> "org.gnome.Nautilus".
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
    // For the moment, maximum one visible (hottest items are at the bottom of the IndexMap).
    //
    // Slot rules:
    // - Only `Visible` or `Appearing` counts as a filled slot.
    // - Pick the hottest candidates regardless of current lifecycle (Visible/Appearing/Pending/Hidden/Hiding).
    //   This allows newly-added hotter notifications to take a slot even if older ones are currently `Visible`.
    // - `Dismissed` cards never come back; they are removed from `top`.
    // - `Dismissing` / `Retracting` are transitional: they do not count toward the limit and are not promoted/removed here.
    if top.is_empty() || limit == 0 {
        return;
    }

    let mut selected: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut filled = 0usize;

    // Select the hottest candidates regardless of current lifecycle.
    //
    // `top` is sorted coldest -> hottest; iterate in reverse to pick hottest first.
    // Transitional lifecycles (`Dismissing` / `Retracting`) are kept but do not count
    // toward the limit and are not promoted/removed here.
    for (id, lifecycle) in top.iter().rev() {
        if filled >= limit {
            break;
        }
        match lifecycle {
            ItemLifecycle::Dismissed | ItemLifecycle::Retracted => {
                // Never re-selected.
            }
            ItemLifecycle::Dismissing | ItemLifecycle::Retracting => {
                // Transitional: ignore for slot filling.
            }
            _ => {
                selected.insert(*id);
                filled += 1;
            }
        }
    }

    let mut to_remove: Vec<u64> = Vec::new();

    for (id, lifecycle) in top.iter_mut() {
        // `Dismissed` never comes back.
        if matches!(lifecycle, ItemLifecycle::Dismissed) {
            to_remove.push(*id);
            continue;
        }

        // `Dismissing` is transitional: don't count it, don't promote it, don't remove it here.
        if matches!(lifecycle, ItemLifecycle::Dismissing)
            || matches!(lifecycle, ItemLifecycle::Retracting)
        {
            continue;
        }

        if selected.contains(id) {
            // Anything that ends up "within limit" starts as Appearing (with animation).
            // Already Visible or Appearing items stay as they are.
            if !matches!(lifecycle, ItemLifecycle::Visible | ItemLifecycle::Appearing) {
                *lifecycle = ItemLifecycle::Appearing;
                appearing_timestamps.insert(*id, SystemTime::now());
            }
            continue;
        }

        // Above the limit.
        match lifecycle {
            ItemLifecycle::Hiding => {
                // no change - let hiding animation complete
            }
            ItemLifecycle::Hidden | ItemLifecycle::Pending => {
                // Keep hidden/pending items so they can be promoted when a slot opens
            }
            ItemLifecycle::Visible | ItemLifecycle::Appearing => {
                *lifecycle = ItemLifecycle::Hiding;
                appearing_timestamps.shift_remove(id);
                hiding_timestamps.insert(*id, SystemTime::now());
            }
            ItemLifecycle::Dismissing | ItemLifecycle::Retracting => {
                // handled above; keep here for exhaustiveness in case the control-flow changes
            }
            ItemLifecycle::Dismissed | ItemLifecycle::Retracted => {
                // handled above, but keep exhaustive in case the enum changes
                to_remove.push(*id);
            }
        }
    }

    // Remove after the mutation pass to avoid borrow issues; preserve order.
    for id in to_remove {
        top.shift_remove(&id);
    }
}

fn reconcile_group_state_on_ingress(
    state: &mut State,
    notif_id: u64,
    group_id: Arc<str>,
    app_title: Arc<str>,
) {
    if let Some(group) = state.groups.get_mut(&group_id) {
        group.top.insert(notif_id.clone(), ItemLifecycle::Visible);
        sort_notif_list(&state.notifications, &mut group.top);
        // Groups don't use appearing/hiding timestamps (no animation)
        let mut stub_appearing = IndexMap::new();
        let mut stub_hiding = IndexMap::new();
        cut_notif_ids(&mut group.top, &mut stub_appearing, &mut stub_hiding, 1);
    } else {
        // @TODO: Sort groups after insertion
        state.groups.insert(
            group_id.clone(),
            Group {
                title: app_title,
                id: group_id.clone(),
                top: indexmap! {notif_id.clone() => ItemLifecycle::Visible},
            },
        );
    }
    // Don't forget to make the group visible
    // @TODO: Sort groups after potential insertion
    state
        .archive
        .insert(group_id.clone(), ItemLifecycle::Visible);
}

fn reconcile_toast_state_on_ingress(state: &mut State, notif_id: u64) {
    if state.toasts.len() == 0 {
        // First toast starts with Appearing animation
        state
            .toasts
            .insert(notif_id.clone(), ItemLifecycle::Appearing);
        state
            .appearing_timestamps
            .insert(notif_id, SystemTime::now());
    } else {
        state
            .toasts
            .insert(notif_id.clone(), ItemLifecycle::Pending);
        sort_notif_list(&state.notifications, &mut state.toasts);
        cut_notif_ids(
            &mut state.toasts,
            &mut state.appearing_timestamps,
            &mut state.hiding_timestamps,
            5,
        );
    }
}

impl Reducer {
    pub fn get_state(&self) -> &State {
        &self.0
    }

    fn process_tick(&mut self) -> Vec<NotificationOp> {
        let now = SystemTime::now();
        let animation_duration = Duration::from_millis(250);

        // Collect Hiding → Hidden transitions
        let hiding_to_hidden: Vec<_> = self
            .0
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Hiding))
            .filter_map(|(toast_id, _)| {
                self.0.hiding_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Collect Appearing → Visible transitions
        let appearing_to_visible: Vec<_> = self
            .0
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Appearing))
            .filter_map(|(toast_id, _)| {
                self.0.appearing_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Collect Dismissing → Dismissed transitions
        let dismissing_to_dismissed: Vec<_> = self
            .0
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Dismissing))
            .filter_map(|(toast_id, _)| {
                self.0.dismissing_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Collect Retracting → Retracted transitions
        let retracting_to_retracted: Vec<_> = self
            .0
            .toasts
            .iter()
            .filter(|(_, lifecycle)| matches!(lifecycle, ItemLifecycle::Retracting))
            .filter_map(|(toast_id, _)| {
                self.0.retracting_timestamps.get(toast_id).and_then(|ts| {
                    if now.duration_since(*ts).unwrap_or_default() >= animation_duration {
                        Some(*toast_id)
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Apply simple state transitions
        for toast_id in hiding_to_hidden {
            self.0.toasts.insert(toast_id, ItemLifecycle::Hidden);
            self.0.hiding_timestamps.shift_remove(&toast_id);
        }

        for toast_id in appearing_to_visible {
            self.0.toasts.insert(toast_id, ItemLifecycle::Visible);
            self.0.appearing_timestamps.shift_remove(&toast_id);
        }

        // Reconcile toasts after hiding transitions
        sort_notif_list(&self.0.notifications, &mut self.0.toasts);
        cut_notif_ids(
            &mut self.0.toasts,
            &mut self.0.appearing_timestamps,
            &mut self.0.hiding_timestamps,
            5,
        );

        // Return operations that need full processing
        let mut follow_up_ops = Vec::new();
        for toast_id in dismissing_to_dismissed {
            self.0.dismissing_timestamps.shift_remove(&toast_id);
            follow_up_ops.push(NotificationOp::NotificationDismissed(toast_id));
        }
        for toast_id in retracting_to_retracted {
            self.0.retracting_timestamps.shift_remove(&toast_id);
            follow_up_ops.push(NotificationOp::NotificationRetracted(toast_id));
        }

        follow_up_ops
    }

    async fn process_single_op(&mut self, input: NotificationOp) -> bool {
        match input {
            NotificationOp::Tick => {
                let follow_up_ops = self.process_tick();
                let mut changed = false;
                // Process follow-up operations without recursion using Box::pin
                for op in follow_up_ops {
                    changed |= Box::pin(self.process_single_op(op)).await;
                }
                changed
            }
            NotificationOp::Ingress(n) => {
                let notification = Notification {
                    actions: derive_actions(&n),
                    app: derive_app_ident(&n),
                    created_at: n.created_at,
                    description: n.description.clone(),
                    icon_hints: derive_icon_hints(&n),
                    id: n.id,
                    replaces_id: n.replaces_id,
                    title: n.title.clone(),
                    ttl: n.ttl,
                    toast_ttl: derive_toast_ttl(&n),
                    urgency: n.hints.urgency,
                };
                let notif_id = notification.id;
                let group_id = notification.app_ident();
                let app_title = notification.app_title();
                self.0.notifications.insert(notif_id, notification);
                reconcile_group_state_on_ingress(&mut self.0, notif_id, group_id, app_title);
                reconcile_toast_state_on_ingress(&mut self.0, notif_id);
                true
            }
            NotificationOp::NotificationDismiss(id) => {
                if let Some(notification) = self.0.notifications.get(&id) {
                    let group_id = notification.app_ident();
                    if let Some(group) = self.0.groups.get_mut(group_id.as_ref()) {
                        group
                            .top
                            .insert(notification.id.clone(), ItemLifecycle::Dismissing);
                    }
                    self.0.toasts.insert(id, ItemLifecycle::Dismissing);
                    self.0.dismissing_timestamps.insert(id, SystemTime::now());
                }
                true
            }
            NotificationOp::NotificationRetract(id) => {
                if let Some(notification) = self.0.notifications.get(&id) {
                    let group_id = notification.app_ident();
                    if let Some(group) = self.0.groups.get_mut(group_id.as_ref()) {
                        group
                            .top
                            .insert(notification.id.clone(), ItemLifecycle::Retracting);
                    }
                    self.0.toasts.insert(id, ItemLifecycle::Retracting);
                    self.0.retracting_timestamps.insert(id, SystemTime::now());
                }
                true
            }
            NotificationOp::NotificationDismissed(id) => {
                let group_id = self.0.notifications.get(&id).map(|n| n.app_ident());

                for group in self.0.groups.values_mut() {
                    group.top.shift_remove(&id);
                }
                self.0.toasts.shift_remove(&id);
                self.0.appearing_timestamps.shift_remove(&id);
                self.0.dismissing_timestamps.shift_remove(&id);
                self.0.hiding_timestamps.shift_remove(&id);
                let _ = self.0.notifications.remove(&id);

                // Promote pending toasts to fill the empty slot
                sort_notif_list(&self.0.notifications, &mut self.0.toasts);
                cut_notif_ids(
                    &mut self.0.toasts,
                    &mut self.0.appearing_timestamps,
                    &mut self.0.hiding_timestamps,
                    5,
                );

                if let Some(group_id) = group_id {
                    let group_has_any = self
                        .0
                        .notifications
                        .values()
                        .any(|n| n.app_ident() == group_id);

                    if !group_has_any {
                        self.0.archive.insert(group_id, ItemLifecycle::Dismissing);
                    }
                }
                true
            }
            NotificationOp::NotificationRetracted(id) => {
                let group_id = self.0.notifications.get(&id).map(|n| n.app_ident());

                for group in self.0.groups.values_mut() {
                    group.top.shift_remove(&id);
                }
                self.0.toasts.shift_remove(&id);
                self.0.appearing_timestamps.shift_remove(&id);
                self.0.hiding_timestamps.shift_remove(&id);
                self.0.retracting_timestamps.shift_remove(&id);
                let _ = self.0.notifications.remove(&id);

                // Promote pending toasts to fill the empty slot
                sort_notif_list(&self.0.notifications, &mut self.0.toasts);
                cut_notif_ids(
                    &mut self.0.toasts,
                    &mut self.0.appearing_timestamps,
                    &mut self.0.hiding_timestamps,
                    5,
                );

                if let Some(group_id) = group_id {
                    let group_has_any = self
                        .0
                        .notifications
                        .values()
                        .any(|n| n.app_ident() == group_id);

                    if !group_has_any {
                        self.0.archive.insert(group_id, ItemLifecycle::Dismissing);
                    }
                }
                true
            }
            NotificationOp::ToastHide(id) => {
                self.0.toasts.insert(id, ItemLifecycle::Hiding);
                self.0.hiding_timestamps.insert(id, SystemTime::now());
                true
            }
            NotificationOp::ToastHidden(id) => {
                println!("HIDE TOAST {:?}", id);
                self.0.toasts.insert(id, ItemLifecycle::Hidden);
                self.0.hiding_timestamps.shift_remove(&id);

                // Promote pending toasts to fill the empty slot
                sort_notif_list(&self.0.notifications, &mut self.0.toasts);
                cut_notif_ids(
                    &mut self.0.toasts,
                    &mut self.0.appearing_timestamps,
                    &mut self.0.hiding_timestamps,
                    5,
                );
                true
            }
            NotificationOp::Batch(_ops) => {
                // Unsupported
                println!("Unsupported batch operation");
                false
            }
        }
    }
}

impl AsyncReducible for Reducer {
    type Input = NotificationOp;

    async fn init() -> Self {
        Self(State {
            appearing_timestamps: IndexMap::new(),
            archive: IndexMap::new(),
            dismissing_timestamps: IndexMap::new(),
            groups: HashMap::new(),
            hiding_timestamps: IndexMap::new(),
            notifications: HashMap::new(),
            retracting_timestamps: IndexMap::new(),
            toasts: IndexMap::new(),
        })
    }

    async fn reduce(&mut self, input: Self::Input) -> bool {
        let res = match input {
            NotificationOp::Batch(ops) => {
                // Process all operations in the batch
                for op in ops {
                    self.process_single_op(op).await;
                }
                true
            }
            op => {
                // Process single operation
                self.process_single_op(op).await
            }
        };
        res
    }
}

pub static REDUCER: AsyncReducer<Reducer> = AsyncReducer::new();
