use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::menu_state::MenuStore;

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

/// Stable identifier for a plugin.
///
/// This is intentionally an opaque string newtype to avoid centralizing plugin
/// knowledge into the main app/router.
///
/// Formatting conventions (recommended, not enforced):
/// - lowercase
/// - `kebab-case` segments or `namespace::like::this`
///
/// Equality is exact-string equality; no normalization is applied.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginId(Cow<'static, str>);

impl PluginId {
    /// Create a plugin id from a static string without allocation.
    pub const fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }

    /// Create a plugin id from an owned string.
    pub fn from_string(s: String) -> Self {
        Self(Cow::Owned(s))
    }

    /// Borrow the underlying id string.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Debug for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PluginId").field(&self.as_str()).finish()
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&'static str> for PluginId {
    fn from(value: &'static str) -> Self {
        Self::from_static(value)
    }
}

impl From<String> for PluginId {
    fn from(value: String) -> Self {
        Self::from_string(value)
    }
}

#[async_trait(?Send)]
pub trait Plugin {
    /// Stable plugin id (used for routing).
    fn id(&self) -> PluginId;

    /// Configure the plugin with settings from config file.
    /// Default implementation does nothing.
    fn configure(&mut self, _settings: &toml::Table) -> Result<()> {
        Ok(())
    }

    async fn init(&mut self) -> Result<()> {
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
