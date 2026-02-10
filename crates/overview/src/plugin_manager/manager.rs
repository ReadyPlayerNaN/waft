//! Plugin manager that orchestrates event-driven IPC-based plugin communication
//!
//! This module provides the main PluginManager that coordinates all IPC components:
//! - Plugin discovery (scanning for .sock files)
//! - Client connections (PluginClient for each daemon)
//! - Widget registry (tracking widget state)
//! - Action routing (sending user actions to plugins)
//!
//! **No polling**: Plugins push updates via their WidgetNotifier, and the manager
//! receives them event-driven via a merged channel.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use waft_ipc::{NamedWidget, PluginMessage};

use super::client::{InternalMessage, PluginClient};
use super::discovery::{discover_plugins, PluginInfo};
use super::registry::WidgetRegistry;
use super::router::ActionRouter;

/// Shared action router handle that can be used from any thread (including GTK).
///
/// Actions are routed directly to plugin clients without going through the
/// PluginManager's event loop, avoiding cross-runtime latency issues.
pub type SharedRouter = Arc<Mutex<ActionRouter>>;

/// Update event sent from PluginManager to the UI
#[derive(Debug, Clone)]
pub enum PluginUpdate {
    /// Full widget set from all plugins (sent on initial load)
    FullUpdate { widgets: Vec<NamedWidget> },

    /// A plugin connected
    PluginConnected { plugin_id: String },

    /// A plugin disconnected
    PluginDisconnected { plugin_id: String },

    /// An error occurred
    Error { plugin_id: String, error: String },
}

/// Configuration for PluginManager behavior
#[derive(Debug, Clone, Default)]
pub struct PluginManagerConfig {}

/// Main coordinator for event-driven IPC-based plugin system
///
/// The PluginManager orchestrates:
/// 1. **Discovery**: Scans for plugin sockets and connects clients
/// 2. **Event loop**: Receives pushed widget updates from daemons
/// 3. **Registry**: Maintains current widget state for all plugins
/// 4. **Routing**: Maintains the shared router for direct action dispatch
pub struct PluginManager {
    registry: WidgetRegistry,
    router: SharedRouter,
    update_tx: mpsc::UnboundedSender<PluginUpdate>,
    /// Merged channel for all plugin messages and disconnections
    merged_tx: mpsc::UnboundedSender<InternalMessage>,
    merged_rx: mpsc::UnboundedReceiver<InternalMessage>,
}

impl PluginManager {
    /// Create a new PluginManager
    ///
    /// Returns a tuple of (PluginManager, update receiver, shared action router).
    /// The shared router can be used from any thread to route actions directly
    /// to plugin clients, bypassing the PluginManager's event loop.
    pub fn new(
        _config: PluginManagerConfig,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<PluginUpdate>,
        SharedRouter,
    ) {
        let (update_tx, update_rx) = mpsc::unbounded_channel();
        let (merged_tx, merged_rx) = mpsc::unbounded_channel();
        let router = Arc::new(Mutex::new(ActionRouter::new()));

        let manager = Self {
            registry: WidgetRegistry::new(),
            router: router.clone(),
            update_tx,
            merged_tx,
            merged_rx,
        };

        (manager, update_rx, router)
    }

    /// Run the plugin manager event loop (blocks until shutdown)
    ///
    /// This is fully event-driven: plugin messages arrive via merged channel
    /// (pushed by daemons). Actions are routed directly via the SharedRouter
    /// from the GTK thread, bypassing this event loop.
    pub async fn run(&mut self) {
        log::info!("[plugin-manager] Starting plugin manager (event-driven)");

        // One-time discovery and connection at startup
        self.discover_and_connect().await;

        // Send initial full widget state to UI
        self.send_full_update();

        while let Some(internal) = self.merged_rx.recv().await {
            match internal {
                InternalMessage::Plugin { plugin_id, msg } => {
                    self.handle_plugin_message(&plugin_id, msg);
                }
                InternalMessage::Disconnected { plugin_id } => {
                    self.handle_disconnection(&plugin_id);
                }
            }
        }
    }

