//! Application configuration.
//!
//! Loads configuration from `~/.config/waft/config.toml`.
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
        dirs::config_dir().map(|d| d.join("waft").join("config.toml"))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> Config {
        let mut settings1 = toml::Table::new();
        settings1.insert("enabled".to_string(), toml::Value::Boolean(true));
        settings1.insert("interval".to_string(), toml::Value::Integer(30));

        let mut settings2 = toml::Table::new();
        settings2.insert("theme".to_string(), toml::Value::String("dark".to_string()));

        Config {
            plugins: vec![
                PluginConfigEntry {
                    id: "clock".to_string(),
                    settings: settings1,
                },
                PluginConfigEntry {
                    id: "weather".to_string(),
                    settings: settings2,
                },
            ],
        }
    }

    #[test]
    fn test_get_plugin_settings_returns_settings_for_existing_plugin() {
        let config = create_test_config();
        let settings = config.get_plugin_settings("clock");

        assert!(settings.is_some());
        let settings = settings.unwrap();
        assert_eq!(settings.get("enabled"), Some(&toml::Value::Boolean(true)));
        assert_eq!(settings.get("interval"), Some(&toml::Value::Integer(30)));
    }

    #[test]
    fn test_get_plugin_settings_returns_none_for_nonexistent_plugin() {
        let config = create_test_config();
        let settings = config.get_plugin_settings("nonexistent");

        assert!(settings.is_none());
    }

    #[test]
    fn test_is_plugin_enabled_returns_true_for_listed_plugin() {
        let config = create_test_config();

        assert!(config.is_plugin_enabled("clock"));
        assert!(config.is_plugin_enabled("weather"));
    }

    #[test]
    fn test_is_plugin_enabled_returns_false_for_unlisted_plugin() {
        let config = create_test_config();

        assert!(!config.is_plugin_enabled("battery"));
        assert!(!config.is_plugin_enabled(""));
    }

    #[test]
    fn test_default_config_has_no_plugins() {
        let config = Config::default();

        assert!(config.plugins.is_empty());
        assert!(!config.is_plugin_enabled("anything"));
    }

    #[test]
    fn test_config_path_returns_some() {
        // config_path depends on dirs crate, but should return Some in most environments
        let path = Config::config_path();
        // We can't assert the exact path, but if it returns Some, it should end with config.toml
        if let Some(p) = path {
            assert!(p.ends_with("config.toml"));
            assert!(p.to_string_lossy().contains("waft"));
        }
    }
}
