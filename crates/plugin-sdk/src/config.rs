//! Configuration loading utilities for plugin daemons.
//!
//! This module provides helpers to load plugin-specific configuration from
//! the waft config file (`~/.config/waft/config.toml`).

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
            ConfigError::NoConfigDir => write!(f, "No config directory"),
            ConfigError::Io(e) => write!(f, "I/O error: {}", e),
            ConfigError::Toml(e) => write!(f, "TOML parse error: {}", e),
            ConfigError::Deserialize(msg) => write!(f, "Deserialization error: {}", msg),
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
/// Searches for a plugin entry in `~/.config/waft/config.toml` matching the
/// given plugin ID and deserializes it to type `T`. Returns `T::default()` if:
/// - Config file doesn't exist
/// - No matching plugin entry is found
///
/// The config file should have this structure:
/// ```toml
/// [[plugins]]
/// id = "plugin-name"
/// option1 = "value1"
/// option2 = 42
/// ```
///
/// # Arguments
///
/// * `plugin_id` - The plugin ID to search for (e.g., "clock-daemon", "darkman-daemon")
///
/// # Type Parameters
///
/// * `T` - Config type implementing `Default` + `DeserializeOwned`
///
/// # Example
///
/// ```rust,no_run
/// use serde::Deserialize;
/// use waft_plugin_sdk::config::load_plugin_config;
///
/// #[derive(Debug, Default, Deserialize)]
/// struct MyPluginConfig {
///     enabled: bool,
///     timeout: u64,
/// }
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config: MyPluginConfig = load_plugin_config("my-plugin")?;
///     println!("Config: {:?}", config);
///     Ok(())
/// }
/// ```
pub fn load_plugin_config<T>(plugin_id: &str) -> Result<T, ConfigError>
where
    T: Default + DeserializeOwned,
{
    let config_path = get_waft_config_path()?;

    // If config file doesn't exist, return defaults
    if !config_path.exists() {
        log::debug!("Config file not found, using defaults for plugin '{}'", plugin_id);
        return Ok(T::default());
    }

    // Read and parse config file
    let content = std::fs::read_to_string(&config_path)?;
    let root: toml::Table = toml::from_str(&content)?;

    // Search for matching plugin entry
    if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
        for plugin in plugins {
            if let Some(table) = plugin.as_table() {
                if let Some(id) = table.get("id").and_then(|v| v.as_str()) {
                    // Match both "plugin-name" and "waft::plugin-name" for compatibility
                    if id == plugin_id || id == format!("waft::{}", plugin_id) {
                        log::debug!("Found config for plugin '{}'", plugin_id);
                        return toml::Value::Table(table.clone())
                            .try_into()
                            .map_err(|e| ConfigError::Deserialize(format!("{}", e)));
                    }
                }
            }
        }
    }

    log::debug!("No config found for plugin '{}', using defaults", plugin_id);
    Ok(T::default())
}

/// Get the path to the waft config file.
///
/// Returns `~/.config/waft/config.toml`
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
    fn test_config_path() {
        let path = get_waft_config_path().expect("Should get config path");
        assert!(path.to_string_lossy().contains("waft/config.toml"));
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        // Load config for a plugin ID that definitely doesn't exist
        let config: TestConfig = load_plugin_config("nonexistent-test-plugin-12345")
            .expect("Should return defaults");
        assert_eq!(config, TestConfig::default());
    }
}
