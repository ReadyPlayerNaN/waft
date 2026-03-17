//! Helper for subscribing to entity store updates with initial reconciliation.
//!
//! Wraps the common pattern of `subscribe_type` + `idle_add_local_once` that
//! every settings page needs to handle "already cached data on first load".

use std::rc::Rc;

use waft_client::EntityStore;
use waft_protocol::Urn;

/// Subscribe to entity updates and trigger an immediate reconciliation with
/// any data already cached in the store.
///
/// Sets up two callbacks:
/// 1. `subscribe_type` — fires on every future update from the daemon.
/// 2. `idle_add_local_once` — fires once after GTK event processing to
///    reconcile with data that arrived before subscription was registered.
pub fn subscribe_entities<E, F>(
    entity_store: &Rc<EntityStore>,
    entity_type: &'static str,
    callback: F,
)
where
    E: for<'de> serde::Deserialize<'de> + 'static,
    F: Fn(Vec<(Urn, E)>) + 'static + Clone,
{
    // 1. Future updates
    {
        let store = entity_store.clone();
        let cb = callback.clone();
        entity_store.subscribe_type(entity_type, move || {
            cb(store.get_entities_typed(entity_type));
        });
    }

    // 2. Initial reconciliation — deferred until after GTK event processing
    {
        let store = entity_store.clone();
        gtk::glib::idle_add_local_once(move || {
            let entities: Vec<(Urn, E)> = store.get_entities_typed(entity_type);
            if !entities.is_empty() {
                callback(entities);
            }
        });
    }
}

/// Subscribe to two entity types. Fires `callback` with both entity lists
/// whenever either type changes, plus an initial reconciliation.
pub fn subscribe_dual_entities<E1, E2, F>(
    entity_store: &Rc<EntityStore>,
    entity_type_1: &'static str,
    entity_type_2: &'static str,
    callback: F,
)
where
    E1: for<'de> serde::Deserialize<'de> + 'static,
    E2: for<'de> serde::Deserialize<'de> + 'static,
    F: Fn(Vec<(Urn, E1)>, Vec<(Urn, E2)>) + 'static + Clone,
{
    // Subscribe to first entity type
    {
        let store = entity_store.clone();
        let cb = callback.clone();
        entity_store.subscribe_type(entity_type_1, move || {
            let e1 = store.get_entities_typed(entity_type_1);
            let e2 = store.get_entities_typed(entity_type_2);
            cb(e1, e2);
        });
    }

    // Subscribe to second entity type
    {
        let store = entity_store.clone();
        let cb = callback.clone();
        entity_store.subscribe_type(entity_type_2, move || {
            let e1 = store.get_entities_typed(entity_type_1);
            let e2 = store.get_entities_typed(entity_type_2);
            cb(e1, e2);
        });
    }

    // Initial reconciliation
    {
        let store = entity_store.clone();
        gtk::glib::idle_add_local_once(move || {
            let e1: Vec<(Urn, E1)> = store.get_entities_typed(entity_type_1);
            let e2: Vec<(Urn, E2)> = store.get_entities_typed(entity_type_2);
            if !e1.is_empty() || !e2.is_empty() {
                callback(e1, e2);
            }
        });
    }
}
