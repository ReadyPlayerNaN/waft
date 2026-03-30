//! TOML configuration serialization for filter groups, profiles, and sound config.

use serde::Serialize;
use waft_protocol::entity::notification_filter::{NotificationGroup, NotificationProfile};

use crate::config::SoundConfig;

/// Wrapper for serializing groups and profiles into the notifications plugin config section.
#[derive(Debug, Serialize)]
struct FilterSection {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    groups: Vec<NotificationGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<NotificationProfile>,
}

/// Wrapper for serializing sound config into the notifications plugin config section.
#[derive(Debug, Serialize)]
struct SoundSection {
    enabled: bool,
    urgency: UrgencySoundsToml,
}

#[derive(Debug, Serialize)]
struct UrgencySoundsToml {
    low: String,
    normal: String,
    critical: String,
}

/// Read the config file and return (root table, config path).
fn read_config() -> anyhow::Result<(toml::Table, std::path::PathBuf)> {
    let path = waft_config::Config::config_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "config path not found")
    })?;

    let root: toml::Table = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).map_err(|e| anyhow::anyhow!("failed to parse existing config: {e}"))?
    } else {
        toml::Table::new()
    };

    Ok((root, path))
}

/// Get a mutable reference to the notifications plugin table within the root TOML,
/// creating it if it doesn't exist.
fn get_notifications_table(
    root: &mut toml::Table,
) -> anyhow::Result<&mut toml::Table> {
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    let toml::Value::Array(plugins_array) = plugins else {
        anyhow::bail!("plugins is not an array");
    };

    // Find the index of the notifications plugin entry, or create one
    let idx = plugins_array.iter().position(|p| {
        p.get("id")
            .and_then(|v| v.as_str())
            .map(|id| id == "plugin::notifications")
            .unwrap_or(false)
    });

    let idx = match idx {
        Some(i) => i,
        None => {
            let mut new_entry = toml::Table::new();
            new_entry.insert(
                "id".to_string(),
                toml::Value::String("plugin::notifications".to_string()),
            );
            plugins_array.push(toml::Value::Table(new_entry));
            plugins_array.len() - 1
        }
    };

    match &mut plugins_array[idx] {
        toml::Value::Table(t) => Ok(t),
        _ => anyhow::bail!("plugin entry is not a table"),
    }
}

/// Serialize and write the root TOML table atomically.
fn write_config(
    root: &toml::Table,
    path: &std::path::Path,
) -> anyhow::Result<()> {
    let toml_str =
        toml::to_string_pretty(root).map_err(|e| anyhow::anyhow!("failed to serialize config: {e}"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &toml_str)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Write filter config to the waft config file, preserving other settings.
///
/// Reads the existing config file as raw TOML, updates the notifications plugin
/// section with the new groups/profiles, and writes back atomically.
pub fn write_filter_config(
    groups: &[NotificationGroup],
    profiles: &[NotificationProfile],
) -> anyhow::Result<()> {
    let (mut root, path) = read_config()?;
    let notif_table = get_notifications_table(&mut root)?;

    // Serialize filter section and merge into the plugin table
    let section = FilterSection {
        groups: groups.to_vec(),
        profiles: profiles.to_vec(),
    };

    let section_value = toml::Value::try_from(&section)
        .map_err(|e| anyhow::anyhow!("failed to serialize filter section: {e}"))?;

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

    write_config(&root, &path)?;

    log::debug!("[notifications/config] wrote filter config to disk");
    Ok(())
}

/// Write sound config to the waft config file, preserving other settings.
pub fn write_sound_config(
    sound_config: &SoundConfig,
) -> anyhow::Result<()> {
    let (mut root, path) = read_config()?;
    let notif_table = get_notifications_table(&mut root)?;

    let section = SoundSection {
        enabled: sound_config.enabled,
        urgency: UrgencySoundsToml {
            low: sound_config.urgency.low.clone(),
            normal: sound_config.urgency.normal.clone(),
            critical: sound_config.urgency.critical.clone(),
        },
    };

    let section_value = toml::Value::try_from(&section)
        .map_err(|e| anyhow::anyhow!("failed to serialize sound section: {e}"))?;

    if let toml::Value::Table(section_table) = section_value {
        notif_table.insert("sounds".to_string(), toml::Value::Table(section_table));
    }

    write_config(&root, &path)?;

    log::debug!("[notifications/config] wrote sound config to disk");
    Ok(())
}
