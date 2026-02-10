//! Action routing for mapping widget actions to plugin clients
//!
//! The ActionRouter maintains a mapping from widget IDs to plugin IDs and routes
//! user actions (button clicks, slider changes, etc.) to the correct plugin daemon
//! via channel-based communication.

use std::collections::HashMap;
use waft_ipc::Action;

use super::client::ClientError;

/// Errors that can occur during action routing
#[derive(Debug)]
pub enum RouterError {
    /// Widget ID not found in routing table
    WidgetNotFound(String),

    /// Plugin for widget is not connected
    PluginNotConnected(String),

    /// Client error occurred while sending action
    ClientError(ClientError),
}

impl std::fmt::Display for RouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterError::WidgetNotFound(id) => write!(f, "widget not found: {}", id),
            RouterError::PluginNotConnected(id) => write!(f, "plugin not connected: {}", id),
            RouterError::ClientError(e) => write!(f, "client error: {}", e),
        }
    }
}

impl std::error::Error for RouterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RouterError::ClientError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ClientError> for RouterError {
    fn from(e: ClientError) -> Self {
        RouterError::ClientError(e)
    }
}

/// Routes widget actions to the appropriate plugin clients via channels.
///
/// The router maintains two key mappings:
/// 1. widget_id -> plugin_id: to find which plugin owns a widget
/// 2. plugin_id -> PluginClient: to send messages to the plugin
pub struct ActionRouter {
    /// Maps widget_id -> plugin_id
    widget_to_plugin: HashMap<String, String>,

    /// Maps plugin_id -> PluginClient
    clients: HashMap<String, super::client::PluginClient>,
}

impl ActionRouter {
    /// Create a new action router
    pub fn new() -> Self {
        Self {
            widget_to_plugin: HashMap::new(),
            clients: HashMap::new(),
        }
    }

    /// Register a plugin client
    pub fn register_client(&mut self, plugin_id: String, client: super::client::PluginClient) {
        log::debug!("[action-router] registered plugin: {}", plugin_id);
        self.clients.insert(plugin_id, client);
    }

    /// Unregister a plugin client
    pub fn unregister_client(&mut self, plugin_id: &str) {
        log::debug!("[action-router] unregistered plugin: {}", plugin_id);
        self.clients.remove(plugin_id);
        self.widget_to_plugin.retain(|_, p| p != plugin_id);
    }

    /// Map a widget to its owning plugin
    pub fn map_widget(&mut self, widget_id: String, plugin_id: String) {
        self.widget_to_plugin.insert(widget_id, plugin_id);
    }

    /// Map multiple widgets to a plugin
    pub fn map_widgets(&mut self, plugin_id: &str, widget_ids: &[String]) {
        for widget_id in widget_ids {
            self.widget_to_plugin
                .insert(widget_id.clone(), plugin_id.to_string());
        }
    }

    /// Remove a widget mapping
    pub fn unmap_widget(&mut self, widget_id: &str) {
        self.widget_to_plugin.remove(widget_id);
    }

    /// Route an action to the appropriate plugin (sync — sends via channel)
    pub fn route_action(
        &self,
        widget_id: String,
        action: Action,
    ) -> Result<(), RouterError> {
        log::debug!(
            "[action-router] routing action for widget {}: {:?}",
            widget_id,
            action
        );

        let plugin_id = self
            .widget_to_plugin
            .get(&widget_id)
            .ok_or_else(|| RouterError::WidgetNotFound(widget_id.clone()))?;

        let client = self
            .clients
            .get(plugin_id)
            .ok_or_else(|| RouterError::PluginNotConnected(plugin_id.clone()))?;

        client.send_action(widget_id, action)?;

        log::debug!("[action-router] action routed successfully");
        Ok(())
    }

    /// Get the plugin ID that owns a widget
    pub fn get_plugin_for_widget(&self, widget_id: &str) -> Option<&str> {
        self.widget_to_plugin.get(widget_id).map(|s| s.as_str())
    }

    /// Check if a plugin client is registered
    pub fn has_client(&self, plugin_id: &str) -> bool {
        self.clients.contains_key(plugin_id)
    }

    /// Get the number of registered clients
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Get the number of mapped widgets
    pub fn widget_count(&self) -> usize {
        self.widget_to_plugin.len()
    }

    /// Get all registered plugin IDs
    pub fn plugin_ids(&self) -> Vec<&str> {
        self.clients.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ActionRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_router_is_empty() {
        let router = ActionRouter::new();
        assert_eq!(router.client_count(), 0);
        assert_eq!(router.widget_count(), 0);
    }

    #[test]
    fn test_map_widget() {
        let mut router = ActionRouter::new();
        router.map_widget("widget1".to_string(), "audio".to_string());

        assert_eq!(router.get_plugin_for_widget("widget1"), Some("audio"));
        assert_eq!(router.widget_count(), 1);
    }

    #[test]
    fn test_map_widgets() {
        let mut router = ActionRouter::new();
        let widget_ids = vec!["w1".to_string(), "w2".to_string(), "w3".to_string()];

        router.map_widgets("audio", &widget_ids);

        assert_eq!(router.widget_count(), 3);
        assert_eq!(router.get_plugin_for_widget("w1"), Some("audio"));
        assert_eq!(router.get_plugin_for_widget("w2"), Some("audio"));
        assert_eq!(router.get_plugin_for_widget("w3"), Some("audio"));
    }

    #[test]
    fn test_unmap_widget() {
        let mut router = ActionRouter::new();
        router.map_widget("widget1".to_string(), "audio".to_string());
        assert_eq!(router.widget_count(), 1);

        router.unmap_widget("widget1");
        assert_eq!(router.widget_count(), 0);
        assert_eq!(router.get_plugin_for_widget("widget1"), None);
    }

    #[test]
    fn test_map_widget_replaces_previous_mapping() {
        let mut router = ActionRouter::new();
        router.map_widget("widget1".to_string(), "audio".to_string());
        router.map_widget("widget1".to_string(), "battery".to_string());

        assert_eq!(router.get_plugin_for_widget("widget1"), Some("battery"));
        assert_eq!(router.widget_count(), 1);
    }

    #[test]
    fn test_has_client() {
        let router = ActionRouter::new();
        assert!(!router.has_client("audio"));
    }

    #[test]
    fn test_plugin_ids_empty() {
        let router = ActionRouter::new();
        assert!(router.plugin_ids().is_empty());
    }

    #[test]
    fn test_get_plugin_for_nonexistent_widget() {
        let router = ActionRouter::new();
        assert_eq!(router.get_plugin_for_widget("nonexistent"), None);
    }

    #[test]
    fn test_router_error_display() {
        let err = RouterError::WidgetNotFound("widget1".to_string());
        assert_eq!(err.to_string(), "widget not found: widget1");

        let err = RouterError::PluginNotConnected("audio".to_string());
        assert_eq!(err.to_string(), "plugin not connected: audio");
    }

    #[test]
    fn test_default_impl() {
        let router = ActionRouter::default();
        assert_eq!(router.client_count(), 0);
        assert_eq!(router.widget_count(), 0);
    }
}
