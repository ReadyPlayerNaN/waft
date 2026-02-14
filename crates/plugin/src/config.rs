//! Configuration loading for plugins.
//!
//! Loads plugin-specific configuration from `~/.config/waft/config.toml`.

use serde::de::DeserializeOwned;
use std::path::PathBuf;

/// Error type for configuration loading.
#[derive(Debug)]
pub enum ConfigError {
    /// Failed to determine config directory.
    NoConfigDir,
    /// I/O error reading config file.
    Io(std::io::Error),
    /// TOML parsing error.
    Toml(toml::de::Error),
    /// Config value deserialization error.
    Deserialize(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NoConfigDir => write!(f, "no config directory"),
            ConfigError::Io(e) => write!(f, "I/O error: {e}"),
            ConfigError::Toml(e) => write!(f, "TOML parse error: {e}"),
            ConfigError::Deserialize(msg) => write!(f, "deserialization error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Toml(e)
    }
}

/// Load plugin-specific configuration from waft config file.
///
/// Searches for a plugin entry in `~/.config/waft/config.toml` matching
/// the given plugin ID. Returns `T::default()` if:
/// - Config file doesn't exist
/// - No matching plugin entry is found
///
/// # Config file format
///
/// ```toml
/// [[plugins]]
/// id = "clock"
/// on_click = "gnome-calendar"
/// ```
pub fn load_plugin_config<T>(plugin_id: &str) -> Result<T, ConfigError>
where
    T: Default + DeserializeOwned,
{
    let config_path = get_waft_config_path()?;

    if !config_path.exists() {
        log::debug!("Config file not found, using defaults for plugin '{plugin_id}'");
        return Ok(T::default());
    }

    let content = std::fs::read_to_string(&config_path)?;
    let root: toml::Table = toml::from_str(&content)?;

    if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
        for plugin in plugins {
            if let Some(table) = plugin.as_table()
                && let Some(id) = table.get("id").and_then(|v| v.as_str())
                    && (id == plugin_id || id == format!("waft::{plugin_id}")) {
                        log::debug!("Found config for plugin '{plugin_id}'");
                        return toml::Value::Table(table.clone())
                            .try_into()
                            .map_err(|e| ConfigError::Deserialize(format!("{e}")));
                    }
        }
    }

    log::debug!("No config found for plugin '{plugin_id}', using defaults");
    Ok(T::default())
}

/// Get the path to the waft config file.
fn get_waft_config_path() -> Result<PathBuf, ConfigError> {
    let config_dir = dirs::config_dir().ok_or(ConfigError::NoConfigDir)?;
    Ok(config_dir.join("waft/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize, PartialEq)]
    struct TestConfig {
        enabled: Option<bool>,
        timeout: Option<u64>,
    }

    #[test]
    fn config_path_contains_waft() {
        let path = get_waft_config_path().expect("should get config path");
        assert!(path.to_string_lossy().contains("waft/config.toml"));
    }

    #[test]
    fn nonexistent_plugin_returns_default() {
        let config: TestConfig =
            load_plugin_config("nonexistent-test-plugin-12345").expect("should return defaults");
        assert_eq!(config, TestConfig::default());
    }
}
