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
