//! Application configuration.
//!
//! Loads configuration from `~/.config/sacrebleui/config.toml`.
//! Each plugin defines and parses its own configuration shape.

use serde::Deserialize;
use std::path::PathBuf;

/// Raw plugin configuration entry from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfigEntry {
    pub id: String,
    #[serde(flatten)]
    pub settings: toml::Table,
}

/// Top-level configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    #[serde(default)]
    pub plugins: Vec<PluginConfigEntry>,
}

impl Config {
    /// Get the configuration file path.
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("sacrebleui").join("config.toml"))
    }

    /// Load configuration from file, or return default if not found.
    pub fn load() -> Self {
        Self::config_path()
            .filter(|p| p.exists())
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .and_then(|c| toml::from_str(&c).ok())
            .unwrap_or_default()
    }

    /// Get settings for a specific plugin by ID.
    pub fn get_plugin_settings(&self, plugin_id: &str) -> Option<&toml::Table> {
        self.plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| &p.settings)
    }

    /// Check if a plugin is enabled (listed in config).
    pub fn is_plugin_enabled(&self, plugin_id: &str) -> bool {
        self.plugins.iter().any(|p| p.id == plugin_id)
    }

    /// Check if any plugins are configured.
    pub fn has_plugins(&self) -> bool {
        !self.plugins.is_empty()
    }
}
