use std::collections::{HashMap, HashSet};

use uuid::Uuid;
use waft_protocol::Urn;

/// Tracks which plugin name is served by which connection.
pub struct PluginRegistry {
    /// Plugin name -> connection ID.
    plugins: HashMap<String, Uuid>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: HashMap::new(),
        }
    }

    /// Register a plugin name to a connection.
    pub fn register(&mut self, plugin_name: String, conn_id: Uuid) {
        eprintln!("[waft] plugin registered: {plugin_name} (conn {conn_id})");
        self.plugins.insert(plugin_name, conn_id);
    }

    /// Find the connection that owns the plugin referenced in a URN.
    pub fn connection_for_urn(&self, urn: &Urn) -> Option<Uuid> {
        self.plugins.get(urn.plugin()).copied()
    }

    /// Find the connection ID for a plugin by name.
    pub fn connection_for_plugin(&self, name: &str) -> Option<Uuid> {
        self.plugins.get(name).copied()
    }

    /// Return all registered plugin names.
    pub fn all_plugin_names(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }

    /// Remove all entries for a disconnected connection.
    pub fn remove_connection(&mut self, conn_id: Uuid) {
        self.plugins.retain(|name, id| {
            if *id == conn_id {
                eprintln!("[waft] plugin unregistered: {name}");
                false
            } else {
                true
            }
        });
    }
}

/// Tracks which app connections subscribe to which entity types.
pub struct AppRegistry {
    /// Entity type -> set of subscribed app connection IDs.
    subscriptions: HashMap<String, HashSet<Uuid>>,
}

impl AppRegistry {
    pub fn new() -> Self {
        AppRegistry {
            subscriptions: HashMap::new(),
        }
    }

    /// Subscribe a connection to an entity type.
    pub fn subscribe(&mut self, entity_type: String, conn_id: Uuid) {
        self.subscriptions
            .entry(entity_type)
            .or_default()
            .insert(conn_id);
    }

    /// Unsubscribe a connection from an entity type.
    pub fn unsubscribe(&mut self, entity_type: &str, conn_id: Uuid) {
        if let Some(subs) = self.subscriptions.get_mut(entity_type) {
            subs.remove(&conn_id);
            if subs.is_empty() {
                self.subscriptions.remove(entity_type);
            }
        }
    }

    /// Get all connections subscribed to an entity type.
    pub fn subscribers(&self, entity_type: &str) -> Vec<Uuid> {
        self.subscriptions
            .get(entity_type)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Check if an entity type has any subscribers.
    pub fn has_subscribers(&self, entity_type: &str) -> bool {
        self.subscriptions
            .get(entity_type)
            .is_some_and(|s| !s.is_empty())
    }

    /// Remove all subscriptions for a disconnected connection.
    pub fn remove_connection(&mut self, conn_id: Uuid) {
        self.subscriptions.retain(|_, subs| {
            subs.remove(&conn_id);
            !subs.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registry_register_and_lookup() {
        let mut reg = PluginRegistry::new();
        let conn = Uuid::new_v4();

        reg.register("clock".to_string(), conn);

        let urn = Urn::new("clock", "clock", "default");
        assert_eq!(reg.connection_for_urn(&urn), Some(conn));

        let other_urn = Urn::new("battery", "battery", "BAT0");
        assert_eq!(reg.connection_for_urn(&other_urn), None);
    }

    #[test]
    fn plugin_registry_remove_connection() {
        let mut reg = PluginRegistry::new();
        let conn = Uuid::new_v4();

        reg.register("clock".to_string(), conn);
        reg.remove_connection(conn);

        let urn = Urn::new("clock", "clock", "default");
        assert_eq!(reg.connection_for_urn(&urn), None);
    }

    #[test]
    fn app_registry_subscribe_and_query() {
        let mut reg = AppRegistry::new();
        let app1 = Uuid::new_v4();
        let app2 = Uuid::new_v4();

        reg.subscribe("clock".to_string(), app1);
        reg.subscribe("clock".to_string(), app2);
        reg.subscribe("battery".to_string(), app1);

        let clock_subs = reg.subscribers("clock");
        assert_eq!(clock_subs.len(), 2);
        assert!(clock_subs.contains(&app1));
        assert!(clock_subs.contains(&app2));

        let battery_subs = reg.subscribers("battery");
        assert_eq!(battery_subs, vec![app1]);

        assert!(reg.subscribers("weather").is_empty());
    }

    #[test]
    fn app_registry_unsubscribe() {
        let mut reg = AppRegistry::new();
        let app = Uuid::new_v4();

        reg.subscribe("clock".to_string(), app);
        reg.unsubscribe("clock", app);

        assert!(reg.subscribers("clock").is_empty());
    }

    #[test]
    fn app_registry_remove_connection() {
        let mut reg = AppRegistry::new();
        let app = Uuid::new_v4();

        reg.subscribe("clock".to_string(), app);
        reg.subscribe("battery".to_string(), app);
        reg.remove_connection(app);

        assert!(reg.subscribers("clock").is_empty());
        assert!(reg.subscribers("battery").is_empty());
    }
}
