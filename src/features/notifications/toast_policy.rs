//! Pure toast state + dismissal policy (no GTK).
//!
//! This module extracts the "toast popup" state transitions out of the notifications plugin so
//! it can be unit-tested without a GTK main loop.
//!
//! Design goals:
//! - No GTK/glib dependencies.
//! - Explicit policy decisions (timeout vs user dismissal) are represented as commands.
//! - Keep the data model simple: the toast list is derived from a stack of notification ids
//!   (most-recent-first) plus per-id toast metadata.
//!
//! Intended integration pattern (plugin-side):
//! - Maintain a `ToastState` instance.
//! - On DBus `Notify`, call `push(...)`.
//! - On user dismissal (toast close button / overlay dismiss), call `dismiss_user(...)` / `dismiss_overlay(...)`.
//! - Periodically (or via timer wheel) call `expire_due(now)` to get ids that should expire.
//! - Keep overlay history/model separate: expiry MUST NOT remove from overlay history, only from toasts.
//! - For rendering, call `visible_items(now)` to get per-id progress metadata (elapsed/ttl).
//!
//! Notes:
//! - We intentionally store `ToastEntry` metadata separately from the notification model.
//! - If you later want to change the "max toast stack memory" (not just visible count),
//!   adjust `max_stack_len`.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Render item for the toast view layer.
///
/// This intentionally lives in the toast policy module so the GTK view can be "dumb":
/// it renders a list of items and emits events, while all timing/progress math lives in pure state.
#[derive(Debug, Clone)]
pub struct ToastRenderItem<T> {
    pub id: NotificationId,
    pub payload: T,
    pub ttl: Option<Duration>,
    pub elapsed: Duration,
}

/// Pure per-toast progress metadata (no GTK).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastProgress {
    pub id: NotificationId,
    pub ttl: Option<Duration>,
    pub elapsed: Duration,
}

/// Stable toast id type (matches `Notification.id` in the rest of the codebase).
pub type NotificationId = u64;

/// Notification urgency as it impacts toast auto-dismiss policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Default for Urgency {
    fn default() -> Self {
        Self::Normal
    }
}

/// Toast TTL policy.
///
/// - `default_ttl`: used when there are no actions
/// - `actions_ttl`: used when actions exist
/// - `Critical` urgency overrides TTL (never auto-dismiss)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastPolicy {
    pub default_ttl: Duration,
    pub actions_ttl: Duration,
}

impl Default for ToastPolicy {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_secs(8),
            actions_ttl: Duration::from_secs(16),
        }
    }
}

/// Metadata needed to decide toast expiry and behavior.
///
/// This is intentionally small and independent from the full notification payload.
/// The plugin keeps full notification data in its model/history.
#[derive(Debug, Clone)]
pub struct ToastEntry {
    pub urgency: Urgency,
    pub has_actions: bool,

    /// Monotonic "created" timestamp used for expiry (Instant is monotonic).
    pub created_at: Instant,

    /// Accumulated paused time (for "pause all timers on hover" or "pause while overlay visible").
    pub paused_total: Duration,

    /// If paused, the time pause started (monotonic).
    pub paused_since: Option<Instant>,
}

impl ToastEntry {
    pub fn new(_id: NotificationId, urgency: Urgency, has_actions: bool, now: Instant) -> Self {
        Self {
            urgency,
            has_actions,
            created_at: now,
            paused_total: Duration::from_millis(0),
            paused_since: None,
        }
    }

    pub fn ttl(&self, policy: ToastPolicy) -> Option<Duration> {
        if self.urgency == Urgency::Critical {
            return None;
        }
        if self.has_actions {
            Some(policy.actions_ttl)
        } else {
            Some(policy.default_ttl)
        }
    }

    pub fn pause(&mut self, now: Instant) {
        if self.paused_since.is_none() {
            self.paused_since = Some(now);
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if let Some(since) = self.paused_since.take() {
            self.paused_total += now.saturating_duration_since(since);
        }
    }

    /// Elapsed time excluding pauses.
    pub fn effective_elapsed(&self, now: Instant) -> Duration {
        let base_elapsed = if let Some(since) = self.paused_since {
            since.saturating_duration_since(self.created_at)
        } else {
            now.saturating_duration_since(self.created_at)
        };

        base_elapsed.saturating_sub(self.paused_total)
    }

    pub fn is_due(&self, now: Instant, policy: ToastPolicy) -> bool {
        match self.ttl(policy) {
            None => false,
            Some(ttl) => self.effective_elapsed(now) >= ttl,
        }
    }
}

/// Resulting action commands from toast state transitions.
///
/// The plugin should interpret these commands:
/// - `ExpireToastOnly`: remove from toast popup/stack only; DO NOT remove from overlay history.
/// - `DismissGlobally`: remove from overlay history/model AND toast popup (global dismissal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastCommand {
    /// Toast timed out (UI presentation expires). Keep overlay history.
    ExpireToastOnly { id: NotificationId },

