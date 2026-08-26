//! Configuration loading for plugins.
//!
//! Loads plugin-specific configuration from `~/.config/waft/config.toml`.

use anyhow::Context;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

fn plugin_id_matches(candidate: &str, plugin_id: &str) -> bool {
    candidate == plugin_id || candidate == format!("waft::{plugin_id}")
}

fn legacy_plugin_ids(plugin_id: &str) -> &'static [&'static str] {
    match plugin_id {
        "power" => &["battery", "waft::battery"],
        _ => &[],
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

    load_plugin_config_from_root(&root, plugin_id)
}

fn load_plugin_config_from_root<T>(root: &toml::Table, plugin_id: &str) -> anyhow::Result<T>
where
    T: Default + DeserializeOwned,
{
    if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
        if let Some(table) = plugins.iter().find_map(|plugin| {
            let table = plugin.as_table()?;
            let id = table.get("id").and_then(|v| v.as_str())?;
            plugin_id_matches(id, plugin_id).then_some(table)
        }) {
            log::debug!("Found config for plugin '{plugin_id}'");
            return toml::Value::Table(table.clone())
                .try_into()
                .with_context(|| format!("failed to deserialize config for plugin '{plugin_id}'"));
        }

        if let Some(table) = plugins.iter().find_map(|plugin| {
            let table = plugin.as_table()?;
            let id = table.get("id").and_then(|v| v.as_str())?;
            legacy_plugin_ids(plugin_id).contains(&id).then_some(table)
        }) {
            log::debug!("Found legacy config for plugin '{plugin_id}'");
            return toml::Value::Table(table.clone())
                .try_into()
                .with_context(|| format!("failed to deserialize config for plugin '{plugin_id}'"));
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

    #[test]
    fn power_alias_matches_legacy_battery_id() {
        assert_eq!(legacy_plugin_ids("power"), &["battery", "waft::battery"]);
        assert!(plugin_id_matches("power", "power"));
        assert!(plugin_id_matches("waft::power", "power"));
        assert!(!plugin_id_matches("battery", "power"));
        assert!(!plugin_id_matches("battery", "audio"));
    }

    #[test]
    fn direct_power_config_wins_over_legacy_battery_alias() {
        #[derive(Debug, Default, Deserialize, PartialEq)]
        struct DriverConfig {
            driver: Option<String>,
        }

        let root: toml::Table = toml::from_str(
            r#"
[[plugins]]
id = "battery"
driver = "legacy-upower"

[[plugins]]
id = "power"
driver = "power-profiles-daemon"
"#,
        )
        .expect("parse config");

        let loaded: DriverConfig =
            load_plugin_config_from_root(&root, "power").expect("load config");

        assert_eq!(
            loaded,
            DriverConfig {
                driver: Some("power-profiles-daemon".to_string())
            }
        );
    }
}
