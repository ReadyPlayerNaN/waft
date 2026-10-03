//! Observable entity cache with per-type subscriptions.
//!
//! Replaces the monolithic `EntityRenderer` by providing direct typed access
//! to entity data and per-entity-type change notifications. Components subscribe
//! to the entity types they care about and receive callbacks only when relevant
//! data changes.
//!
//! All access is GTK main thread only (`RefCell`, not `RwLock`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use uuid::Uuid;

use waft_protocol::Urn;
use waft_protocol::entity::notification::NOTIFICATION_ENTITY_TYPE;
use waft_protocol::message::AppNotification;

/// Callback for entity actions routed back to the daemon.
/// Parameters: (urn, action_name, params) -> action id if dispatched.
pub type EntityActionCallback = Rc<dyn Fn(Urn, String, serde_json::Value) -> Option<Uuid>>;

/// Type alias for subscriber map to reduce complexity.
type SubscriberMap = RefCell<HashMap<String, Vec<Rc<dyn Fn()>>>>;

/// Type alias for action error callback list to reduce complexity.
type ActionErrorCallbacks = RefCell<Vec<Rc<dyn Fn(Uuid, String)>>>;
type ActionErrorDetailsCallbacks =
    RefCell<Vec<Rc<dyn Fn(Uuid, waft_protocol::error::ProtocolError)>>>;

/// Type alias for action success callback list to reduce complexity.
type ActionSuccessCallbacks = RefCell<Vec<Rc<dyn Fn(Uuid, Option<serde_json::Value>)>>>;

/// A cached entity: URN, entity type, and raw JSON data.
#[derive(Clone)]
struct CachedEntity {
    urn: Urn,
    entity_type: String,
    data: serde_json::Value,
}

/// Observable entity cache that distributes notifications to per-type subscribers.
///
/// Lives on the GTK main thread. Receives `AppNotification` from the daemon
/// notification channel and routes changes to components that subscribe to
/// specific entity types.
pub struct EntityStore {
    /// Cached entity data: URN string -> CachedEntity
    cache: RefCell<HashMap<String, CachedEntity>>,
    /// Per-entity-type subscribers: entity_type -> list of callbacks
    subscribers: SubscriberMap,
    /// Callbacks invoked when an action error is received from the daemon.
    action_error_callbacks: ActionErrorCallbacks,
    action_error_details_callbacks: ActionErrorDetailsCallbacks,
    /// Callbacks invoked when an action succeeds, optionally carrying response data.
    action_success_callbacks: ActionSuccessCallbacks,
    disconnect_callbacks: RefCell<Vec<Rc<dyn Fn()>>>,
    /// Cached values awaiting confirmation from a replacement transport.
    unconfirmed: RefCell<HashSet<String>>,
}

impl EntityStore {
    pub fn new() -> Self {
        Self {
            cache: RefCell::new(HashMap::new()),
            subscribers: RefCell::new(HashMap::new()),
            action_error_callbacks: RefCell::new(Vec::new()),
            action_error_details_callbacks: RefCell::new(Vec::new()),
            action_success_callbacks: RefCell::new(Vec::new()),
            disconnect_callbacks: RefCell::new(Vec::new()),
            unconfirmed: RefCell::new(HashSet::new()),
        }
    }

