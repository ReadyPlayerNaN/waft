use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

#[allow(dead_code)]
pub enum Slot {
    Info,
    Controls,
    Header,
}

pub struct Widget {
    pub slot: Slot,
    pub weight: i32,
    pub el: gtk::Widget,
}

/// Callback type for expand toggle events.
/// The callback receives the new expanded state (true = expanded).
pub type ExpandCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

/// A feature toggle widget with optional expandable menu.
pub struct WidgetFeatureToggle {
    pub weight: i32,
    pub el: gtk::Widget,
    /// Optional menu widget (for expandable toggles).
    pub menu: Option<gtk::Widget>,
    /// Callback when expand state changes. Grid connects to this.
    /// Callback receives new expanded state (true = expanded).
    pub on_expand_toggled: Option<ExpandCallback>,
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

    async fn create_elements(&mut self) -> Result<()> {
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    fn get_widgets(&self) -> Vec<Arc<Widget>> {
        Vec::new()
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        Vec::new()
    }
}
