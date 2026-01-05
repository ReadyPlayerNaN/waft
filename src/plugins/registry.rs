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

/// Plugin registry that manages all loaded plugins
#[derive(Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) {
        let name = plugin.name().to_string();
        self.plugins.insert(name, Box::new(plugin));
    }

    /// Get all feature toggles from all plugins
    pub fn get_all_feature_toggles(&self) -> Vec<FeatureToggle> {
        let mut toggles = Vec::new();

        for plugin in self.plugins.values() {
            toggles.extend(plugin.feature_toggles());
        }

        toggles
    }

    /// Initialize all plugins
    pub async fn initialize_all(&mut self) -> Result<()> {
        for (name, plugin) in self.plugins.iter_mut() {
            println!("Initializing plugin: {}", name);
            if let Err(e) = plugin.initialize().await {
                eprintln!("Failed to initialize plugin '{}': {}", name, e);
                return Err(e);
            }
        }

        Ok(())
    }

    /// Clean up all plugins
    pub async fn cleanup_all(&mut self) -> Result<()> {
        for (name, plugin) in self.plugins.iter_mut() {
            if let Err(e) = plugin.cleanup().await {
                eprintln!("Failed to cleanup plugin '{}': {}", name, e);
                // Continue cleaning up other plugins even if one fails
            }
        }

        Ok(())
    }
}