    /// Process a notification from the waft daemon.
    pub fn handle_notification(&self, notification: AppNotification) {
        match notification {
            AppNotification::EntityUpdated { urn, data, .. } => {
                let entity_type = urn.entity_type().to_string();
                self.handle_entity_updated(urn, &entity_type, data);
            }
            AppNotification::EntityRemoved { urn, .. } => {
                let entity_type = urn.entity_type().to_string();
                self.handle_entity_removed(&urn, &entity_type);
            }
            AppNotification::ActionSuccess { action_id, data } => {
                log::debug!("[entity-store] action {action_id} succeeded");
                for cb in self.action_success_callbacks.borrow().iter() {
                    cb(action_id, data.clone());
                }
            }
            AppNotification::ActionError {
                action_id,
                error,
                error_details,
            } => {
                log::warn!("[entity-store] action {action_id} failed: {error}");
                let details = error_details
                    .unwrap_or_else(|| waft_protocol::error::ProtocolError::action(error.clone()));
                for cb in self.action_error_details_callbacks.borrow().iter() {
                    cb(action_id, details.clone());
                }
                for cb in self.action_error_callbacks.borrow().iter() {
                    cb(action_id, error.clone());
                }
            }
            AppNotification::EntityStale { urn, .. } => {
                let entity_type = urn.entity_type().to_string();
                log::debug!("[entity-store] entity {urn} ({entity_type}) is stale");
                self.handle_entity_removed(&urn, &entity_type);
            }
            AppNotification::EntityOutdated { urn, .. } => {
                let entity_type = urn.entity_type().to_string();
                log::debug!("[entity-store] entity {urn} ({entity_type}) is outdated");
                self.handle_entity_removed(&urn, &entity_type);
            }
            AppNotification::DescribeResponse { .. } => {
                // Description responses are handled by CLI/settings, not the entity store.
                log::debug!("[entity-store] received DescribeResponse (ignored by entity store)");
            }
            AppNotification::StatusComplete { entity_type } => {
                self.complete_reconnection_snapshot(&entity_type);
            }
            AppNotification::ProtocolError { error } => {
                log::warn!("[entity-store] received ProtocolError: {}", error.message);
            }
        }
    }

    /// Release pending action owners without treating a transport failure as entity deletion.
    pub fn handle_disconnect(&self) {
        self.unconfirmed
            .borrow_mut()
            .extend(self.cache.borrow().keys().cloned());
        let callbacks = self.disconnect_callbacks.borrow().clone();
        for callback in callbacks {
            callback();
        }
    }

