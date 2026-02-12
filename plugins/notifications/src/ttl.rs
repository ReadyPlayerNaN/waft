//! TTL expiration for notifications.
//!
//! Replaces the old glib 200ms `Tick` timer with a sleep-to-deadline approach.
//! NO POLLING: calculates the next expiration deadline, sleeps until it,
//! then removes expired notifications.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};

use log::{debug, warn};
use tokio::sync::Notify;
use waft_plugin::EntityNotifier;

use crate::store::{NotificationOp, State, process_op};

/// Run the TTL expiration loop.
///
/// Sleeps until the next notification deadline, then removes expired
/// notifications. The `wake` notify is used to interrupt sleep when
/// new notifications arrive (which may have earlier deadlines).
pub async fn run_ttl_expiration(
    state: Arc<StdMutex<State>>,
    notifier: EntityNotifier,
    wake: Arc<Notify>,
) {
    loop {
        let next_deadline = compute_next_deadline(&state);

        match next_deadline {
            Some(deadline) => {
                let now = Instant::now();
                if deadline > now {
                    // Sleep until deadline or until woken by new notification
                    tokio::select! {
                        _ = tokio::time::sleep(deadline - now) => {}
                        _ = wake.notified() => {
                            // New notification arrived; re-check deadlines
                            continue;
                        }
                    }
                }

                // Expire notifications that have passed their TTL
                let expired_ids = collect_expired(&state);
                if !expired_ids.is_empty() {
                    debug!(
                        "[notifications/ttl] Expiring {} notifications",
                        expired_ids.len()
                    );
                    let changed = {
                        let mut guard = match state.lock() {
                            Ok(g) => g,
                            Err(e) => {
                                warn!("[notifications/ttl] mutex poisoned, recovering: {e}");
                                e.into_inner()
                            }
                        };
                        process_op(&mut guard, NotificationOp::TtlExpiry(expired_ids))
                    };
                    if changed {
                        notifier.notify();
                    }
                }
            }
            None => {
                // No notifications with TTL; wait for wake signal
                wake.notified().await;
            }
        }
    }
}

/// Compute the next expiration deadline as an `Instant`.
///
/// Looks at all panel notifications that have a TTL and a visible-since
/// timestamp, then returns the earliest deadline.
fn compute_next_deadline(state: &Arc<StdMutex<State>>) -> Option<Instant> {
    let guard = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[notifications/ttl] mutex poisoned in compute_next_deadline, recovering: {e}");
            e.into_inner()
        }
    };

    let now_system = SystemTime::now();
    let now_instant = Instant::now();
    let mut earliest: Option<Instant> = None;

    for (id, visible_since) in &guard.panel_visible_since_timestamps {
        let ttl_ms = match guard.notifications.get(id).and_then(|n| n.ttl) {
            Some(ttl) if ttl > 0 => ttl,
            _ => continue,
        };

        let deadline_system = *visible_since + Duration::from_millis(ttl_ms);

        // Convert SystemTime deadline to Instant
        let deadline_instant = if deadline_system > now_system {
            let remaining = deadline_system
                .duration_since(now_system)
                .unwrap_or(Duration::ZERO);
            now_instant + remaining
        } else {
            // Already expired
            now_instant
        };

        earliest = Some(match earliest {
            Some(e) if e <= deadline_instant => e,
            _ => deadline_instant,
        });
    }

    earliest
}

/// Collect IDs of notifications whose TTL has expired.
fn collect_expired(state: &Arc<StdMutex<State>>) -> Vec<u64> {
    let guard = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[notifications/ttl] mutex poisoned in collect_expired, recovering: {e}");
            e.into_inner()
        }
    };

    let now = SystemTime::now();
    let mut expired = Vec::new();

    for (id, visible_since) in &guard.panel_visible_since_timestamps {
        let ttl_ms = match guard.notifications.get(id).and_then(|n| n.ttl) {
            Some(ttl) if ttl > 0 => ttl,
            _ => continue,
        };

        let deadline = *visible_since + Duration::from_millis(ttl_ms);
        if now >= deadline {
            expired.push(*id);
        }
    }

    expired
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::ingress::IngressedNotification;
    use crate::store;
    use std::sync::Arc;

    fn make_notification(id: u64, ttl: Option<u64>) -> IngressedNotification {
        IngressedNotification {
            app_name: Some(Arc::from("test-app")),
            actions: vec![],
            created_at: SystemTime::now(),
            description: Arc::from("test"),
            icon: None,
            id,
            hints: Default::default(),
            replaces_id: None,
            title: Arc::from("Test"),
            ttl,
        }
    }

    #[test]
    fn compute_next_deadline_empty_state() {
        let state = Arc::new(StdMutex::new(State::new()));
        assert!(compute_next_deadline(&state).is_none());
    }

    #[test]
    fn compute_next_deadline_no_ttl() {
        let state = Arc::new(StdMutex::new(State::new()));
        {
            let mut guard = state.lock().unwrap();
            store::process_op(
                &mut guard,
                store::NotificationOp::Ingress(Box::new(make_notification(1, None))),
            );
        }
        assert!(compute_next_deadline(&state).is_none());
    }

    #[test]
    fn compute_next_deadline_with_ttl() {
        let state = Arc::new(StdMutex::new(State::new()));
        {
            let mut guard = state.lock().unwrap();
            store::process_op(
                &mut guard,
                store::NotificationOp::Ingress(Box::new(make_notification(1, Some(5000)))),
            );
        }
        let deadline = compute_next_deadline(&state);
        assert!(deadline.is_some());
        // Deadline should be in the future (roughly 5 seconds from now)
        let deadline = deadline.unwrap();
        assert!(deadline > Instant::now());
    }

    #[test]
    fn collect_expired_none_expired() {
        let state = Arc::new(StdMutex::new(State::new()));
        {
            let mut guard = state.lock().unwrap();
            store::process_op(
                &mut guard,
                store::NotificationOp::Ingress(Box::new(make_notification(1, Some(60000)))),
            );
        }
        let expired = collect_expired(&state);
        assert!(expired.is_empty());
    }

    #[test]
    fn collect_expired_with_zero_ttl_skipped() {
        let state = Arc::new(StdMutex::new(State::new()));
        {
            let mut guard = state.lock().unwrap();
            // ttl=0 means "server default" which we treat as no expiry in store
            store::process_op(
                &mut guard,
                store::NotificationOp::Ingress(Box::new(make_notification(1, Some(0)))),
            );
        }
        let expired = collect_expired(&state);
        assert!(expired.is_empty());
    }

    #[test]
    fn collect_expired_past_deadline() {
        let state = Arc::new(StdMutex::new(State::new()));
        {
            let mut guard = state.lock().unwrap();
            store::process_op(
                &mut guard,
                store::NotificationOp::Ingress(Box::new(make_notification(1, Some(1)))),
            );
            // Backdate the visible-since timestamp to ensure expiry
            if let Some(ts) = guard.panel_visible_since_timestamps.get_mut(&1) {
                *ts = SystemTime::now() - Duration::from_secs(10);
            }
        }
        let expired = collect_expired(&state);
        assert_eq!(expired, vec![1]);
    }
}
