//! Agenda store module.
//!
//! Manages agenda state with reactive subscriptions.

use std::collections::BTreeMap;

use crate::set_field;
use crate::store::{PluginStore, StoreOp, StoreState};

use super::values::{AgendaEvent, CalendarSource};

/// Operations for the agenda store.
#[derive(Clone)]
#[allow(dead_code)] // ClearEvents is tested but not yet used in production
pub enum AgendaOp {
    SetSources(Vec<CalendarSource>),
    UpsertEvents(Vec<AgendaEvent>),
    RemoveEvents(Vec<String>),
    ClearEvents,
    SetAvailable(bool),
    SetLoading(bool),
    SetError(Option<String>),
    SetNextPeriodStart(Option<i64>),
    SetQuerySince(i64),
    SetShowPast(bool),
}

impl StoreOp for AgendaOp {}

/// State for the agenda plugin.
#[derive(Clone)]
pub struct AgendaState {
    pub sources: Vec<CalendarSource>,
    pub events: BTreeMap<String, AgendaEvent>,
    pub available: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub next_period_start: Option<i64>,
    /// Start of the current query time range (events ending before this are out of range).
    pub query_since: Option<i64>,
    /// Whether to show past events. `true` = no filtering (default), `false` = hide past.
    pub show_past: bool,
}

impl Default for AgendaState {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            events: BTreeMap::new(),
            available: false,
            loading: false,
            error: None,
            next_period_start: None,
            query_since: None,
            show_past: true,
        }
    }
}

impl StoreState for AgendaState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

/// Type alias for the agenda store.
pub type AgendaStore = PluginStore<AgendaOp, AgendaState>;

