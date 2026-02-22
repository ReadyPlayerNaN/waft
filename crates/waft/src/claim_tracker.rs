//! Tracks in-flight EntityClaim requests and aggregates app responses.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use uuid::Uuid;
use waft_protocol::urn::Urn;

/// How long to wait for all app responses before treating missing ones as "pass".
const CLAIM_TIMEOUT: Duration = Duration::from_secs(2);

/// Result of a completed claim: whether any subscriber claimed the entity.
pub struct ClaimResolution {
    pub urn: Urn,
    pub claim_id: Uuid,
    #[allow(dead_code)]
    pub entity_type: String,
    pub plugin_conn_id: Uuid,
    pub claimed: bool,
}

struct PendingClaim {
    urn: Urn,
    entity_type: String,
    plugin_conn_id: Uuid,
    expected: HashSet<Uuid>,
    received: HashSet<Uuid>,
    any_claimed: bool,
    deadline: Instant,
}

/// Tracks in-flight claim requests.
pub struct ClaimTracker {
    pending: HashMap<Uuid, PendingClaim>,
}

impl ClaimTracker {
    pub fn new() -> Self {
        ClaimTracker {
            pending: HashMap::new(),
        }
    }

    /// Start tracking a claim. Returns the deadline.
    ///
    /// `expected_conns` is the set of app connection IDs we are broadcasting to.
    pub fn start(
        &mut self,
        claim_id: Uuid,
        urn: Urn,
        entity_type: String,
        plugin_conn_id: Uuid,
        expected_conns: HashSet<Uuid>,
    ) -> Instant {
        let deadline = Instant::now() + CLAIM_TIMEOUT;
        self.pending.insert(
            claim_id,
            PendingClaim {
                urn,
                entity_type,
                plugin_conn_id,
                expected: expected_conns,
                received: HashSet::new(),
                any_claimed: false,
                deadline,
            },
        );
        deadline
    }

    /// Record a response from an app. Returns the resolution if all expected have responded.
    pub fn record_response(
        &mut self,
        claim_id: Uuid,
        from_conn: Uuid,
        claimed: bool,
    ) -> Option<ClaimResolution> {
        let claim = self.pending.get_mut(&claim_id)?;

        if !claim.expected.contains(&from_conn) {
            return None; // unexpected responder, ignore
        }

        claim.received.insert(from_conn);
        claim.any_claimed |= claimed;

        if claim.expected == claim.received {
            self.resolve(claim_id)
        } else {
            None
        }
    }

    /// Remove and return all claims that have passed their deadline (treat as not claimed).
    pub fn drain_timed_out(&mut self) -> Vec<ClaimResolution> {
        let now = Instant::now();
        let expired: Vec<Uuid> = self
            .pending
            .iter()
            .filter(|(_, c)| c.deadline <= now)
            .map(|(id, _)| *id)
            .collect();

        expired
            .into_iter()
            .filter_map(|id| {
                self.pending.remove(&id).map(|c| ClaimResolution {
                    urn: c.urn,
                    claim_id: id,
                    entity_type: c.entity_type,
                    plugin_conn_id: c.plugin_conn_id,
                    claimed: c.any_claimed,
                })
            })
            .collect()
    }

    /// When an app disconnects, treat it as having responded "pass" for all claims it was expected in.
    /// Returns resolutions for claims that are now fully resolved.
    pub fn remove_app_conn(&mut self, conn_id: Uuid) -> Vec<ClaimResolution> {
        let claim_ids: Vec<Uuid> = self
            .pending
            .iter()
            .filter(|(_, c)| c.expected.contains(&conn_id))
            .map(|(id, _)| *id)
            .collect();

        let mut resolutions = Vec::new();
        for claim_id in claim_ids {
            if let Some(claim) = self.pending.get_mut(&claim_id) {
                claim.received.insert(conn_id); // treat as responded
                // any_claimed unchanged (disconnect = pass)
                if claim.expected == claim.received {
                    if let Some(resolution) = self.resolve(claim_id) {
                        resolutions.push(resolution);
                    }
                }
            }
        }
        resolutions
    }

    /// When a plugin disconnects, cancel all its pending claims (no resolution sent).
    pub fn remove_plugin_conn(&mut self, conn_id: Uuid) -> Vec<Uuid> {
        let ids: Vec<Uuid> = self
            .pending
            .iter()
            .filter(|(_, c)| c.plugin_conn_id == conn_id)
            .map(|(id, _)| *id)
            .collect();

        for id in &ids {
            self.pending.remove(id);
        }
        ids
    }

