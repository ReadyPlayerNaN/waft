//! Configuration loading for plugins.
//!
//! Loads plugin-specific configuration from `~/.config/waft/config.toml`.

use anyhow::Context;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

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
pub fn load_plugin_config<T>(plugin_id: &str) -> anyhow::Result<T>
where
    T: Default + DeserializeOwned,
{
    let config_path = get_waft_config_path()?;

    if !config_path.exists() {
        log::debug!("Config file not found, using defaults for plugin '{plugin_id}'");
        return Ok(T::default());
    }

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read config file {}", config_path.display()))?;
    let root: toml::Table = toml::from_str(&content).context("failed to parse config TOML")?;

    if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
        for plugin in plugins {
            if let Some(table) = plugin.as_table()
                && let Some(id) = table.get("id").and_then(|v| v.as_str())
                && (id == plugin_id || id == format!("waft::{plugin_id}"))
            {
                log::debug!("Found config for plugin '{plugin_id}'");
                return toml::Value::Table(table.clone())
                    .try_into()
                    .with_context(|| {
                        format!("failed to deserialize config for plugin '{plugin_id}'")
                    });
            }
        }
    }

    log::debug!("No config found for plugin '{plugin_id}', using defaults");
    Ok(T::default())
}

/// Get the path to the waft config file.
fn get_waft_config_path() -> anyhow::Result<PathBuf> {
    let config_dir = dirs::config_dir().context("no config directory")?;
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