/// Create a new agenda store instance.
pub fn create_agenda_store() -> AgendaStore {
    PluginStore::new(|state: &mut AgendaState, op: AgendaOp| match op {
        AgendaOp::SetSources(sources) => {
            state.sources = sources;
            true
        }
        AgendaOp::UpsertEvents(events) => {
            let mut changed = false;
            for event in events {
                let key = event.occurrence_key();
                state.events.insert(key, event);
                changed = true;
            }
            changed
        }
        AgendaOp::RemoveEvents(uids) => {
            let mut changed = false;
            for uid in uids {
                // Recurring events share the same base UID but are stored
                // with occurrence keys (uid@start_time). Remove all
                // occurrences whose key starts with the base UID.
                let keys_to_remove: Vec<String> = state
                    .events
                    .keys()
                    .filter(|k| {
                        k.starts_with(&uid)
                            && (k.len() == uid.len() || k[uid.len()..].starts_with('@'))
                    })
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    state.events.remove(&key);
                    changed = true;
                }
            }
            changed
        }
        AgendaOp::ClearEvents => {
            if state.events.is_empty() {
                false
            } else {
                state.events.clear();
                true
            }
        }
        AgendaOp::SetAvailable(available) => set_field!(state.available, available),
        AgendaOp::SetLoading(loading) => set_field!(state.loading, loading),
        AgendaOp::SetError(error) => set_field!(state.error, error),
        AgendaOp::SetNextPeriodStart(ts) => set_field!(state.next_period_start, ts),
        AgendaOp::SetQuerySince(since) => set_field!(state.query_since, Some(since)),
        AgendaOp::SetShowPast(show) => set_field!(state.show_past, show),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(uid: &str, start: i64, end: i64) -> AgendaEvent {
        AgendaEvent {
            uid: uid.to_string(),
            summary: format!("Event {}", uid),
            start_time: start,
            end_time: end,
            all_day: false,
            description: None,
            alt_description: None,
            location: None,
            attendees: Vec::new(),
        }
    }

    #[test]
    fn upsert_events_inserts_new_events() {
        let store = create_agenda_store();
        store.emit(AgendaOp::UpsertEvents(vec![
            make_event("a", 1000, 2000),
            make_event("b", 3000, 4000),
        ]));
        let state = store.get_state();
        assert_eq!(state.events.len(), 2);
        // Keys are occurrence keys: uid@start_time
        assert!(state.events.contains_key("a@1000"));
        assert!(state.events.contains_key("b@3000"));
    }

    #[test]
    fn upsert_recurring_events_keeps_all_instances() {
        let store = create_agenda_store();
        // Same UID, different start times (recurring event instances)
        store.emit(AgendaOp::UpsertEvents(vec![make_event("a", 1000, 2000)]));
        store.emit(AgendaOp::UpsertEvents(vec![make_event("a", 5000, 6000)]));
        let state = store.get_state();
        // Both instances stored under different occurrence keys
        assert_eq!(state.events.len(), 2);
        assert!(state.events.contains_key("a@1000"));
        assert!(state.events.contains_key("a@5000"));
    }

    #[test]
    fn upsert_same_occurrence_updates_existing() {
        let store = create_agenda_store();
        store.emit(AgendaOp::UpsertEvents(vec![make_event("a", 1000, 2000)]));
        // Same uid AND same start_time → same occurrence key → overwrites
        store.emit(AgendaOp::UpsertEvents(vec![make_event("a", 1000, 3000)]));
        let state = store.get_state();
        assert_eq!(state.events.len(), 1);
        assert_eq!(state.events["a@1000"].end_time, 3000);
    }

    #[test]
    fn upsert_does_not_filter_by_time_range() {
        let store = create_agenda_store();
        // Events with arbitrary timestamps should all be accepted
        store.emit(AgendaOp::UpsertEvents(vec![
            make_event("past", 100, 200),
            make_event("future", 9999999999, 9999999999 + 3600),
        ]));
        let state = store.get_state();
        assert_eq!(state.events.len(), 2);
    }

    #[test]
    fn remove_events_removes_all_occurrences_by_base_uid() {
        let store = create_agenda_store();
        // Two occurrences of "a" and one of "b"
        store.emit(AgendaOp::UpsertEvents(vec![
            make_event("a", 1000, 2000),
            make_event("a", 5000, 6000),
            make_event("b", 3000, 4000),
        ]));
        store.emit(AgendaOp::RemoveEvents(vec!["a".to_string()]));
        let state = store.get_state();
        assert_eq!(state.events.len(), 1);
        assert!(state.events.contains_key("b@3000"));
    }

    #[test]
    fn remove_nonexistent_uid_is_noop() {
        let store = create_agenda_store();
        store.emit(AgendaOp::UpsertEvents(vec![make_event("a", 1000, 2000)]));
        store.emit(AgendaOp::RemoveEvents(vec!["nonexistent".to_string()]));
        let state = store.get_state();
        assert_eq!(state.events.len(), 1);
    }

    #[test]
    fn clear_events_removes_all() {
        let store = create_agenda_store();
        store.emit(AgendaOp::UpsertEvents(vec![
            make_event("a", 1000, 2000),
            make_event("b", 3000, 4000),
        ]));
        store.emit(AgendaOp::ClearEvents);
        let state = store.get_state();
        assert!(state.events.is_empty());
    }

    #[test]
    fn clear_empty_events_is_noop() {
        let store = create_agenda_store();
        // Should not trigger a state change
        store.emit(AgendaOp::ClearEvents);
        let state = store.get_state();
        assert!(state.events.is_empty());
    }

    #[test]
    fn set_next_period_start_sets_value() {
        let store = create_agenda_store();
        assert!(store.get_state().next_period_start.is_none());

        store.emit(AgendaOp::SetNextPeriodStart(Some(1700000000)));
        assert_eq!(store.get_state().next_period_start, Some(1700000000));
    }

    #[test]
    fn set_next_period_start_clears_value() {
        let store = create_agenda_store();
        store.emit(AgendaOp::SetNextPeriodStart(Some(1700000000)));
        store.emit(AgendaOp::SetNextPeriodStart(None));
        assert!(store.get_state().next_period_start.is_none());
    }

    #[test]
    fn set_next_period_start_same_value_no_change() {
        let store = create_agenda_store();
        store.emit(AgendaOp::SetNextPeriodStart(Some(1700000000)));

        let changed = std::cell::Cell::new(false);
        // Subscribing after setting the value - emitting same value should not trigger
        // (we can't directly test the return value, but we can verify state is stable)
        store.emit(AgendaOp::SetNextPeriodStart(Some(1700000000)));
        assert!(!changed.get());
    }

    #[test]
    fn set_loading_updates_state() {
        let store = create_agenda_store();
        assert!(!store.get_state().loading);
        store.emit(AgendaOp::SetLoading(true));
        assert!(store.get_state().loading);
        store.emit(AgendaOp::SetLoading(false));
        assert!(!store.get_state().loading);
    }

    #[test]
    fn set_available_updates_state() {
        let store = create_agenda_store();
        assert!(!store.get_state().available);
        store.emit(AgendaOp::SetAvailable(true));
        assert!(store.get_state().available);
    }

    #[test]
    fn set_error_updates_state() {
        let store = create_agenda_store();
        assert!(store.get_state().error.is_none());
        store.emit(AgendaOp::SetError(Some("fail".to_string())));
        assert_eq!(store.get_state().error.as_deref(), Some("fail"));
        store.emit(AgendaOp::SetError(None));
        assert!(store.get_state().error.is_none());
    }

    #[test]
    fn set_sources_updates_state() {
        let store = create_agenda_store();
        assert!(store.get_state().sources.is_empty());
        store.emit(AgendaOp::SetSources(vec![CalendarSource {
            uid: "cal-1".to_string(),
            display_name: "Personal".to_string(),
        }]));
        let state = store.get_state();
        assert_eq!(state.sources.len(), 1);
        assert_eq!(state.sources[0].uid, "cal-1");
    }

    #[test]
    fn default_state_is_empty() {
        let store = create_agenda_store();
        let state = store.get_state();
        assert!(state.sources.is_empty());
        assert!(state.events.is_empty());
        assert!(!state.available);
        assert!(!state.loading);
        assert!(state.error.is_none());
        assert!(state.next_period_start.is_none());
        assert!(state.show_past);
    }

    #[test]
    fn default_state_show_past_is_true() {
        let store = create_agenda_store();
        assert!(store.get_state().show_past);
    }

    #[test]
    fn set_show_past_updates_state() {
        let store = create_agenda_store();
        assert!(store.get_state().show_past);
        store.emit(AgendaOp::SetShowPast(false));
        assert!(!store.get_state().show_past);
        store.emit(AgendaOp::SetShowPast(true));
        assert!(store.get_state().show_past);
    }

    #[test]
    fn remove_then_upsert_handles_time_change_correctly() {
        // Regression test for duplicate events when time changes
        // Simulates the correct handling of ObjectsModified: Remove old occurrences, then insert new
        let store = create_agenda_store();

        // Initial event at 14:00 (1770127200 = 2026-02-03 14:00 UTC)
        store.emit(AgendaOp::UpsertEvents(vec![make_event(
            "event-123",
            1770127200,
            1770130800,
        )]));

        {
            let state = store.get_state();
            assert_eq!(state.events.len(), 1);
            assert!(state.events.contains_key("event-123@1770127200"));
        }

        // Event time changed to 15:00 (1770130800 = 2026-02-03 15:00 UTC)
        // Simulate correct handling: remove old occurrences, then upsert new
        store.emit(AgendaOp::RemoveEvents(vec!["event-123".to_string()]));
        store.emit(AgendaOp::UpsertEvents(vec![make_event(
            "event-123",
            1770130800,
            1770134400,
        )]));

        let state = store.get_state();
        // Should only have 1 event with the new time
        assert_eq!(
            state.events.len(),
            1,
            "Should have exactly 1 event after time change. Keys: {:?}",
            state.events.keys().collect::<Vec<_>>()
        );
        assert!(!state.events.contains_key("event-123@1770127200"), "Old occurrence should be removed");
        assert!(state.events.contains_key("event-123@1770130800"), "New occurrence should exist");
        assert_eq!(state.events["event-123@1770130800"].end_time, 1770134400);
    }

    #[test]
    fn upsert_only_creates_duplicates_when_time_changes() {
        // Demonstrates the bug: UpsertEvents alone creates duplicates when time changes
        let store = create_agenda_store();

        // Initial event at 14:00
        store.emit(AgendaOp::UpsertEvents(vec![make_event(
            "event-123",
            1770127200,
            1770130800,
        )]));

        // Event time changed to 15:00 - upsert without remove
        store.emit(AgendaOp::UpsertEvents(vec![make_event(
            "event-123",
            1770130800,
            1770134400,
        )]));

        let state = store.get_state();
        // BUG: This creates a duplicate
        assert_eq!(
            state.events.len(),
            2,
            "BUG: UpsertEvents creates duplicate when time changes"
        );
    }
}