    /// User explicitly dismissed. Remove globally.
    DismissGlobally { id: NotificationId },
}

/// Pure toast state.
///
/// - Maintains a most-recent-first stack of ids.
/// - Maintains per-id `ToastEntry` metadata.
/// - Exposes `visible_ids()` for rendering (capped by `max_visible`).
#[derive(Debug)]
pub struct ToastState {
    policy: ToastPolicy,
    max_visible: usize,

    /// Hard cap on retained toast ids in memory (not just visible).
    /// This prevents unbounded growth if notifications are very frequent.
    max_stack_len: usize,

    /// Most-recent-first stack.
    stack: VecDeque<NotificationId>,

    /// Per-id entry metadata.
    entries: HashMap<NotificationId, ToastEntry>,

    /// Global pause flag for all timers.
    paused: bool,
}

impl ToastState {
    pub fn new(policy: ToastPolicy, max_visible: usize) -> Self {
        Self {
            policy,
            max_visible: max_visible.max(1),
            max_stack_len: 50,
            stack: VecDeque::new(),
            entries: HashMap::new(),
            paused: false,
        }
    }

    /// Pause all timers.
    pub fn pause_all(&mut self, now: Instant) {
        if self.paused {
            return;
        }
        self.paused = true;
        for e in self.entries.values_mut() {
            e.pause(now);
        }
    }

    /// Resume all timers.
    pub fn resume_all(&mut self, now: Instant) {
        if !self.paused {
            return;
        }
        self.paused = false;
        for e in self.entries.values_mut() {
            e.resume(now);
        }
    }

    /// Clear all toast entries and stack state.
    ///
    /// This is useful for "Do Not Disturb" semantics where you want to ensure that
    /// any queued/pending toasts do not appear later when DND is turned off.
    ///
    /// Note: this is toast-state only; it does not affect the main notification history/model.
    pub fn clear_all(&mut self) {
        self.stack.clear();
        self.entries.clear();
    }

    /// Push a new notification id to the toast stack as "most recent".
    ///
    /// If the id already exists, it is moved to the front and its entry metadata is replaced.
    pub fn push(&mut self, id: NotificationId, urgency: Urgency, has_actions: bool, now: Instant) {
        // Remove any previous instance from stack.
        self.stack.retain(|x| *x != id);

        // Insert at front.
        self.stack.push_front(id);

        // Replace entry.
        let mut entry = ToastEntry::new(id, urgency, has_actions, now);
        if self.paused {
            entry.pause(now);
        }
        self.entries.insert(id, entry);

        self.trim_stack();
    }

    /// Remove an id from toast state (toast-only removal).
    ///
    /// This is used for:
    /// - toast expiry
    /// - DBus CloseNotification from client (if you want toast to disappear immediately)
    pub fn remove_toast_only(&mut self, id: NotificationId) {
        self.stack.retain(|x| *x != id);
        self.entries.remove(&id);
    }

    /// Global user dismissal (toast close button / default click / overlay dismissal).
    pub fn dismiss_globally(&mut self, id: NotificationId) -> ToastCommand {
        self.remove_toast_only(id);
        ToastCommand::DismissGlobally { id }
    }

    /// User dismissed from the toast UI.
    pub fn dismiss_user(&mut self, id: NotificationId) -> ToastCommand {
        self.dismiss_globally(id)
    }

    /// User dismissed from overlay/history UI (must also remove from toast stack).
    pub fn dismiss_overlay(&mut self, id: NotificationId) -> ToastCommand {
        self.dismiss_globally(id)
    }

