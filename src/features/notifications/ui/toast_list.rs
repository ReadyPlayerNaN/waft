use log::info;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::SystemTime;

use adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::gtk;
use relm4::prelude::*;

use super::super::types::{NotificationDisplay, NotificationUrgency};
use super::card::{
    NotificationCard, NotificationCardInit, NotificationCardInput, NotificationCardOutput,
    NotificationContentUpdate,
};

pub struct ToastList {
    // Currently unused by this view, but kept for future expand/collapse UI
    expanded: bool,

    id: Arc<str>,
    title: Arc<str>,

    /// Full notification state (source of truth), sorted:
    /// - urgency: Critical first, others equal
    /// - created_at: DESC
    /// - id: DESC (tie-break)
    notifications: VecDeque<Arc<NotificationDisplay>>,

    /// Stores only the currently displayed notifications (max 3).
    /// IMPORTANT: never clear and rebuild during normal operation.
    notifications_list: FactoryVecDeque<NotificationCard>,
}

pub struct ToastListInit {
    pub notifications: Option<Vec<Arc<NotificationDisplay>>>,
    pub id: Arc<str>,
    pub title: Arc<str>,
}

#[derive(Debug, Clone)]
pub enum ToastListInput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    Ingest(Arc<NotificationDisplay>),
    Remove(u64),
    TimedOut(u64),
}

#[derive(Debug, Clone)]
pub enum ToastListOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    TimedOut(u64),
    Collapse(Arc<str>),
    Expand(Arc<str>),
}

fn transform_notification_card_outputs(msg: NotificationCardOutput) -> ToastListInput {
    match msg {
        NotificationCardOutput::ActionClick(id, action) => ToastListInput::ActionClick(id, action),
        NotificationCardOutput::CardClick(id) => ToastListInput::CardClick(id),
        NotificationCardOutput::Close(id) => ToastListInput::CardClose(id),
        NotificationCardOutput::TimedOut(id) => ToastListInput::TimedOut(id),
    }
}

#[relm4::component(pub)]
impl SimpleComponent for ToastList {
    type Init = ToastListInit;
    type Input = ToastListInput;
    type Output = ToastListOutput;
    type Widgets = ToastListWidgets;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,

            #[local_ref]
            notifications_container -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
            },
        }
    }

    fn init(
        value: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut notifications: VecDeque<Arc<NotificationDisplay>> = match value.notifications {
            Some(n) => VecDeque::from(n),
            None => VecDeque::new(),
        };

        // Ensure initial state is correctly sorted + unique by id (last write wins).
        // Note: input should typically already be unique, but we enforce it defensively.
        ToastList::dedup_by_id_keep_last(&mut notifications);
        ToastList::sort_notifications_in_place(&mut notifications);

        let notifications_list = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_card_outputs);

        let mut model = Self {
            expanded: false,
            id: value.id,
            title: value.title,
            notifications,
            notifications_list,
        };

        let notifications_container = model.notifications_list.widget();
        let widgets = view_output!();

        // Init: fill up to MAX_VISIBLE_NOTIFICATIONS without ever clearing (factory is empty here).
        model.init_visible_notifications();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::ActionClick(notification_id, action) => {
                let _ = sender.output(Self::Output::ActionClick(notification_id, action));
            }
            Self::Input::CardClick(notification_id) => {
                let _ = sender.output(Self::Output::CardClick(notification_id));
            }
            Self::Input::CardClose(notification_id) => {
                // Plugin is the source of truth: emit intent to close, but don't mutate local state here.
                let _ = sender.output(Self::Output::CardClose(notification_id));
            }
            Self::Input::TimedOut(notification_id) => {
                info!("Notification {} timed out.", notification_id);
                // Plugin is the source of truth: emit timeout, plugin decides whether to remove.
                let _ = sender.output(Self::Output::TimedOut(notification_id));
            }
            Self::Input::Ingest(notification) => {
                info!("Ingesting notification: {}", notification.id);
                self.ingest_notification(notification);
            }
            Self::Input::Remove(notification_id) => {
                info!("Removing notification: {}", notification_id);
                self.remove_notification(notification_id);
            }
        }
    }
}

impl ToastList {
    const MAX_VISIBLE_NOTIFICATIONS: usize = 3;

    // ----------------------------
    // State ordering + normalization
    // ----------------------------

    fn urgency_rank(u: NotificationUrgency) -> u8 {
        // "URGENT first, others equal"
        match u {
            NotificationUrgency::Critical => 1,
            NotificationUrgency::Normal | NotificationUrgency::Low => 0,
        }
    }

    fn systemtime_cmp_desc(a: &SystemTime, b: &SystemTime) -> std::cmp::Ordering {
        // SystemTime doesn't implement Ord; compare using duration from UNIX_EPOCH when possible.
        //
        // If conversion fails (time before UNIX_EPOCH), fall back to "equal" rather than panicking.
        let a = a.duration_since(SystemTime::UNIX_EPOCH);
        let b = b.duration_since(SystemTime::UNIX_EPOCH);

        match (a, b) {
            (Ok(da), Ok(db)) => db.cmp(&da),
            _ => std::cmp::Ordering::Equal,
        }
    }