    pub fn on_disconnect<F: Fn() + 'static>(&self, callback: F) {
        self.disconnect_callbacks
            .borrow_mut()
            .push(Rc::new(callback));
    }

    /// Subscribe to changes for a specific entity type.
    ///
    /// The callback is invoked whenever entities of the given type are added,
    /// updated, or removed. The subscriber should call `get_entities_by_type()`
    /// to get current state.
    pub fn subscribe_type<F>(&self, entity_type: &str, callback: F)
    where
        F: Fn() + 'static,
    {
        self.subscribers
            .borrow_mut()
            .entry(entity_type.to_string())
            .or_default()
            .push(Rc::new(callback));
    }

    /// Register a callback invoked when an action error is received from the daemon.
    ///
    /// The callback receives the action UUID and error message string.
    pub fn on_action_error<F: Fn(Uuid, String) + 'static>(&self, callback: F) {
        self.action_error_callbacks
            .borrow_mut()
            .push(Rc::new(callback));
    }

    /// Structured action failures, including a fallback for legacy plugins.
    pub fn on_action_error_details<F: Fn(Uuid, waft_protocol::error::ProtocolError) + 'static>(
        &self,
        callback: F,
    ) {
        self.action_error_details_callbacks
            .borrow_mut()
            .push(Rc::new(callback));
    }

    /// Register a callback invoked when an action succeeds.
    ///
    /// The callback receives the action UUID and optional response data.
    pub fn on_action_success<F: Fn(Uuid, Option<serde_json::Value>) + 'static>(&self, callback: F) {
        self.action_success_callbacks
            .borrow_mut()
            .push(Rc::new(callback));
    }

    /// Get all cached entities of a given type as typed values.
    ///
    /// Returns a vec of (Urn, T) pairs. Entities that fail to deserialize
    /// are silently skipped (logged at warn level).
    pub fn get_entities_typed<T: serde::de::DeserializeOwned>(
        &self,
        entity_type: &str,
    ) -> Vec<(Urn, T)> {
        let cache = self.cache.borrow();
        cache
            .values()
            .filter(|e| e.entity_type == entity_type)
            .filter_map(|e| match serde_json::from_value(e.data.clone()) {
                Ok(typed) => Some((e.urn.clone(), typed)),
                Err(err) => {
                    log::warn!(
                        "[entity-store] failed to deserialize {} ({}): {err}",
                        e.urn,
                        e.entity_type,
                    );
                    None
                }
            })
            .collect()
    }

    /// Get a single entity by URN as a typed value.
    pub fn get_entity_typed<T: serde::de::DeserializeOwned>(&self, urn: &Urn) -> Option<T> {
        let cache = self.cache.borrow();
        cache
            .get(urn.as_str())
            .and_then(|e| match serde_json::from_value(e.data.clone()) {
                Ok(typed) => Some(typed),
                Err(err) => {
                    log::warn!(
                        "[entity-store] failed to deserialize {} ({}): {err}",
                        e.urn,
                        e.entity_type,
                    );
                    None
                }
            })
    }

    /// Returns true if the entity with this URN is currently in the store.
    pub fn has_entity(&self, urn: &Urn) -> bool {
        self.cache.borrow().contains_key(urn.as_str())
    }

    /// Get all cached entities of a given type as raw (Urn, Value) pairs.
    pub fn get_entities_raw(&self, entity_type: &str) -> Vec<(Urn, serde_json::Value)> {
        let cache = self.cache.borrow();
        cache
            .values()
            .filter(|e| e.entity_type == entity_type)
            .map(|e| (e.urn.clone(), e.data.clone()))
            .collect()
    }

    fn handle_entity_updated(&self, urn: Urn, entity_type: &str, data: serde_json::Value) {
        let urn_str = urn.as_str().to_string();

        // Equal data still confirms recovery once after transport replacement.
        let confirmed = self.unconfirmed.borrow_mut().remove(&urn_str);
        // Skip ordinary unchanged updates.
        {
            let cache = self.cache.borrow();
            if let Some(cached) = cache.get(&urn_str)
                && cached.data == data
                && !confirmed
            {
                return;
            }
        }

        self.cache.borrow_mut().insert(
            urn_str,
            CachedEntity {
                urn,
                entity_type: entity_type.to_string(),
                data,
            },
        );

        self.notify_type(entity_type);
    }

    /// A completed daemon snapshot is the authority for membership after
    /// reconnect. Missing values stay cached until this explicit boundary;
    /// legacy daemons or interrupted snapshots never imply deletion.
    fn complete_reconnection_snapshot(&self, entity_type: &str) {
        let removed = {
            let mut cache = self.cache.borrow_mut();
            let mut unconfirmed = self.unconfirmed.borrow_mut();
            let missing: Vec<_> = cache
                .iter()
                .filter(|(urn, entity)| {
                    entity.entity_type == entity_type && unconfirmed.contains(*urn)
                })
                .map(|(urn, _)| urn.clone())
                .collect();
            for urn in &missing {
                cache.remove(urn);
                unconfirmed.remove(urn);
            }
            !missing.is_empty()
        };
        if removed {
            self.notify_type(entity_type);
        }
    }

    fn handle_entity_removed(&self, urn: &Urn, entity_type: &str) {
        let urn_str = urn.as_str().to_string();
        self.unconfirmed.borrow_mut().remove(&urn_str);
        let removed = self.cache.borrow_mut().remove(&urn_str).is_some();
        if removed || entity_type == NOTIFICATION_ENTITY_TYPE {
            self.notify_type(entity_type);
        }
    }

    fn notify_type(&self, entity_type: &str) {
        let subscribers = self.subscribers.borrow();
        if let Some(callbacks) = subscribers.get(entity_type) {
            for cb in callbacks {
                cb();
            }
        }
    }
}

