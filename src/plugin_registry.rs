use super::plugin::{Plugin, Slot, Widget, WidgetFeatureToggle};

use anyhow::Result;
use log::warn;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::menu_state::MenuStore;

/// Plugin registry that manages all loaded plugins
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<Mutex<Box<dyn Plugin>>>>,
    menu_store: Arc<MenuStore>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new(menu_store: Arc<MenuStore>) -> Self {
        Self {
            plugins: HashMap::new(),
            menu_store,
        }
    }

    /// Register a plugin and return a cloneable handle to it.
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) -> Arc<Mutex<Box<dyn Plugin>>> {
        let name = plugin.id().to_string();
        let handle: Arc<Mutex<Box<dyn Plugin>>> = Arc::new(Mutex::new(Box::new(plugin)));
        self.plugins.insert(name, handle.clone());
        handle
    }

    /// Get all widget elements for a given slot, sorted by weight (heavier goes lower).
    ///
    /// This returns the widget `el` values so callers can directly `append()` them into
    /// the target container (`header`, `left_col`, `right_col`).
    #[allow(dead_code)]
    pub fn get_widgets_for_slot(&self, slot: Slot) -> Vec<Arc<Widget>> {
        let mut widgets: Vec<Arc<Widget>> = Vec::new();

        for plugin in self.plugins.values() {
            if let Ok(guard) = plugin.lock() {
                widgets.extend(guard.get_widgets());
            }
        }

        widgets.retain(|w| {
            matches!(
                (&w.slot, &slot),
                (Slot::Info, Slot::Info)
                    | (Slot::Controls, Slot::Controls)
                    | (Slot::Header, Slot::Header)
            )
        });
        widgets.sort_by_key(|w| w.weight);
        widgets
    }

    /// Get all feature toggles from all plugins
    pub fn get_all_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        let mut toggles = Vec::new();
        for plugin in self.plugins.values() {
            if let Ok(guard) = plugin.lock() {
                let t = guard.get_feature_toggles();
                toggles.extend(t);
            }
        }

        toggles.sort_by_key(|w| w.weight);
        toggles
    }

    /// Initialize all plugins
    pub async fn init(&self) -> Result<()> {
        for (name, plugin) in self.plugins.iter() {
            let mut guard = plugin
                .lock()
                .map_err(|_| anyhow::anyhow!("Plugin mutex poisoned: {}", name))?;
            if let Err(e) = guard.init().await {
                eprintln!("Failed to initialize plugin '{}': {}", name, e);
                return Err(e);
            }
        }
        Ok(())
    }

    pub async fn create_elements(&self, app: &gtk::Application) -> Result<()> {
        for (name, plugin) in self.plugins.iter() {
            let mut guard = plugin
                .lock()
                .map_err(|_| anyhow::anyhow!("Plugin mutex poisoned: {}", name))?;
            if let Err(e) = guard.create_elements(app, self.menu_store.clone()).await {
                eprintln!("Failed to initialize plugin '{}': {}", name, e);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Clean up all plugins
    #[allow(dead_code)]
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

    /// Notify all plugins about overlay visibility changes.
    pub fn notify_overlay_visible(&self, visible: bool) {
        for (name, plugin) in &self.plugins {
            match plugin.lock() {
                Ok(guard) => guard.on_overlay_visible(visible),
                Err(e) => {
                    warn!(
                        "[registry] plugin '{name}' mutex poisoned in notify_overlay_visible: {e}"
                    );
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn menu_store(&self) -> Arc<MenuStore> {
        self.menu_store.clone()
    }
}
