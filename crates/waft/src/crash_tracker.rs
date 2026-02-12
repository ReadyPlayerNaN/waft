//! Crash tracking and circuit breaker for plugin processes.
//!
//! When a plugin disconnects unexpectedly, the daemon attempts to restart it.
//! If a plugin crashes repeatedly (5+ times in 60 seconds), the circuit breaker
//! trips and the plugin is no longer restarted. Subscribers receive
//! `EntityStale` on first crash and `EntityOutdated` when the breaker trips.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Maximum number of crashes within the window before the circuit breaker trips.
const MAX_CRASHES: usize = 5;

/// Time window for counting crashes.
const CRASH_WINDOW: Duration = Duration::from_secs(60);

/// Outcome when a plugin crash is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrashOutcome {
    /// The plugin should be restarted.
    Restart,
    /// Too many crashes — circuit breaker tripped, do not restart.
    CircuitBroken,
}

/// Per-plugin crash history.
struct CrashHistory {
    /// Recent crash timestamps, newest last.
    crashes: VecDeque<Instant>,
    /// Whether the circuit breaker has tripped.
    broken: bool,
}

impl CrashHistory {
    fn new() -> Self {
        CrashHistory {
            crashes: VecDeque::new(),
            broken: false,
        }
    }

    /// Record a crash and determine the outcome.
    fn record_crash(&mut self, now: Instant) -> CrashOutcome {
        if self.broken {
            return CrashOutcome::CircuitBroken;
        }

        self.crashes.push_back(now);

        // Evict crashes outside the window
        let cutoff = now.checked_sub(CRASH_WINDOW).unwrap_or(now);
        while let Some(&oldest) = self.crashes.front() {
            if oldest < cutoff {
                self.crashes.pop_front();
            } else {
                break;
            }
        }

        if self.crashes.len() >= MAX_CRASHES {
            self.broken = true;
            CrashOutcome::CircuitBroken
        } else {
            CrashOutcome::Restart
        }
    }

    /// Whether the circuit breaker has tripped for this plugin.
    fn circuit_broken(&self) -> bool {
        self.broken
    }
}

/// Tracks crash history for all plugins.
pub struct CrashTracker {
    history: HashMap<String, CrashHistory>,
}

impl CrashTracker {
    pub fn new() -> Self {
        CrashTracker {
            history: HashMap::new(),
        }
    }

    /// Record a crash for a plugin and determine the outcome.
    ///
    /// Returns `Restart` if the plugin should be restarted, or
    /// `CircuitBroken` if it has crashed too many times.
    pub fn record_crash(&mut self, plugin_name: &str) -> CrashOutcome {
        let now = Instant::now();
        self.history
            .entry(plugin_name.to_string())
            .or_insert_with(CrashHistory::new)
            .record_crash(now)
    }

    /// Check whether a plugin's circuit breaker has tripped.
    pub fn circuit_broken(&self, plugin_name: &str) -> bool {
        self.history
            .get(plugin_name)
            .is_some_and(|h| h.circuit_broken())
    }

    /// Reset crash history for a plugin (e.g. after a successful long run).
    #[allow(dead_code)]
    pub fn reset(&mut self, plugin_name: &str) {
        self.history.remove(plugin_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_crash_returns_restart() {
        let mut tracker = CrashTracker::new();
        assert_eq!(tracker.record_crash("weather"), CrashOutcome::Restart);
        assert!(!tracker.circuit_broken("weather"));
    }

    #[test]
    fn circuit_breaks_after_max_crashes() {
        let mut tracker = CrashTracker::new();
        for _ in 0..MAX_CRASHES - 1 {
            assert_eq!(tracker.record_crash("weather"), CrashOutcome::Restart);
        }
        // The MAX_CRASHES-th crash trips the breaker
        assert_eq!(
            tracker.record_crash("weather"),
            CrashOutcome::CircuitBroken
        );
        assert!(tracker.circuit_broken("weather"));
    }

    #[test]
    fn circuit_stays_broken_on_subsequent_crashes() {
        let mut tracker = CrashTracker::new();
        for _ in 0..MAX_CRASHES {
            tracker.record_crash("weather");
        }
        assert_eq!(
            tracker.record_crash("weather"),
            CrashOutcome::CircuitBroken
        );
    }

    #[test]
    fn independent_plugins_have_independent_history() {
        let mut tracker = CrashTracker::new();
        for _ in 0..MAX_CRASHES {
            tracker.record_crash("weather");
        }
        assert!(tracker.circuit_broken("weather"));
        assert!(!tracker.circuit_broken("clock"));
        assert_eq!(tracker.record_crash("clock"), CrashOutcome::Restart);
    }

    #[test]
    fn reset_clears_crash_history() {
        let mut tracker = CrashTracker::new();
        for _ in 0..MAX_CRASHES {
            tracker.record_crash("weather");
        }
        assert!(tracker.circuit_broken("weather"));

        tracker.reset("weather");
        assert!(!tracker.circuit_broken("weather"));
        assert_eq!(tracker.record_crash("weather"), CrashOutcome::Restart);
    }

    #[test]
    fn old_crashes_expire_outside_window() {
        let mut history = CrashHistory::new();

        // Record crashes "in the past" by manipulating the now parameter.
        // All old crashes must fall before `now - CRASH_WINDOW` to be evicted.
        // The last old crash is at `past + (MAX_CRASHES - 2)`, so push past far enough.
        let past = Instant::now() - CRASH_WINDOW - Duration::from_secs(MAX_CRASHES as u64);
        for i in 0..MAX_CRASHES - 1 {
            history.crashes.push_back(past + Duration::from_secs(i as u64));
        }

        // A new crash should evict the old ones and return Restart
        let outcome = history.record_crash(Instant::now());
        assert_eq!(outcome, CrashOutcome::Restart);
        // Only the recent crash should remain
        assert_eq!(history.crashes.len(), 1);
    }

    #[test]
    fn unknown_plugin_not_broken() {
        let tracker = CrashTracker::new();
        assert!(!tracker.circuit_broken("nonexistent"));
    }
}
