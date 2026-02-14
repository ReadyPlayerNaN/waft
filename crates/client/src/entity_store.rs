//! Observable entity cache with per-type subscriptions.
//!
//! Replaces the monolithic `EntityRenderer` by providing direct typed access
//! to entity data and per-entity-type change notifications. Components subscribe
//! to the entity types they care about and receive callbacks only when relevant
//! data changes.
//!
//! All access is GTK main thread only (`RefCell`, not `RwLock`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use waft_protocol::message::AppNotification;
use waft_protocol::Urn;

/// Callback for entity actions routed back to the daemon.
/// Parameters: (urn, action_name, params)
pub type EntityActionCallback = Rc<dyn Fn(Urn, String, serde_json::Value)>;

/// Type alias for subscriber map to reduce complexity.
type SubscriberMap = RefCell<HashMap<String, Vec<Rc<dyn Fn()>>>>;

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
}

impl EntityStore {
    pub fn new() -> Self {
        Self {
            cache: RefCell::new(HashMap::new()),
            subscribers: RefCell::new(HashMap::new()),
        }
    }

    /// Process a notification from the waft daemon.
    pub fn handle_notification(&self, notification: AppNotification) {
        match notification {
            AppNotification::EntityUpdated {
                urn,
                entity_type,
                data,
            } => {
                self.handle_entity_updated(urn, entity_type, data);
            }
            AppNotification::EntityRemoved { urn, entity_type } => {
                self.handle_entity_removed(&urn, &entity_type);
            }
            AppNotification::ActionSuccess { action_id } => {
                log::debug!("[entity-store] action {action_id} succeeded");
            }
            AppNotification::ActionError { action_id, error } => {
                log::warn!("[entity-store] action {action_id} failed: {error}");
            }
            AppNotification::EntityStale { urn, entity_type } => {
                log::debug!("[entity-store] entity {urn} ({entity_type}) is stale");
                self.handle_entity_removed(&urn, &entity_type);
            }
            AppNotification::EntityOutdated { urn, entity_type } => {
                log::debug!("[entity-store] entity {urn} ({entity_type}) is outdated");
                self.handle_entity_removed(&urn, &entity_type);
            }
        }
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
            .filter_map(|e| {
                match serde_json::from_value(e.data.clone()) {
                    Ok(typed) => Some((e.urn.clone(), typed)),
                    Err(err) => {
                        log::warn!(
                            "[entity-store] failed to deserialize {} ({}): {err}",
                            e.urn,
                            e.entity_type,
                        );
                        None
                    }
                }
            })
            .collect()
    }

    /// Get a single entity by URN as a typed value.
    pub fn get_entity_typed<T: serde::de::DeserializeOwned>(&self, urn: &Urn) -> Option<T> {
        let cache = self.cache.borrow();
        cache.get(urn.as_str()).and_then(|e| {
            match serde_json::from_value(e.data.clone()) {
                Ok(typed) => Some(typed),
                Err(err) => {
                    log::warn!(
                        "[entity-store] failed to deserialize {} ({}): {err}",
                        e.urn,
                        e.entity_type,
                    );
                    None
                }
            }
        })
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

    fn handle_entity_updated(
        &self,
        urn: Urn,
        entity_type: String,
        data: serde_json::Value,
    ) {
        let urn_str = urn.as_str().to_string();

        // Skip if data unchanged
        {
            let cache = self.cache.borrow();
            if let Some(cached) = cache.get(&urn_str)
                && cached.data == data {
                    return;
                }
        }

        self.cache.borrow_mut().insert(
            urn_str,
            CachedEntity {
                urn,
                entity_type: entity_type.clone(),
                data,
            },
        );

        self.notify_type(&entity_type);
    }

    fn handle_entity_removed(&self, urn: &Urn, entity_type: &str) {
        let urn_str = urn.as_str().to_string();
        if self.cache.borrow_mut().remove(&urn_str).is_some() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use waft_protocol::entity;

    fn make_updated(urn: Urn, entity_type: &str, data: serde_json::Value) -> AppNotification {
        AppNotification::EntityUpdated {
            urn,
            entity_type: entity_type.to_string(),
            data,
        }
    }

    fn make_removed(urn: Urn, entity_type: &str) -> AppNotification {
        AppNotification::EntityRemoved {
            urn,
            entity_type: entity_type.to_string(),
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
        .unwrap();

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
        .unwrap();

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data.clone()));
        store.handle_notification(make_updated(urn, entity::clock::ENTITY_TYPE, data));
        assert_eq!(called.get(), 1, "identical data should not trigger notification");
    }

    #[test]
    fn get_entities_typed() {
        let store = EntityStore::new();
        let urn = Urn::new("clock", "clock", "default");
        let clock = entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        };
        let data = serde_json::to_value(&clock).unwrap();

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
        .unwrap();

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data));

        let clock: Option<entity::clock::Clock> = store.get_entity_typed(&urn);
        assert!(clock.is_some());
        assert_eq!(clock.unwrap().time, "15:00");
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
        .unwrap();

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
        .unwrap();

        store.handle_notification(make_updated(urn, entity::clock::ENTITY_TYPE, data));
        assert_eq!(clock_called.get(), 1);
        assert_eq!(audio_called.get(), 0, "audio subscriber should not be triggered by clock update");
    }

    #[test]
    fn entity_stale_removes() {
        let store = EntityStore::new();
        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .unwrap();

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data));
        assert_eq!(store.get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE).len(), 1);

        store.handle_notification(AppNotification::EntityStale {
            urn,
            entity_type: entity::clock::ENTITY_TYPE.to_string(),
        });
        assert_eq!(store.get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE).len(), 0);
    }

    #[test]
    fn entity_outdated_removes() {
        let store = EntityStore::new();
        let urn = Urn::new("clock", "clock", "default");
        let data = serde_json::to_value(entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday".to_string(),
        })
        .unwrap();

        store.handle_notification(make_updated(urn.clone(), entity::clock::ENTITY_TYPE, data));
        assert_eq!(store.get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE).len(), 1);

        store.handle_notification(AppNotification::EntityOutdated {
            urn,
            entity_type: entity::clock::ENTITY_TYPE.to_string(),
        });
        assert_eq!(store.get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE).len(), 0);
    }
}