impl Default for EntityStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use waft_protocol::entity;

    #[test]
    fn equal_reconnected_value_confirms_recovery_without_deleting_cached_rows() {
        let store = EntityStore::new();
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        store.subscribe_type("clock", move || seen.set(seen.get() + 1));
        let urn = Urn::new("clock", "clock", "singleton");
        let notification = make_updated(urn, "clock", serde_json::json!({"time":"fixture"}));
        store.handle_notification(notification.clone());
        store.handle_notification(notification.clone());
        assert_eq!(calls.get(), 1);
        store.handle_disconnect();
        assert_eq!(store.get_entities_raw("clock").len(), 1);
        assert_eq!(calls.get(), 1);
        store.handle_notification(notification.clone());
        assert_eq!(calls.get(), 2, "freshness is an observable change");
        store.handle_notification(notification);
        assert_eq!(calls.get(), 2);
    }
    #[test]
    fn completed_reconnect_snapshot_removes_only_absent_members() {
        let store = EntityStore::new();
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        store.subscribe_type("online-account", move || seen.set(seen.get() + 1));
        let retained = Urn::new("goa", "online-account", "retained");
        let removed = Urn::new("goa", "online-account", "removed");
        let other = Urn::new("eds", "calendar-event", "stale");
        let retained_update = make_updated(
            retained.clone(),
            "online-account",
            serde_json::json!({"name":"fixture"}),
        );
        store.handle_notification(retained_update.clone());
        store.handle_notification(make_updated(
            removed.clone(),
            "online-account",
            serde_json::json!({}),
        ));
        store.handle_notification(make_updated(
            other.clone(),
            "calendar-event",
            serde_json::json!({}),
        ));
        store.handle_disconnect();
        store.handle_notification(retained_update);
        assert!(
            store.has_entity(&removed),
            "partial snapshots retain membership"
        );
        let before = calls.get();
        store.handle_notification(AppNotification::StatusComplete {
            entity_type: "online-account".into(),
        });
        assert!(store.has_entity(&retained));
        assert!(!store.has_entity(&removed));
        assert!(store.has_entity(&other), "uncompleted types remain cached");
        assert_eq!(
            calls.get(),
            before + 1,
            "one reconciliation after missing members are removed"
        );
        store.handle_notification(AppNotification::StatusComplete {
            entity_type: "online-account".into(),
        });
        assert_eq!(calls.get(), before + 1, "duplicate completion is harmless");
    }

    #[test]
    fn empty_reconnect_snapshot_is_authoritative_only_after_completion() {
        let store = EntityStore::new();
        let urn = Urn::new("eds", "calendar-event", "fixture");
        store.handle_notification(make_updated(
            urn.clone(),
            "calendar-event",
            serde_json::json!({}),
        ));
        store.handle_notification(AppNotification::StatusComplete {
            entity_type: "calendar-event".into(),
        });
        assert!(
            store.has_entity(&urn),
            "ordinary completion does not delete fresh members"
        );
        store.handle_disconnect();
        assert!(store.has_entity(&urn), "disconnect is not deletion");
        store.handle_notification(AppNotification::StatusComplete {
            entity_type: "calendar-event".into(),
        });
        assert!(
            !store.has_entity(&urn),
            "completed empty snapshot retires stale membership"
        );
    }

    #[test]
    fn interrupted_or_legacy_reconnect_retains_unconfirmed_members() {
        let store = EntityStore::new();
        let old = Urn::new("eds", "calendar-event", "old");
        let partial = Urn::new("eds", "calendar-event", "partial");
        store.handle_notification(make_updated(
            old.clone(),
            "calendar-event",
            serde_json::json!({}),
        ));
        store.handle_disconnect();
        store.handle_notification(make_updated(
            partial.clone(),
            "calendar-event",
            serde_json::json!({}),
        ));
        store.handle_disconnect();
        assert!(store.has_entity(&old));
        assert!(
            store.has_entity(&partial),
            "no completion means no authoritative deletion"
        );
        store.handle_notification(make_updated(
            partial.clone(),
            "calendar-event",
            serde_json::json!({}),
        ));
        assert!(
            store.has_entity(&old),
            "legacy daemon without StatusComplete remains conservative"
        );
        store.handle_notification(AppNotification::StatusComplete {
            entity_type: "calendar-event".into(),
        });
        assert!(!store.has_entity(&old));
        assert!(store.has_entity(&partial));
    }

    fn make_updated(urn: Urn, entity_type: &str, data: serde_json::Value) -> AppNotification {
        AppNotification::EntityUpdated {
            urn,
            entity_type: Some(entity_type.to_string()),
            data,
        }
    }

    fn make_removed(urn: Urn, entity_type: &str) -> AppNotification {
        AppNotification::EntityRemoved {
            urn,
            entity_type: Some(entity_type.to_string()),
        }
    }

    #[test]
    fn subscribe_and_notify() {
        let store = EntityStore::new();
        let called = Rc::new(Cell::new(0u32));
        let called_clone = called.clone();

        store.subscribe_type(entity::clock::ENTITY_TYPE, move || {
            called_clone.set(called_clone.get() + 1);
        });

        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .expect("expected value");

        store.handle_notification(make_updated(urn, entity::clock::ENTITY_TYPE, data));
        assert_eq!(called.get(), 1);
    }

    #[test]
    fn deduplication_skips_unchanged() {
        let store = EntityStore::new();
        let called = Rc::new(Cell::new(0u32));
        let called_clone = called.clone();

        store.subscribe_type(entity::clock::ENTITY_TYPE, move || {
            called_clone.set(called_clone.get() + 1);
        });

        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .expect("expected value");

        store.handle_notification(make_updated(
            urn.clone(),
            entity::clock::ENTITY_TYPE,
            data.clone(),
        ));
        store.handle_notification(make_updated(urn, entity::clock::ENTITY_TYPE, data));
        assert_eq!(
            called.get(),
            1,
            "identical data should not trigger notification"
        );
    }

    #[test]
    fn get_entities_typed() {
        let store = EntityStore::new();
        let urn = Urn::new("clock", "clock", "default");
        let clock = entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        };
        let data = serde_json::to_value(&clock).expect("expected value");

        store.handle_notification(make_updated(urn, entity::clock::ENTITY_TYPE, data));

        let entities: Vec<(Urn, entity::clock::Clock)> =
            store.get_entities_typed(entity::clock::ENTITY_TYPE);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].1.time, "14:30");
    }

    #[test]
    fn get_entity_typed_by_urn() {
        let store = EntityStore::new();
        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "15:00".to_string(),
            date: "Friday".to_string(),
        })
        .expect("expected value");

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data));

        let clock: Option<entity::clock::Clock> = store.get_entity_typed(&urn);
        assert!(clock.is_some());
        assert_eq!(clock.expect("expected value").time, "15:00");
    }

    #[test]
    fn entity_removed_notifies() {
        let store = EntityStore::new();
        let called = Rc::new(Cell::new(0u32));
        let called_clone = called.clone();

        store.subscribe_type(entity::clock::ENTITY_TYPE, move || {
            called_clone.set(called_clone.get() + 1);
        });

        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .expect("expected value");

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data));
        assert_eq!(called.get(), 1);

        store.handle_notification(make_removed(urn, entity::clock::ENTITY_TYPE));
        assert_eq!(called.get(), 2);

        let entities: Vec<(Urn, entity::clock::Clock)> =
            store.get_entities_typed(entity::clock::ENTITY_TYPE);
        assert!(entities.is_empty());
    }

    #[test]
    fn remove_nonexistent_does_not_notify() {
        let store = EntityStore::new();
        let called = Rc::new(Cell::new(0u32));
        let called_clone = called.clone();

        store.subscribe_type(entity::clock::ENTITY_TYPE, move || {
            called_clone.set(called_clone.get() + 1);
        });

        let urn = Urn::new("clock", "clock", "default");
        store.handle_notification(make_removed(urn, entity::clock::ENTITY_TYPE));
        assert_eq!(called.get(), 0);
    }

    #[test]
    fn notification_remove_still_notifies_after_cache_is_empty() {
        let store = EntityStore::new();
        let called = Rc::new(Cell::new(0u32));
        let called_clone = called.clone();

        store.subscribe_type(entity::notification::NOTIFICATION_ENTITY_TYPE, move || {
            called_clone.set(called_clone.get() + 1);
        });

        let urn = Urn::new("notifications", "notification", "1");

        store.handle_notification(make_removed(
            urn.clone(),
            entity::notification::NOTIFICATION_ENTITY_TYPE,
        ));
        assert_eq!(called.get(), 1);

        store.handle_notification(make_removed(
            urn,
            entity::notification::NOTIFICATION_ENTITY_TYPE,
        ));
        assert_eq!(called.get(), 2);
    }

    #[test]
    fn different_types_isolated() {
        let store = EntityStore::new();
        let clock_called = Rc::new(Cell::new(0u32));
        let audio_called = Rc::new(Cell::new(0u32));

        let cc = clock_called.clone();
        store.subscribe_type(entity::clock::ENTITY_TYPE, move || {
            cc.set(cc.get() + 1);
        });

        let ac = audio_called.clone();
        store.subscribe_type(entity::audio::ENTITY_TYPE, move || {
            ac.set(ac.get() + 1);
        });

        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .expect("expected value");

        store.handle_notification(make_updated(urn, entity::clock::ENTITY_TYPE, data));
        assert_eq!(clock_called.get(), 1);
        assert_eq!(
            audio_called.get(),
            0,
            "audio subscriber should not be triggered by clock update"
        );
    }

    #[test]
    fn entity_stale_removes() {
        let store = EntityStore::new();
        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .expect("expected value");

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data));
        assert_eq!(
            store
                .get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE)
                .len(),
            1
        );

        store.handle_notification(AppNotification::EntityStale {
            urn,
            entity_type: Some(entity::clock::ENTITY_TYPE.to_string()),
        });
        assert_eq!(
            store
                .get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE)
                .len(),
            0
        );
    }

    #[test]
    fn action_success_callback_fires_with_data() {
        let store = EntityStore::new();
        let received = Rc::new(RefCell::new(None));
        let received_clone = received.clone();

        store.on_action_success(move |id, data| {
            *received_clone.borrow_mut() = Some((id, data));
        });

        let action_id = Uuid::new_v4();
        let response_data = serde_json::json!({"qr_string": "WIFI:T:WPA;S:Net;P:pass;;"});
        store.handle_notification(AppNotification::ActionSuccess {
            action_id,
            data: Some(response_data.clone()),
        });

        let result = received.borrow();
        let (id, data) = result.as_ref().expect("callback should have fired");
        assert_eq!(*id, action_id);
        assert_eq!(*data, Some(response_data));
    }

    #[test]
    fn action_success_callback_fires_without_data() {
        let store = EntityStore::new();
        let received = Rc::new(RefCell::new(None));
        let received_clone = received.clone();

        store.on_action_success(move |id, data| {
            *received_clone.borrow_mut() = Some((id, data));
        });

        let action_id = Uuid::new_v4();
        store.handle_notification(AppNotification::ActionSuccess {
            action_id,
            data: None,
        });

        let result = received.borrow();
        let (id, data) = result.as_ref().expect("callback should have fired");
        assert_eq!(*id, action_id);
        assert_eq!(*data, None);
    }

    #[test]
    fn action_error_callback_fires() {
        let store = EntityStore::new();
        let received = Rc::new(RefCell::new(None));
        let received_clone = received.clone();

        store.on_action_error(move |id, error| {
            *received_clone.borrow_mut() = Some((id, error));
        });

        let action_id = Uuid::new_v4();
        store.handle_notification(AppNotification::ActionError {
            action_id,
            error: "device not found".to_string(),
            error_details: None,
        });

        let result = received.borrow();
        let (id, error) = result.as_ref().expect("callback should have fired");
        assert_eq!(*id, action_id);
        assert_eq!(*error, "device not found");
    }

    #[test]
    fn entity_outdated_removes() {
        let store = EntityStore::new();
        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .expect("expected value");

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data));
        assert_eq!(
            store
                .get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE)
                .len(),
            1
        );

        store.handle_notification(AppNotification::EntityOutdated {
            urn,
            entity_type: Some(entity::clock::ENTITY_TYPE.to_string()),
        });
        assert_eq!(
            store
                .get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE)
                .len(),
            0
        );
    }
}