    /// Handle an incoming plugin message (SetWidgets, UpdateWidget, RemoveWidget)
    fn handle_plugin_message(&mut self, plugin_id: &str, msg: PluginMessage) {
        match msg {
            PluginMessage::SetWidgets { widgets } => {
                log::debug!(
                    "[plugin-manager] Received {} widgets from {}",
                    widgets.len(),
                    plugin_id
                );

                // Update registry
                self.registry.set_widgets(plugin_id, widgets.clone());

                // Update router mappings
                if let Ok(mut router) = self.router.lock() {
                    let widget_ids: Vec<String> = widgets.iter().map(|w| w.id.clone()).collect();
                    router.map_widgets(plugin_id, &widget_ids);
                }

                // Send full update to UI so daemon widgets get rendered
                self.send_full_update();
            }
            PluginMessage::UpdateWidget { id, widget } => {
                log::debug!(
                    "[plugin-manager] UpdateWidget {} from {}",
                    id,
                    plugin_id
                );
                self.registry.update_widget(plugin_id, &id, widget);
                self.send_full_update();
            }
            PluginMessage::RemoveWidget { id } => {
                log::debug!(
                    "[plugin-manager] RemoveWidget {} from {}",
                    id,
                    plugin_id
                );
                self.registry.remove_widget(plugin_id, &id);
                if let Ok(mut router) = self.router.lock() {
                    router.unmap_widget(&id);
                }
                self.send_full_update();
            }
        }
    }

    /// Handle a plugin disconnection
    fn handle_disconnection(&mut self, plugin_id: &str) {
        log::info!("[plugin-manager] Plugin disconnected: {}", plugin_id);

        if let Ok(mut router) = self.router.lock() {
            router.unregister_client(plugin_id);
        }
        self.registry.remove_plugin(plugin_id);

        let _ = self.update_tx.send(PluginUpdate::PluginDisconnected {
            plugin_id: plugin_id.to_string(),
        });

        // Send updated widgets to UI
        self.send_full_update();
    }

    /// Get all current widgets
    pub fn get_all_widgets(&self) -> Vec<NamedWidget> {
        self.registry.get_all_widgets()
    }

    /// Discover plugins and connect to them
    async fn discover_and_connect(&mut self) {
        let plugins = discover_plugins();
        log::info!("[plugin-manager] Discovered {} plugins", plugins.len());

        for plugin_info in plugins {
            self.connect_plugin(plugin_info).await;
        }
    }

    /// Connect to a single plugin
    async fn connect_plugin(&mut self, plugin_info: PluginInfo) {
        let plugin_id = plugin_info.name.clone();
        let socket_path = plugin_info.socket_path.clone();

        log::debug!(
            "[plugin-manager] Connecting to plugin: {} at {:?}",
            plugin_id,
            socket_path
        );

        match PluginClient::connect(
            plugin_id.clone(),
            socket_path,
            self.merged_tx.clone(),
        )
        .await
        {
            Ok(client) => {
                log::info!("[plugin-manager] Connected to plugin: {}", plugin_id);

                // Request initial widgets (response will arrive via merged channel)
                if let Err(e) = client.send_get_widgets() {
                    log::warn!(
                        "[plugin-manager] Failed to request initial widgets from {}: {}",
                        plugin_id,
                        e
                    );
                }

                // Register client in shared router
                if let Ok(mut router) = self.router.lock() {
                    router.register_client(plugin_id.clone(), client);
                }

                // Notify UI
                let _ = self.update_tx.send(PluginUpdate::PluginConnected {
                    plugin_id: plugin_id.clone(),
                });
            }
            Err(e) => {
                log::warn!("[plugin-manager] Failed to connect to {}: {}", plugin_id, e);
                let _ = self.update_tx.send(PluginUpdate::Error {
                    plugin_id: plugin_id.clone(),
                    error: format!("Connection failed: {}", e),
                });
            }
        }
    }

    /// Send full widget update to UI
    fn send_full_update(&self) {
        let widgets = self.registry.get_all_widgets();
        log::debug!(
            "[plugin-manager] Sending full update with {} widgets",
            widgets.len()
        );

        let _ = self.update_tx.send(PluginUpdate::FullUpdate { widgets });
    }

    /// Get the number of connected plugins
    pub fn connected_plugin_count(&self) -> usize {
        self.router.lock().map(|r| r.client_count()).unwrap_or(0)
    }

    /// Get the total number of widgets
    pub fn widget_count(&self) -> usize {
        self.registry.widget_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_creation() {
        let config = PluginManagerConfig::default();
        let (manager, _rx, _tx) = PluginManager::new(config);

        assert_eq!(manager.connected_plugin_count(), 0);
        assert_eq!(manager.widget_count(), 0);
    }

    #[test]
    fn test_config_defaults() {
        let _config = PluginManagerConfig::default();
    }

    #[test]
    fn test_get_all_widgets_empty() {
        let config = PluginManagerConfig::default();
        let (manager, _rx, _tx) = PluginManager::new(config);

        let widgets = manager.get_all_widgets();
        assert!(widgets.is_empty());
    }
}
