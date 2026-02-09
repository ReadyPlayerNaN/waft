//! Application configuration.
//!
//! Loads configuration from `~/.config/waft/config.toml`.
//! Each plugin defines and parses its own configuration shape.

use serde::Deserialize;
use std::path::PathBuf;

/// System-wide daemon configuration.
///
/// Controls how plugins interact with daemon processes.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SystemConfig {
    /// Daemon mode: "disabled" | "opt-in" | "opt-out" | "required"
    ///
    /// - "disabled": No plugins use daemon mode
    /// - "opt-in": Plugins must explicitly enable daemon mode (default)
    /// - "opt-out": Plugins use daemon mode unless explicitly disabled
    /// - "required": All plugins must use daemon mode
    pub daemon_mode: String,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            daemon_mode: "opt-in".to_string(),
        }
    }
}

/// Raw plugin configuration entry from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfigEntry {
    pub id: String,
    /// Override system daemon_mode for this plugin.
    ///
    /// - `None`: Use system default
    /// - `Some(true)`: Force daemon mode
    /// - `Some(false)`: Force in-process mode
    #[serde(default)]
    pub use_daemon: Option<bool>,
    #[serde(flatten)]
    pub settings: toml::Table,
}

/// Top-level configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    #[serde(default)]
    pub system: SystemConfig,
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
            system: SystemConfig::default(),
            plugins: vec![
                PluginConfigEntry {
                    id: "clock".to_string(),
                    use_daemon: None,
                    settings: settings1,
                },
                PluginConfigEntry {
                    id: "weather".to_string(),
                    use_daemon: None,
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

    #[test]
    fn test_default_system_config_uses_opt_in() {
        let config = SystemConfig::default();
        assert_eq!(config.daemon_mode, "opt-in");
    }

    #[test]
    fn test_default_config_has_default_system_config() {
        let config = Config::default();
        assert_eq!(config.system.daemon_mode, "opt-in");
    }

    #[test]
    fn test_plugin_use_daemon_defaults_to_none() {
        let config = create_test_config();
        let clock = config.plugins.iter().find(|p| p.id == "clock").unwrap();
        assert_eq!(clock.use_daemon, None);
    }

    #[test]
    fn test_parse_config_with_daemon_settings() {
        let toml = r#"
[system]
daemon_mode = "opt-out"

[[plugins]]
id = "clock"
use_daemon = true

[[plugins]]
id = "weather"
use_daemon = false

[[plugins]]
id = "battery"
        "#;

        let config: Config = toml::from_str(toml).expect("Failed to parse config");
        assert_eq!(config.system.daemon_mode, "opt-out");

        let clock = config.plugins.iter().find(|p| p.id == "clock").unwrap();
        assert_eq!(clock.use_daemon, Some(true));

        let weather = config.plugins.iter().find(|p| p.id == "weather").unwrap();
        assert_eq!(weather.use_daemon, Some(false));

        let battery = config.plugins.iter().find(|p| p.id == "battery").unwrap();
        assert_eq!(battery.use_daemon, None);
    }
}
