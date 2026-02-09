//! Plugin manager that orchestrates IPC-based plugin communication
//!
//! This module provides the main PluginManager that coordinates all IPC components:
//! - Plugin discovery (scanning for .sock files)
//! - Client connections (PluginClient for each daemon)
//! - Widget registry (tracking widget state)
//! - Action routing (sending user actions to plugins)
//! - Diff calculation (minimizing GTK updates)

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use waft_ipc::{Action, NamedWidget};

use super::client::{ClientError, PluginClient};
use super::diff::{diff_widgets, WidgetDiff};
use super::discovery::{discover_plugins, PluginInfo};
use super::registry::WidgetRegistry;
use super::router::{ActionRouter, RouterError};

/// Update event sent from PluginManager to the UI
#[derive(Debug, Clone)]
pub enum PluginUpdate {
    /// Full widget set from all plugins (sent on initial load)
    FullUpdate {
        widgets: Vec<NamedWidget>,
    },

    /// Incremental widget changes (sent on plugin updates)
    IncrementalUpdate {
        diffs: Vec<WidgetDiff>,
    },

    /// A plugin connected
    PluginConnected {
        plugin_id: String,
    },

    /// A plugin disconnected
    PluginDisconnected {
        plugin_id: String,
    },

    /// An error occurred
    Error {
        plugin_id: String,
        error: String,
    },
}

/// Configuration for PluginManager behavior
#[derive(Debug, Clone)]
pub struct PluginManagerConfig {
    /// How often to poll plugins for widget updates (default: 2 seconds)
    pub poll_interval: Duration,

    /// How often to attempt reconnection to disconnected plugins (default: 5 seconds)
    pub reconnect_interval: Duration,

    /// Whether to automatically reconnect to plugins (default: true)
    pub auto_reconnect: bool,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            reconnect_interval: Duration::from_secs(5),
            auto_reconnect: true,
        }
    }
}

/// Main coordinator for IPC-based plugin system
///
/// The PluginManager orchestrates:
/// 1. **Discovery**: Scans for plugin sockets and connects clients
/// 2. **Polling**: Periodically requests widgets from each plugin
/// 3. **Registry**: Maintains current widget state for all plugins
/// 4. **Diffing**: Computes minimal widget changes for efficient GTK updates
/// 5. **Routing**: Routes user actions back to the appropriate plugin
///
/// # Example
///
/// ```no_run
/// use waft_overview::plugin_manager::{PluginManager, PluginManagerConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let (mut manager, mut updates) = PluginManager::new(PluginManagerConfig::default());
///
/// // Spawn background task to manage plugins
/// tokio::spawn(async move {
///     manager.run().await;
/// });
///
/// // Handle updates in UI thread
/// while let Some(update) = updates.recv().await {
///     match update {
///         waft_overview::plugin_manager::PluginUpdate::FullUpdate { widgets } => {
///             println!("Received {} widgets", widgets.len());
///         }
///         _ => {}
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct PluginManager {
    config: PluginManagerConfig,
    registry: WidgetRegistry,
    router: ActionRouter,
    update_tx: mpsc::UnboundedSender<PluginUpdate>,
}

impl PluginManager {
    /// Create a new PluginManager
    ///
    /// Returns a tuple of (PluginManager, update receiver). The receiver should
    /// be used to listen for widget updates and send them to the UI.
    pub fn new(config: PluginManagerConfig) -> (Self, mpsc::UnboundedReceiver<PluginUpdate>) {
        let (update_tx, update_rx) = mpsc::unbounded_channel();

        let manager = Self {
            config,
            registry: WidgetRegistry::new(),
            router: ActionRouter::new(),
            update_tx,
        };

        (manager, update_rx)
    }

    /// Run the plugin manager (blocks until shutdown)
    ///
    /// This method should be called from a background tokio task. It will:
    /// 1. Discover available plugins
    /// 2. Connect to each plugin
    /// 3. Request initial widget state
    /// 4. Poll for updates on an interval
    /// 5. Attempt reconnection to disconnected plugins
    pub async fn run(&mut self) {
        log::info!("[plugin-manager] Starting plugin manager");

        // Initial discovery and connection
        self.discover_and_connect().await;

        // Send initial full widget state to UI
        self.send_full_update();

        // Set up polling intervals
        let mut poll_timer = interval(self.config.poll_interval);
        let mut reconnect_timer = interval(self.config.reconnect_interval);

        loop {
            tokio::select! {
                _ = poll_timer.tick() => {
                    self.poll_all_plugins().await;
                }

                _ = reconnect_timer.tick() => {
                    if self.config.auto_reconnect {
                        self.reconnect_disconnected_plugins().await;
                    }
                }
            }
        }
    }

