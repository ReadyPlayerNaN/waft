use super::plugin::{Plugin, Slot, Widget, WidgetFeatureToggle, WidgetRegistrar};

use anyhow::Result;
use gtk::prelude::*;
use log::{error, warn};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::menu_state::MenuStore;
use crate::ui::failed_widget::FailedWidget;

/// Handle to a plugin stored in the registry.
///
/// Uses `Option` to allow taking the plugin out during async operations,
/// avoiding holding the `RefCell` borrow across await points.
type PluginHandle = Rc<RefCell<Option<Box<dyn Plugin>>>>;

/// Plugin registry that manages all loaded plugins.
///
/// Uses `RefCell` for widget/toggle storage since all access is from the main GTK thread.
pub struct PluginRegistry {
    plugins: HashMap<String, PluginHandle>,
    menu_store: Rc<MenuStore>,
    /// Registered widgets (dynamically updated by plugins)
    widgets: RefCell<Vec<Rc<Widget>>>,
    /// Registered feature toggles (dynamically updated by plugins)
    toggles: RefCell<Vec<Rc<WidgetFeatureToggle>>>,
    /// Subscribers notified when widgets or toggles change
    subscribers: RefCell<Vec<Rc<dyn Fn()>>>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new(menu_store: Rc<MenuStore>) -> Self {
        Self {
            plugins: HashMap::new(),
            menu_store,
            widgets: RefCell::new(Vec::new()),
            toggles: RefCell::new(Vec::new()),
            subscribers: RefCell::new(Vec::new()),
        }
    }

    /// Register a pre-boxed plugin (e.g. from a dynamically loaded .so).
    pub fn register_boxed(&mut self, plugin: Box<dyn Plugin>) -> PluginHandle {
        let name = plugin.id().to_string();
        let handle: PluginHandle = Rc::new(RefCell::new(Some(plugin)));
        self.plugins.insert(name, handle.clone());
        handle
    }

    /// Get all widget elements for a given slot, sorted by weight (heavier goes lower).
    ///
    /// This returns the registered widgets filtered by slot and sorted by weight.
    pub fn get_widgets_for_slot(&self, slot: Slot) -> Vec<Rc<Widget>> {
        let mut widgets: Vec<Rc<Widget>> = self
            .widgets
            .borrow()
            .iter()
            .filter(|w| {
                matches!(
                    (&w.slot, &slot),
                    (Slot::Info, Slot::Info)
                        | (Slot::Controls, Slot::Controls)
                        | (Slot::Header, Slot::Header)
                        | (Slot::Actions, Slot::Actions)
                )
            })
            .cloned()
            .collect();
        widgets.sort_by_key(|w| w.weight);
        widgets
    }

