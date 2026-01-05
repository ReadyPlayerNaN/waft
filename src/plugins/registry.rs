/*!
Plugin system for sacrebleui.

This module defines the plugin interface that allows extending sacrebleui's functionality.
Plugins can provide:
- Widgets for left/right columns
- Feature toggles
- Control sliders

Each plugin is responsible for connecting to outside services and providing data for the view layer.
*/
use super::bindings::FeatureToggle;
use super::plugin::Plugin;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Plugin registry that manages all loaded plugins
#[derive(Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<Mutex<Box<dyn Plugin>>>>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin and return a cloneable handle to it.
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) -> Arc<Mutex<Box<dyn Plugin>>> {
        let name = plugin.name().to_string();
        let handle: Arc<Mutex<Box<dyn Plugin>>> = Arc::new(Mutex::new(Box::new(plugin)));
        self.plugins.insert(name, handle.clone());
        handle
    }

    /// Get all feature toggles from all plugins
    pub fn get_all_feature_toggles(&self) -> Vec<FeatureToggle> {
        let mut toggles = Vec::new();

        for plugin in self.plugins.values() {
            if let Ok(guard) = plugin.lock() {
                toggles.extend(guard.feature_toggles());
            }
        }

        toggles
    }

    /// Initialize all plugins
    pub async fn initialize_all(&mut self) -> Result<()> {
        for (name, plugin) in self.plugins.iter() {
            println!("Initializing plugin: {}", name);
            let mut guard = plugin
                .lock()
                .map_err(|_| anyhow::anyhow!("Plugin mutex poisoned: {}", name))?;
            if let Err(e) = guard.initialize().await {
                eprintln!("Failed to initialize plugin '{}': {}", name, e);
                return Err(e);
            }
        }

        Ok(())
    }

    /// Clean up all plugins
    pub async fn cleanup_all(&mut self) -> Result<()> {
        for (name, plugin) in self.plugins.iter() {
            let mut guard = match plugin.lock() {
                Ok(g) => g,
                Err(_) => {
                    eprintln!("Failed to cleanup plugin '{}': mutex poisoned", name);
                    continue;
                }
            };

            if let Err(e) = guard.cleanup().await {
                eprintln!("Failed to cleanup plugin '{}': {}", name, e);
                // Continue cleaning up other plugins even if one fails
            }
        }

        Ok(())
    }
}
