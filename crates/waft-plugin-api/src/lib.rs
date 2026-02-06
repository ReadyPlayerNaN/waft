pub mod loader;
pub mod overview;

pub use overview::*;

use std::borrow::Cow;
use std::fmt;

/// Metadata describing a loaded plugin.
pub struct PluginMetadata {
    pub id: PluginId,
    pub name: &'static str,
    pub version: &'static str,
    pub rustc_version: &'static str,
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

/// Macro for plugins to export their metadata.
///
/// Generates a `#[no_mangle] extern "C"` function named `waft_plugin_metadata`
/// that the loader calls to discover the plugin.
#[macro_export]
macro_rules! export_plugin_metadata {
    ($id:expr, $name:expr, $version:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn waft_plugin_metadata() -> $crate::PluginMetadata {
            $crate::PluginMetadata {
                id: $crate::PluginId::from_static($id),
                name: $name,
                version: $version,
                rustc_version: option_env!("RUSTC_VERSION").unwrap_or("unknown"),
            }
        }
    };
}

/// Macro for plugins to export an OverviewPlugin factory.
///
/// Generates a `#[no_mangle] extern "C"` function named
/// `waft_create_overview_plugin` that the loader calls to instantiate the plugin.
#[macro_export]
macro_rules! export_overview_plugin {
    ($create:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn waft_create_overview_plugin() -> *mut dyn $crate::overview::OverviewPlugin
        {
            let plugin: Box<dyn $crate::overview::OverviewPlugin> = Box::new($create);
            Box::into_raw(plugin)
        }
    };
}
