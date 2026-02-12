use std::collections::HashMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// A pending action awaiting a response from a plugin.
pub struct PendingAction {
    pub action_id: Uuid,
    pub app_conn_id: Uuid,
    pub plugin_conn_id: Uuid,
    pub deadline: Instant,
}

/// Tracks in-flight actions and their timeouts.
pub struct ActionTracker {
    pending: HashMap<Uuid, PendingAction>,
}

impl ActionTracker {
    pub fn new() -> Self {
        ActionTracker {
            pending: HashMap::new(),
        }
    }

    /// Start tracking an action. Returns when the action should time out.
    pub fn track(
        &mut self,
        action_id: Uuid,
        app_conn_id: Uuid,
        plugin_conn_id: Uuid,
        timeout_ms: Option<u64>,
    ) {
        let timeout = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT);
        let deadline = Instant::now() + timeout;

        self.pending.insert(
            action_id,
            PendingAction {
                action_id,
                app_conn_id,
                plugin_conn_id,
                deadline,
            },
        );
    }

    /// Resolve (complete) a pending action, returning its metadata.
    pub fn resolve(&mut self, action_id: Uuid) -> Option<PendingAction> {
        self.pending.remove(&action_id)
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
            .filter_map(|id| self.pending.remove(&id))
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
            .filter_map(|id| self.pending.remove(&id))
            .collect()
    }

    /// Duration until the next action times out, or `None` if no pending actions.
    /// Will be used to replace interval-based timeout polling with sleep-to-deadline.
    #[allow(dead_code)]
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
        let resolved = resolved.unwrap();
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

        let deadline = tracker.next_deadline().unwrap();
        // The nearest deadline should be roughly 100ms from now
        assert!(deadline <= Instant::now() + Duration::from_millis(200));
    }
}