    /// Trigger an action on a widget
    ///
    /// This method should be called from the UI when a user interacts with a widget.
    /// It routes the action to the appropriate plugin via the ActionRouter.
    pub async fn trigger_action(
        &mut self,
        widget_id: String,
        action: Action,
    ) -> Result<(), RouterError> {
        self.router.route_action(widget_id, action).await
    }

    /// Get all current widgets (for initial UI rendering)
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

        match PluginClient::connect(plugin_id.clone(), socket_path).await {
            Ok(client) => {
                log::info!("[plugin-manager] Connected to plugin: {}", plugin_id);

                // Request initial widgets
                if let Err(e) = self.request_plugin_widgets(&plugin_id, &client).await {
                    log::warn!(
                        "[plugin-manager] Failed to get initial widgets from {}: {}",
                        plugin_id,
                        e
                    );
                } else {
                    // Register client in router
                    self.router.register_client(plugin_id.clone(), client);

                    // Notify UI
                    let _ = self.update_tx.send(PluginUpdate::PluginConnected {
                        plugin_id: plugin_id.clone(),
                    });
                }
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

    /// Request widgets from a plugin and update registry
    async fn request_plugin_widgets(
        &mut self,
        plugin_id: &str,
        client: &PluginClient,
    ) -> Result<(), ClientError> {
        // Clone client to avoid borrow checker issues
        let mut client = PluginClient::connect(
            client.plugin_name().to_string(),
            client.socket_path().clone(),
        )
        .await?;

        let widgets = client.request_widgets().await?;

        log::debug!(
            "[plugin-manager] Received {} widgets from {}",
            widgets.len(),
            plugin_id
        );

        // Update registry
        self.registry.set_widgets(plugin_id, widgets.clone());

        // Update router mappings
        let widget_ids: Vec<String> = widgets.iter().map(|w| w.id.clone()).collect();
        self.router.map_widgets(plugin_id, &widget_ids);

        Ok(())
    }

    /// Poll all connected plugins for widget updates
    async fn poll_all_plugins(&mut self) {
        let plugin_ids: Vec<String> = self.router.plugin_ids().iter().map(|s| s.to_string()).collect();

        for plugin_id in plugin_ids {
            // Try to get client (we need mutable access)
            // Note: In a real implementation, we'd need to handle this more carefully
            // For now, we'll skip the actual polling and just log
            log::debug!("[plugin-manager] Would poll plugin: {}", plugin_id);

            // TODO: Implement actual polling once we can safely access mutable clients
            // This requires restructuring to avoid borrow conflicts with ActionRouter
        }
    }

    /// Attempt to reconnect to disconnected plugins
    async fn reconnect_disconnected_plugins(&mut self) {
        // Discover plugins again
        let plugins = discover_plugins();

        for plugin_info in plugins {
            let plugin_id = &plugin_info.name;

            // Check if we're not already connected
            if !self.router.has_client(plugin_id) {
                log::debug!("[plugin-manager] Attempting to reconnect to {}", plugin_id);
                self.connect_plugin(plugin_info).await;
            }
        }
    }

    /// Send full widget update to UI
    fn send_full_update(&self) {
        let widgets = self.registry.get_all_widgets();
        log::debug!("[plugin-manager] Sending full update with {} widgets", widgets.len());

        let _ = self.update_tx.send(PluginUpdate::FullUpdate { widgets });
    }

    /// Send incremental widget update to UI
    #[allow(dead_code)]
    fn send_incremental_update(&self, old_widgets: Vec<NamedWidget>, new_widgets: Vec<NamedWidget>) {
        let diffs = diff_widgets(&old_widgets, &new_widgets);

        if !diffs.is_empty() {
            log::debug!("[plugin-manager] Sending incremental update with {} diffs", diffs.len());
            let _ = self.update_tx.send(PluginUpdate::IncrementalUpdate { diffs });
        }
    }

    /// Get the number of connected plugins
    pub fn connected_plugin_count(&self) -> usize {
        self.router.client_count()
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
        let (manager, _rx) = PluginManager::new(config);

        assert_eq!(manager.connected_plugin_count(), 0);
        assert_eq!(manager.widget_count(), 0);
    }

    #[test]
    fn test_config_defaults() {
        let config = PluginManagerConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(2));
        assert_eq!(config.reconnect_interval, Duration::from_secs(5));
        assert!(config.auto_reconnect);
    }

    #[test]
    fn test_get_all_widgets_empty() {
        let config = PluginManagerConfig::default();
        let (manager, _rx) = PluginManager::new(config);

        let widgets = manager.get_all_widgets();
        assert!(widgets.is_empty());
    }
}