    /// Earliest deadline among pending claims, or `None` if no pending claims.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|c| c.deadline).min()
    }

    fn resolve(&mut self, claim_id: Uuid) -> Option<ClaimResolution> {
        self.pending.remove(&claim_id).map(|c| ClaimResolution {
            urn: c.urn,
            claim_id,
            entity_type: c.entity_type,
            plugin_conn_id: c.plugin_conn_id,
            claimed: c.any_claimed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_urn() -> Urn {
        Urn::new("notifications", "notification", "42")
    }

    #[test]
    fn single_responder_claimed() {
        let mut tracker = ClaimTracker::new();
        let claim_id = Uuid::new_v4();
        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.start(
            claim_id,
            make_urn(),
            "notification".to_string(),
            plugin,
            HashSet::from([app]),
        );

        let result = tracker.record_response(claim_id, app, true);
        assert!(result.is_some());
        assert!(result.unwrap().claimed);
    }

    #[test]
    fn single_responder_not_claimed() {
        let mut tracker = ClaimTracker::new();
        let claim_id = Uuid::new_v4();
        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.start(
            claim_id,
            make_urn(),
            "notification".to_string(),
            plugin,
            HashSet::from([app]),
        );

        let result = tracker.record_response(claim_id, app, false);
        assert!(result.is_some());
        assert!(!result.unwrap().claimed);
    }

    #[test]
    fn two_responders_one_claims() {
        let mut tracker = ClaimTracker::new();
        let claim_id = Uuid::new_v4();
        let app1 = Uuid::new_v4();
        let app2 = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.start(
            claim_id,
            make_urn(),
            "notification".to_string(),
            plugin,
            HashSet::from([app1, app2]),
        );

        // First response: not claimed
        let result = tracker.record_response(claim_id, app1, false);
        assert!(result.is_none()); // still waiting for app2

        // Second response: claimed
        let result = tracker.record_response(claim_id, app2, true);
        assert!(result.is_some());
        assert!(result.unwrap().claimed); // OR of both
    }

    #[test]
    fn unexpected_responder_is_ignored() {
        let mut tracker = ClaimTracker::new();
        let claim_id = Uuid::new_v4();
        let app = Uuid::new_v4();
        let interloper = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.start(
            claim_id,
            make_urn(),
            "notification".to_string(),
            plugin,
            HashSet::from([app]),
        );

        let result = tracker.record_response(claim_id, interloper, true);
        assert!(result.is_none());

        // Real app still needed
        let result = tracker.record_response(claim_id, app, false);
        assert!(result.is_some());
        assert!(!result.unwrap().claimed);
    }

    #[test]
    fn drain_timed_out() {
        let mut tracker = ClaimTracker::new();
        let claim_id = Uuid::new_v4();
        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        // Manually insert with expired deadline
        tracker.pending.insert(
            claim_id,
            PendingClaim {
                urn: make_urn(),
                entity_type: "notification".to_string(),
                plugin_conn_id: plugin,
                expected: HashSet::from([app]),
                received: HashSet::new(),
                any_claimed: false,
                deadline: Instant::now() - Duration::from_millis(1),
            },
        );

        let timed_out = tracker.drain_timed_out();
        assert_eq!(timed_out.len(), 1);
        assert!(!timed_out[0].claimed);
    }

    #[test]
    fn remove_app_conn_resolves_claim() {
        let mut tracker = ClaimTracker::new();
        let claim_id = Uuid::new_v4();
        let app1 = Uuid::new_v4();
        let app2 = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.start(
            claim_id,
            make_urn(),
            "notification".to_string(),
            plugin,
            HashSet::from([app1, app2]),
        );

        tracker.record_response(claim_id, app1, false);

        // app2 disconnects — should resolve claim as not claimed
        let resolutions = tracker.remove_app_conn(app2);
        assert_eq!(resolutions.len(), 1);
        assert!(!resolutions[0].claimed);
    }

    #[test]
    fn remove_plugin_conn_cancels_claims() {
        let mut tracker = ClaimTracker::new();
        let claim_id = Uuid::new_v4();
        let app = Uuid::new_v4();
        let plugin = Uuid::new_v4();

        tracker.start(
            claim_id,
            make_urn(),
            "notification".to_string(),
            plugin,
            HashSet::from([app]),
        );

        let cancelled = tracker.remove_plugin_conn(plugin);
        assert_eq!(cancelled, vec![claim_id]);
        assert!(tracker.pending.is_empty());
    }

    #[test]
    fn next_deadline_none_when_empty() {
        let tracker = ClaimTracker::new();
        assert!(tracker.next_deadline().is_none());
    }
}