    /// Get all feature toggles, sorted by weight
    pub fn get_all_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        let mut toggles: Vec<Rc<WidgetFeatureToggle>> =
            self.toggles.borrow().iter().cloned().collect();
        toggles.sort_by_key(|w| w.weight);
        toggles
    }

    /// Initialize all plugins with shared resources.
    ///
    /// Plugins that fail to initialize are logged but don't prevent other plugins
    /// from loading. Returns Ok even if some plugins fail.
    pub async fn init(&self, resources: &super::plugin::PluginResources) -> Result<()> {
        let mut failed_plugins = Vec::new();

        for (name, plugin_cell) in self.plugins.iter() {
            // Take the plugin out of the cell to avoid holding borrow across await
            let mut plugin = match plugin_cell.try_borrow_mut() {
                Ok(mut guard) => match guard.take() {
                    Some(p) => p,
                    None => {
                        error!("[registry] Plugin '{}' missing during init", name);
                        failed_plugins.push((name.clone(), "plugin missing".to_string()));
                        continue;
                    }
                },
                Err(e) => {
                    error!("[registry] Plugin '{}' already borrowed during init: {}", name, e);
                    failed_plugins.push((name.clone(), "already borrowed".to_string()));
                    continue;
                }
            };
            // Borrow is now dropped

            if let Err(e) = plugin.init(resources).await {
                error!("[registry] Failed to initialize plugin '{}': {}", name, e);
                failed_plugins.push((name.clone(), e.to_string()));
            }

            // Put the plugin back
            plugin_cell.borrow_mut().replace(plugin);
        }

        if !failed_plugins.is_empty() {
            warn!(
                "[registry] {} plugin(s) failed to initialize: {:?}",
                failed_plugins.len(),
                failed_plugins.iter().map(|(n, _)| n).collect::<Vec<_>>()
            );
        }

        Ok(())
    }

    /// Create UI elements for all plugins.
    ///
    /// Plugins that fail to create elements get a "failed widget" placeholder
    /// registered in the Info slot. Returns Ok even if some plugins fail.
    pub async fn create_elements(
        &self,
        app: &gtk::Application,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let mut failed_plugins = Vec::new();

        for (name, plugin_cell) in self.plugins.iter() {
            // Take the plugin out of the cell to avoid holding borrow across await
            let mut plugin = match plugin_cell.try_borrow_mut() {
                Ok(mut guard) => match guard.take() {
                    Some(p) => p,
                    None => {
                        error!("[registry] Plugin '{}' missing during create_elements", name);
                        failed_plugins.push((name.clone(), "plugin missing".to_string()));
                        continue;
                    }
                },
                Err(e) => {
                    error!(
                        "[registry] Plugin '{}' already borrowed during create_elements: {}",
                        name, e
                    );
                    failed_plugins.push((name.clone(), "already borrowed".to_string()));
                    continue;
                }
            };
            // Borrow is now dropped

            if let Err(e) = plugin
                .create_elements(app, self.menu_store.clone(), registrar.clone())
                .await
            {
                error!("[registry] Failed to create elements for plugin '{}': {}", name, e);
                failed_plugins.push((name.clone(), e.to_string()));
            }

            // Put the plugin back
            plugin_cell.borrow_mut().replace(plugin);
        }

        // Register failed widget indicators for plugins that failed
        for (name, error_msg) in &failed_plugins {
            let failed_widget = FailedWidget::new(name, error_msg);
            registrar.register_widget(Rc::new(Widget {
                id: format!("{}:failed", name),
                slot: Slot::Info,
                weight: 999, // Show at the bottom
                el: failed_widget.widget().clone().upcast::<gtk::Widget>(),
            }));
        }

        if !failed_plugins.is_empty() {
            warn!(
                "[registry] {} plugin(s) failed to create elements: {:?}",
                failed_plugins.len(),
                failed_plugins.iter().map(|(n, _)| n).collect::<Vec<_>>()
            );
        }

        Ok(())
    }

    /// Clean up all plugins
    #[allow(dead_code)]
    pub async fn cleanup_all(&mut self) -> Result<()> {
        for (name, plugin_cell) in self.plugins.iter() {
            // Take the plugin out of the cell to avoid holding borrow across await
            let mut plugin = match plugin_cell.try_borrow_mut() {
                Ok(mut guard) => match guard.take() {
                    Some(p) => p,
                    None => {
                        eprintln!("Failed to cleanup plugin '{}': plugin missing", name);
                        continue;
                    }
                },
                Err(_) => {
                    eprintln!("Failed to cleanup plugin '{}': already borrowed", name);
                    continue;
                }
            };
            // Borrow is now dropped

            if let Err(e) = plugin.cleanup().await {
                eprintln!("Failed to cleanup plugin '{}': {}", name, e);
                // Continue cleaning up other plugins even if one fails
            }

            // Put the plugin back
            plugin_cell.borrow_mut().replace(plugin);
        }

        Ok(())
    }

    /// Notify all plugins about overlay visibility changes.
    pub fn notify_overlay_visible(&self, visible: bool) {
        for (name, plugin_cell) in &self.plugins {
            match plugin_cell.try_borrow() {
                Ok(guard) => {
                    if let Some(plugin) = guard.as_ref() {
                        plugin.on_overlay_visible(visible);
                    }
                }
                Err(e) => {
                    warn!(
                        "[registry] plugin '{name}' already borrowed in notify_overlay_visible: {e}"
                    );
                }
            }
        }
    }

    /// Notify all plugins that the session is locking.
    pub fn notify_session_locked(&self) {
        for (name, plugin_cell) in &self.plugins {
            match plugin_cell.try_borrow() {
                Ok(guard) => {
                    if let Some(plugin) = guard.as_ref() {
                        plugin.on_session_lock();
                    }
                }
                Err(e) => {
                    warn!(
                        "[registry] plugin '{name}' already borrowed in notify_session_locked: {e}"
                    );
                }
            }
        }
    }

    /// Notify all plugins that the session has unlocked.
    pub fn notify_session_unlocked(&self) {
        for (name, plugin_cell) in &self.plugins {
            match plugin_cell.try_borrow() {
                Ok(guard) => {
                    if let Some(plugin) = guard.as_ref() {
                        plugin.on_session_unlock();
                    }
                }
                Err(e) => {
                    warn!(
                        "[registry] plugin '{name}' already borrowed in notify_session_unlocked: {e}"
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

    pub fn menu_store(&self) -> Rc<MenuStore> {
        self.menu_store.clone()
    }

    /// Subscribe to widget/toggle changes.
    ///
    /// The callback is invoked whenever widgets or toggles are registered or unregistered.
    /// Subscribers should call `get_widgets_for_slot()` or `get_all_feature_toggles()`
    /// to get the current state.
    pub fn subscribe_widgets<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        self.subscribers.borrow_mut().push(Rc::new(callback));
    }

    /// Notify all subscribers that widgets/toggles have changed.
    fn notify_subscribers(&self) {
        for callback in self.subscribers.borrow().iter() {
            callback();
        }
    }
}

impl WidgetRegistrar for PluginRegistry {
    fn register_widget(&self, widget: Rc<Widget>) {
        self.widgets.borrow_mut().push(widget);
        self.notify_subscribers();
    }

    fn register_feature_toggle(&self, toggle: Rc<WidgetFeatureToggle>) {
        self.toggles.borrow_mut().push(toggle);
        self.notify_subscribers();
    }

    fn unregister_widget(&self, id: &str) {
        self.widgets.borrow_mut().retain(|w| w.id != id);
        self.notify_subscribers();
    }

    fn unregister_feature_toggle(&self, id: &str) {
        self.toggles.borrow_mut().retain(|t| t.id != id);
        self.notify_subscribers();
    }
}

/// A wrapper that allows Rc<PluginRegistry> to be used as Rc<dyn WidgetRegistrar>.
///
/// This exists because:
/// - Rc<PluginRegistry> is needed for sharing across closures
/// - Rc<dyn WidgetRegistrar> is needed for plugins (main-thread-only)
/// - Plugins keep the registrar for runtime widget updates
pub struct RegistrarHandle {
    registry: Rc<PluginRegistry>,
}

impl RegistrarHandle {
    pub fn new(registry: Rc<PluginRegistry>) -> Self {
        Self { registry }
    }
}

impl WidgetRegistrar for RegistrarHandle {
    fn register_widget(&self, widget: Rc<Widget>) {
        self.registry.register_widget(widget);
    }

    fn register_feature_toggle(&self, toggle: Rc<WidgetFeatureToggle>) {
        self.registry.register_feature_toggle(toggle);
    }

    fn unregister_widget(&self, id: &str) {
        self.registry.unregister_widget(id);
    }

    fn unregister_feature_toggle(&self, id: &str) {
        self.registry.unregister_feature_toggle(id);
    }
}