    /// Compute which ids are currently visible, most-recent-first, capped to `max_visible`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn visible_ids(&self) -> Vec<NotificationId> {
        self.stack.iter().copied().take(self.max_visible).collect()
    }

    /// Compute per-id progress metadata for currently visible toasts.
    ///
    /// This is intended for the GTK view layer to render timeout indicators without owning any
    /// timer/pause/expiry logic.
    pub fn visible_items(&self, now: Instant) -> Vec<ToastProgress> {
        self.stack
            .iter()
            .copied()
            .take(self.max_visible)
            .filter_map(|id| {
                let e = self.entries.get(&id)?;
                Some(ToastProgress {
                    id,
                    ttl: e.ttl(self.policy),
                    elapsed: e.effective_elapsed(now),
                })
            })
            .collect()
    }

    /// Expire any due toasts (toast-only), returning commands for the caller.
    ///
    /// Policy:
    /// - Expiry produces `ExpireToastOnly` commands, not global dismissal.
    pub fn expire_due(&mut self, now: Instant) -> Vec<ToastCommand> {
        if self.paused {
            return vec![];
        }

        let mut due: Vec<NotificationId> = vec![];
        for id in self.stack.iter().copied() {
            if let Some(e) = self.entries.get(&id) {
                if e.is_due(now, self.policy) {
                    due.push(id);
                }
            }
        }

        for id in &due {
            self.remove_toast_only(*id);
        }

        due.into_iter()
            .map(|id| ToastCommand::ExpireToastOnly { id })
            .collect()
    }

    fn trim_stack(&mut self) {
        while self.stack.len() > self.max_stack_len {
            if let Some(id) = self.stack.pop_back() {
                self.entries.remove(&id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now0() -> Instant {
        Instant::now()
    }

    #[test]
    fn visible_ids_is_most_recent_first_and_capped() {
        let now = now0();
        let mut s = ToastState::new(ToastPolicy::default(), 2);

        s.push(1, Urgency::Normal, false, now);
        s.push(2, Urgency::Normal, false, now);
        s.push(3, Urgency::Normal, false, now);

        assert_eq!(s.visible_ids(), vec![3, 2]);
    }

    #[test]
    fn expiry_removes_toast_only_and_does_not_global_dismiss() {
        let base = now0();
        let mut s = ToastState::new(
            ToastPolicy {
                default_ttl: Duration::from_millis(10),
                actions_ttl: Duration::from_millis(10),
            },
            5,
        );

        s.push(42, Urgency::Normal, false, base);

        let cmds = s.expire_due(base + Duration::from_millis(20));
        assert_eq!(cmds, vec![ToastCommand::ExpireToastOnly { id: 42 }]);
        assert_eq!(s.visible_ids(), Vec::<NotificationId>::new());
    }

    #[test]
    fn critical_never_expires() {
        let base = now0();
        let mut s = ToastState::new(
            ToastPolicy {
                default_ttl: Duration::from_millis(1),
                actions_ttl: Duration::from_millis(1),
            },
            5,
        );

        s.push(1, Urgency::Critical, false, base);

        let cmds = s.expire_due(base + Duration::from_secs(10));
        assert!(cmds.is_empty());
        assert_eq!(s.visible_ids(), vec![1]);
    }

    #[test]
    fn pause_all_prevents_expiry_progress() {
        let base = now0();
        let mut s = ToastState::new(
            ToastPolicy {
                default_ttl: Duration::from_millis(10),
                actions_ttl: Duration::from_millis(10),
            },
            5,
        );

        s.push(1, Urgency::Normal, false, base);

        // Pause before expiry.
        s.pause_all(base + Duration::from_millis(5));

        // Even far in the future, while paused, nothing expires.
        let cmds = s.expire_due(base + Duration::from_secs(10));
        assert!(cmds.is_empty());
        assert_eq!(s.visible_ids(), vec![1]);

        // Resume, and then expiry should apply.
        s.resume_all(base + Duration::from_secs(10));
        let cmds = s.expire_due(base + Duration::from_secs(10) + Duration::from_millis(20));
        assert_eq!(cmds, vec![ToastCommand::ExpireToastOnly { id: 1 }]);
    }

    #[test]
    fn dismiss_globally_removes_from_state() {
        let now = now0();
        let mut s = ToastState::new(ToastPolicy::default(), 5);

        s.push(1, Urgency::Normal, false, now);
        s.push(2, Urgency::Normal, false, now);

        let cmd = s.dismiss_user(1);
        assert_eq!(cmd, ToastCommand::DismissGlobally { id: 1 });
        assert_eq!(s.visible_ids(), vec![2]);
    }
}
