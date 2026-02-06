use super::plugin::{Plugin, Slot, Widget, WidgetFeatureToggle, WidgetRegistrar};

use anyhow::Result;
use gtk::prelude::*;
use log::{error, warn};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use crate::menu_state::MenuStore;
use crate::ui::failed_widget::FailedWidget;

/// Plugin registry that manages all loaded plugins.
///
/// Uses `RefCell` for widget/toggle storage since all access is from the main GTK thread.
pub struct PluginRegistry {
    plugins: HashMap<String, Rc<Mutex<Box<dyn Plugin>>>>,
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

    /// Register a plugin and return a cloneable handle to it.
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) -> Rc<Mutex<Box<dyn Plugin>>> {
        let name = plugin.id().to_string();
        let handle: Rc<Mutex<Box<dyn Plugin>>> = Rc::new(Mutex::new(Box::new(plugin)));
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

    /// Initialize all plugins.
    ///
    /// Plugins that fail to initialize are logged but don't prevent other plugins
    /// from loading. Returns Ok even if some plugins fail.
    pub async fn init(&self) -> Result<()> {
        let mut failed_plugins = Vec::new();

        for (name, plugin) in self.plugins.iter() {
            let mut guard = match plugin.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("[registry] Plugin '{}' mutex poisoned during init: {}", name, e);
                    failed_plugins.push((name.clone(), "mutex poisoned".to_string()));
                    continue;
                }
            };

            if let Err(e) = guard.init().await {
                error!("[registry] Failed to initialize plugin '{}': {}", name, e);
                failed_plugins.push((name.clone(), e.to_string()));
            }
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

        for (name, plugin) in self.plugins.iter() {
            let mut guard = match plugin.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!(
                        "[registry] Plugin '{}' mutex poisoned during create_elements: {}",
                        name, e
                    );
                    failed_plugins.push((name.clone(), "mutex poisoned".to_string()));
                    continue;
                }
            };

            if let Err(e) = guard
                .create_elements(app, self.menu_store.clone(), registrar.clone())
                .await
            {
                error!("[registry] Failed to create elements for plugin '{}': {}", name, e);
                failed_plugins.push((name.clone(), e.to_string()));
            }
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

    /// Notify all plugins that the session is locking.
    pub fn notify_session_locked(&self) {
        for (name, plugin) in &self.plugins {
            match plugin.lock() {
                Ok(guard) => guard.on_session_lock(),
                Err(e) => {
                    warn!(
                        "[registry] plugin '{name}' mutex poisoned in notify_session_locked: {e}"
                    );
                }
            }
        }
    }

    /// Notify all plugins that the session has unlocked.
    pub fn notify_session_unlocked(&self) {
        for (name, plugin) in &self.plugins {
            match plugin.lock() {
                Ok(guard) => guard.on_session_unlock(),
                Err(e) => {
                    warn!(
                        "[registry] plugin '{name}' mutex poisoned in notify_session_unlocked: {e}"
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