    fn sort_notifications_in_place(notifications: &mut VecDeque<Arc<NotificationDisplay>>) {
        if notifications.len() <= 1 {
            return;
        }

        let mut v: Vec<Arc<NotificationDisplay>> = notifications.drain(..).collect();
        v.sort_by(|a, b| {
            let ur = Self::urgency_rank(b.urgency).cmp(&Self::urgency_rank(a.urgency));
            if ur != std::cmp::Ordering::Equal {
                return ur;
            }

            let c = Self::systemtime_cmp_desc(&a.created_at, &b.created_at);
            if c != std::cmp::Ordering::Equal {
                return c;
            }

            // Tie-break for determinism.
            b.id.cmp(&a.id)
        });

        *notifications = VecDeque::from(v);
    }

    fn dedup_by_id_keep_last(notifications: &mut VecDeque<Arc<NotificationDisplay>>) {
        // O(n^2) is fine here (notification counts are small).
        let mut out: VecDeque<Arc<NotificationDisplay>> =
            VecDeque::with_capacity(notifications.len());
        while let Some(n) = notifications.pop_front() {
            // Remove any existing occurrence of the same id (keep the newest "write" we see).
            out.retain(|x| x.id != n.id);
            out.push_back(n);
        }
        *notifications = out;
    }

    // ----------------------------
    // Factory synchronization (NO CLEAR)
    // ----------------------------

    fn desired_top_notifications(&self) -> Vec<Arc<NotificationDisplay>> {
        self.notifications
            .iter()
            .take(Self::MAX_VISIBLE_NOTIFICATIONS)
            .cloned()
            .collect()
    }

    fn init_visible_notifications(&mut self) {
        // Factory is empty here. Just add desired top3 in correct order (newest/first at top).
        let desired = self.desired_top_notifications();

        // Build so desired[0] ends up at front/top.
        let mut guard = self.notifications_list.guard();
        for n in desired.into_iter().rev() {
            guard.push_front(NotificationCardInit {
                countdown: false,
                notification: n,
            });
        }
    }

    fn ingest_notification(&mut self, notification: Arc<NotificationDisplay>) {
        // 1) Reconcile state: replace by id (no duplicates), then sort.
        let id = notification.id;
        self.notifications.retain(|n| n.id != id);
        self.notifications.push_front(notification);
        Self::sort_notifications_in_place(&mut self.notifications);

        // 2) Reconcile top3 view with minimal ops (no clearing).
        self.reconcile_visible_top3();
    }

    fn remove_notification(&mut self, notification_id: u64) {
        let before = self.notifications.len();
        self.notifications.retain(|n| n.id != notification_id);

        // Nothing to do if not present in state and therefore cannot affect top3.
        if self.notifications.len() == before {
            return;
        }

        // State ordering remains valid after removals.
        self.reconcile_visible_top3();
    }

    fn reconcile_visible_top3(&mut self) {
        // Goal: Make the factory exactly reflect the first up-to-3 notifications in `self.notifications`,
        // using only removals/insertions (no clear).
        //
        // We prefer correctness + bounded churn over any full rebuild.
        let desired = self.desired_top_notifications();
        let desired_ids: Vec<u64> = desired.iter().map(|n| n.id).collect();

        let mut guard = self.notifications_list.guard();

        // Step 1: remove anything currently displayed that shouldn't be displayed at all.
        // Iterate from back to front to keep indices stable.
        let mut idx = guard.len();
        while idx > 0 {
            idx -= 1;
            let id = guard
                .get(idx)
                .map(|c| c.notification.id)
                .unwrap_or_default();
            if !desired_ids.contains(&id) {
                let _ = guard.remove(idx);
            }
        }

        // Step 2: enforce correct order and exact content, position-by-position.
        // For any mismatch at position i, remove existing at i (if any) and insert desired[i].
        for (i, desired_n) in desired.iter().enumerate() {
            let matches = guard
                .get(i)
                .map(|c| c.notification.id == desired_n.id)
                .unwrap_or(false);

            if matches {
                // The id matches, but the notification payload might have been replaced in state.
                // Refresh the existing card in-place to avoid tearing down/recreating the component.
                if let Some(card) = guard.get(i) {
                    if !Arc::ptr_eq(&card.notification, desired_n) {
                        guard.send(
                            i,
                            NotificationCardInput::Content(NotificationContentUpdate {
                                notification: desired_n.clone(),
                            }),
                        );
                    }
                }
                continue;
            }

            // If the desired id exists elsewhere in the visible list, remove it first to avoid duplicates.
            let existing_pos = guard.iter().position(|c| c.notification.id == desired_n.id);
            if let Some(pos) = existing_pos {
                let _ = guard.remove(pos);
            }

            // If there is a wrong element at i, remove it.
            if i < guard.len() {
                let _ = guard.remove(i);
            }

            // Insert the desired notification at i.
            guard.insert(
                i,
                NotificationCardInit {
                    countdown: false,
                    notification: desired_n.clone(),
                },
            );
        }

        // Step 3: if we have extra items (possible if desired shrank), trim from bottom.
        while guard.len() > desired.len() {
            let _ = guard.pop_back();
        }
    }
}
