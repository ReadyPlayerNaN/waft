//! TOML configuration serialization for filter groups and profiles.

use serde::Serialize;
use waft_protocol::entity::notification_filter::{NotificationGroup, NotificationProfile};

/// Wrapper for serializing groups and profiles into the notifications plugin config section.
#[derive(Debug, Serialize)]
struct FilterSection {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    groups: Vec<NotificationGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<NotificationProfile>,
}

/// Write filter config to the waft config file, preserving other settings.
///
/// Reads the existing config file as raw TOML, updates the notifications plugin
/// section with the new groups/profiles, and writes back atomically.
pub fn write_filter_config(
    groups: &[NotificationGroup],
    profiles: &[NotificationProfile],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = waft_config::Config::config_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "config path not found")
    })?;

    // Read existing config as raw TOML table
    let mut root: toml::Table = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content)
            .map_err(|e| format!("failed to parse existing config: {e}"))?
    } else {
        toml::Table::new()
    };

    // Find the [[plugins]] array
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    let plugins_array = match plugins {
        toml::Value::Array(arr) => arr,
        _ => return Err("plugins is not an array".into()),
    };

    // Find the notifications plugin entry
    let notif_entry = plugins_array.iter_mut().find(|p| {
        p.get("id")
            .and_then(|v| v.as_str())
            .map(|id| id == "plugin::notifications")
            .unwrap_or(false)
    });

    let notif_table = if let Some(entry) = notif_entry {
        match entry {
            toml::Value::Table(t) => t,
            _ => return Err("plugin entry is not a table".into()),
        }
    } else {
        // Create new entry
        let mut new_entry = toml::Table::new();
        new_entry.insert(
            "id".to_string(),
            toml::Value::String("plugin::notifications".to_string()),
        );
        plugins_array.push(toml::Value::Table(new_entry));
        match plugins_array.last_mut().unwrap() {
            toml::Value::Table(t) => t,
            _ => unreachable!(),
        }
    };

    // Serialize filter section and merge into the plugin table
    let section = FilterSection {
        groups: groups.to_vec(),
        profiles: profiles.to_vec(),
    };

    let section_value = toml::Value::try_from(&section)
        .map_err(|e| format!("failed to serialize filter section: {e}"))?;

    if let toml::Value::Table(section_table) = section_value {
        for (key, value) in section_table {
            notif_table.insert(key, value);
        }
    }

    // Remove empty groups/profiles keys to keep the file clean
    if groups.is_empty() {
        notif_table.remove("groups");
    }
    if profiles.is_empty() {
        notif_table.remove("profiles");
    }

    // Serialize and write atomically
    let toml_str =
        toml::to_string_pretty(&root).map_err(|e| format!("failed to serialize config: {e}"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &toml_str)?;
    std::fs::rename(&tmp_path, &path)?;

    log::debug!("[notifications/config] wrote filter config to disk");

    Ok(())
}
