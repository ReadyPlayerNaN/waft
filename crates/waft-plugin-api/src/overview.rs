use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::PluginId;
use waft_core::dbus::DbusHandle;
use waft_core::menu_state::MenuStore;

/// Trait for registering and unregistering widgets at runtime.
///
/// Plugins receive an `Rc<dyn WidgetRegistrar>` during `create_elements()`
/// and use it to dynamically register/unregister their widgets.
/// Uses `Rc` because all widget operations must happen on the main GTK thread.
#[allow(dead_code)] // unregister methods are part of the API but not yet used
pub trait WidgetRegistrar {
    /// Register a slot widget. Notifies subscribers of the change.
    fn register_widget(&self, widget: Rc<Widget>);

    /// Register a feature toggle. Notifies subscribers of the change.
    fn register_feature_toggle(&self, toggle: Rc<WidgetFeatureToggle>);

    /// Unregister a slot widget by its ID. Notifies subscribers of the change.
    fn unregister_widget(&self, id: &str);

    /// Unregister a feature toggle by its ID. Notifies subscribers of the change.
    fn unregister_feature_toggle(&self, id: &str);
}

#[allow(dead_code)]
pub enum Slot {
    Info,
    Controls,
    Header,
    Actions,
}

pub struct Widget {
    pub id: String,
    pub slot: Slot,
    pub weight: i32,
    pub el: gtk::Widget,
}

/// Resources provided by the host app to plugins during initialization.
///
/// The host creates shared resources (like DBus connections) once and passes them
/// to all plugins. Plugins can use what they need and ignore the rest.
#[derive(Clone)]
pub struct PluginResources {
    /// Session DBus connection (for user services like darkman, etc.)
    pub session_dbus: Option<Arc<DbusHandle>>,
    /// System DBus connection (for system services like BlueZ, NetworkManager, UPower)
    pub system_dbus: Option<Arc<DbusHandle>>,
    /// Tokio runtime handle for spawning async tasks
    ///
    /// Dynamic plugins need this to spawn tasks that require tokio runtime
    /// (like D-Bus signal monitoring). The host app runs with #[tokio::main],
    /// so this handle is always available.
    pub tokio_handle: Option<tokio::runtime::Handle>,
}

/// Callback type for expand toggle events.
/// The callback receives the new expanded state (true = expanded).
pub type ExpandCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

/// A feature toggle widget with optional expandable menu.
pub struct WidgetFeatureToggle {
    pub id: String,
    pub weight: i32,
    pub el: gtk::Widget,
    /// Optional menu widget (for expandable toggles).
    pub menu: Option<gtk::Widget>,
    /// Callback when expand state changes. Grid connects to this.
    /// Callback receives new expanded state (true = expanded).
    pub on_expand_toggled: Option<ExpandCallback>,
    /// Optional menu ID for coordinating with MenuStore.
    pub menu_id: Option<String>,
}

#[async_trait(?Send)]
pub trait OverviewPlugin {
    /// Stable plugin id (used for routing).
    fn id(&self) -> PluginId;

    /// Configure the plugin with settings from config file.
    /// Default implementation does nothing.
    fn configure(&mut self, _settings: &toml::Table) -> Result<()> {
        Ok(())
    }

    /// Initialize the plugin with resources provided by the host.
    ///
    /// Resources like DBus connections are created by the host and passed to plugins
    /// to avoid each plugin creating its own connections. Plugins can use what they need.
    async fn init(&mut self, _resources: &PluginResources) -> Result<()> {
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        _registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    /// Called when the main overlay window visibility changes.
    /// `visible` is `true` when the overlay appears, `false` when it finishes hiding.
    fn on_overlay_visible(&self, _visible: bool) {}

    /// Called when the session is about to lock (screen locker activating).
    /// Plugins should pause animations and hide any visible windows.
    fn on_session_lock(&self) {}

    /// Called when the session unlocks (screen locker deactivated).
    /// Plugins should resume normal operation.
    fn on_session_unlock(&self) {}
}
