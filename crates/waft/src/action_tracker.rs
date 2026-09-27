use std::collections::HashMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// A pending action awaiting a response from a plugin.
#[derive(Clone)]
pub struct PendingAction {
    pub action_id: Uuid,
    pub app_conn_id: Uuid,
    pub plugin_conn_id: Uuid,
    pub deadline: Instant,
    action_key: Option<String>,
}

/// Tracks in-flight actions and their timeouts.
pub struct ActionTracker {
    pending: HashMap<Uuid, PendingAction>,
    pending_keys: HashMap<String, Uuid>,
}

impl ActionTracker {
    pub fn new() -> Self {
        ActionTracker {
            pending: HashMap::new(),
            pending_keys: HashMap::new(),
        }
    }

    /// Start tracking an action. Returns `false` when the action ID is already
    /// in flight, preserving the original request instead of orphaning it.
    pub fn track(
        &mut self,
        action_id: Uuid,
        app_conn_id: Uuid,
        plugin_conn_id: Uuid,
        timeout_ms: Option<u64>,
    ) -> bool {
        self.track_with_key(action_id, app_conn_id, plugin_conn_id, timeout_ms, None)
    }

    /// Start tracking a non-reentrant action keyed by entity/action identity.
    pub fn track_with_key(
        &mut self,
        action_id: Uuid,
        app_conn_id: Uuid,
        plugin_conn_id: Uuid,
        timeout_ms: Option<u64>,
        action_key: Option<String>,
    ) -> bool {
        if self.pending.contains_key(&action_id)
            || action_key
                .as_ref()
                .is_some_and(|key| self.pending_keys.contains_key(key))
        {
            return false;
        }
        let timeout = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT);
        let deadline = Instant::now() + timeout;

        if let Some(key) = action_key.as_ref() {
            self.pending_keys.insert(key.clone(), action_id);
        }
        self.pending.insert(
            action_id,
            PendingAction {
                action_id,
                app_conn_id,
                plugin_conn_id,
                deadline,
                action_key,
            },
        );
        true
    }

    /// Resolve (complete) a pending action, returning its metadata.
    pub fn resolve(&mut self, action_id: Uuid) -> Option<PendingAction> {
        let action = self.pending.remove(&action_id)?;
        if let Some(key) = action.action_key.as_ref() {
            self.pending_keys.remove(key);
        }
        Some(action)
    }

    /// Remove and return all actions that have exceeded their deadline.
    pub fn drain_timed_out(&mut self) -> Vec<PendingAction> {
        let now = Instant::now();
        let expired: Vec<Uuid> = self
            .pending
            .iter()
            .filter(|(_, action)| action.deadline <= now)
            .map(|(id, _)| *id)
            .collect();

        expired
            .into_iter()
            .filter_map(|id| self.resolve(id))
            .collect()
    }

    /// Snapshot all actions associated with a connection without resolving them.
    pub fn actions_for_connection(&self, conn_id: Uuid) -> Vec<PendingAction> {
        self.pending
            .values()
            .filter(|a| a.app_conn_id == conn_id || a.plugin_conn_id == conn_id)
            .cloned()
            .collect()
    }

    /// Remove all actions associated with a connection (plugin or app disconnect).
    pub fn drain_for_connection(&mut self, conn_id: Uuid) -> Vec<PendingAction> {
        let matching: Vec<Uuid> = self
            .pending
            .iter()
            .filter(|(_, a)| a.app_conn_id == conn_id || a.plugin_conn_id == conn_id)
            .map(|(id, _)| *id)
            .collect();

        matching
            .into_iter()
            .filter_map(|id| self.resolve(id))
            .collect()
    }

    /// Earliest deadline among pending actions, or `None` if no pending actions.
    ///
    /// Used by the daemon event loop to sleep precisely until the next timeout
    /// instead of polling on a fixed interval.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|a| a.deadline).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_and_resolve() {
        let mut tracker = ActionTracker::new();
        let action_id = Uuid::new_v4();
        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.track(action_id, app, plugin, None);

        let resolved = tracker.resolve(action_id);
        assert!(resolved.is_some());
        let resolved = resolved.expect("expected value");
        assert_eq!(resolved.action_id, action_id);
        assert_eq!(resolved.app_conn_id, app);
        assert_eq!(resolved.plugin_conn_id, plugin);

        // Double resolve returns None
        assert!(tracker.resolve(action_id).is_none());
    }

    #[test]
    fn drain_timed_out() {
        let mut tracker = ActionTracker::new();
        let expired_id = Uuid::new_v4();
        let alive_id = Uuid::new_v4();
        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        // Expired: 0ms timeout
        tracker.track(expired_id, app, plugin, Some(0));
        // Alive: 60s timeout
        tracker.track(alive_id, app, plugin, Some(60_000));

        std::thread::sleep(Duration::from_millis(1));

        let timed_out = tracker.drain_timed_out();
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0].action_id, expired_id);

        // The alive action should still be pending
        assert!(tracker.resolve(alive_id).is_some());
    }

    #[test]
    fn drain_for_connection() {
        let mut tracker = ActionTracker::new();
        let a1 = Uuid::new_v4();
        let a2 = Uuid::new_v4();
        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();
        let other_app = Uuid::new_v4();

        tracker.track(a1, app, plugin, None);
        tracker.track(a2, other_app, plugin, None);

        let drained = tracker.drain_for_connection(app);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].action_id, a1);

        // a2 still pending
        assert!(tracker.resolve(a2).is_some());
    }

    #[test]
    fn next_deadline() {
        let mut tracker = ActionTracker::new();
        assert!(tracker.next_deadline().is_none());

        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.track(Uuid::new_v4(), app, plugin, Some(100));
        tracker.track(Uuid::new_v4(), app, plugin, Some(5000));

        let deadline = tracker.next_deadline().expect("expected value");
        // The nearest deadline should be roughly 100ms from now
        assert!(deadline <= Instant::now() + Duration::from_millis(200));
    }

    #[test]
    fn duplicate_action_ids_do_not_replace_an_in_flight_action() {
        let mut tracker = ActionTracker::new();
        let action_id = Uuid::new_v4();
        let first_app = Uuid::new_v4();
        let second_app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.track(action_id, first_app, plugin, Some(0));
        tracker.track(action_id, second_app, plugin, Some(60_000));

        std::thread::sleep(Duration::from_millis(1));
        let timed_out = tracker.drain_timed_out();

        assert_eq!(timed_out.len(), 1);
        assert_eq!(
            timed_out[0].app_conn_id, first_app,
            "a repeated daemon call must not orphan the original request"
        );
    }
}
