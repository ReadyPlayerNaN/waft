//! Persistence for waft-managed virtual audio devices.
//!
//! Virtual device definitions are stored in `~/.config/waft/config.toml` under
//! the audio plugin section. Additionally, `~/.config/pulse/default.pa` is kept
//! in sync so PulseAudio recreates devices even without waft running.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A persisted virtual audio device configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualDeviceConfig {
    /// Module type: "null-sink" or "null-source".
    pub module_type: String,
    /// Internal pactl sink/source name (waft_ prefixed).
    pub sink_name: String,
    /// Human-readable display label.
    pub label: String,
}

/// Sanitize a user label into a valid pactl sink/source name.
///
/// Lowercase, replace non-alphanumeric with `_`, collapse consecutive underscores,
/// trim trailing underscores, prepend `waft_`.
pub fn sanitize_sink_name(label: &str) -> String {
    let lower = label.to_lowercase();
    let replaced: String = lower
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

    // Collapse consecutive underscores
    let mut collapsed = String::with_capacity(replaced.len());
    let mut prev_underscore = false;
    for c in replaced.chars() {
        if c == '_' {
            if !prev_underscore {
                collapsed.push(c);
            }
            prev_underscore = true;
        } else {
            collapsed.push(c);
            prev_underscore = false;
        }
    }

    // Trim trailing underscores
    let trimmed = collapsed.trim_end_matches('_');

    format!("waft_{trimmed}")
}

/// Ensure a sink name is unique among existing devices.
///
/// If `base` already exists in `existing`, appends `_2`, `_3`, etc.
pub fn ensure_unique_sink_name(base: &str, existing: &[VirtualDeviceConfig]) -> String {
    if !existing.iter().any(|d| d.sink_name == base) {
        return base.to_string();
    }

    let mut counter = 2u32;
    loop {
        let candidate = format!("{base}_{counter}");
        if !existing.iter().any(|d| d.sink_name == candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Read virtual devices from config.toml.
///
/// Returns an empty vec if the config file is missing, the audio plugin section
/// is absent, or the virtual_devices key does not exist.
pub fn read_virtual_devices() -> Vec<VirtualDeviceConfig> {
    let config = waft_config::Config::load();
    let settings = match config.get_plugin_settings("audio") {
        Some(s) => s,
        None => return Vec::new(),
    };

    let devices_value = match settings.get("virtual_devices") {
        Some(v) => v,
        None => return Vec::new(),
    };

    match devices_value.clone().try_into::<Vec<VirtualDeviceConfig>>() {
        Ok(devices) => devices,
        Err(e) => {
            log::warn!("[audio/config] failed to deserialize virtual_devices: {e}");
            Vec::new()
        }
    }
}

/// Read the config file and return (root table, config path).
fn read_config() -> anyhow::Result<(toml::Table, PathBuf)> {
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

/// Get a mutable reference to the audio plugin table within the root TOML,
/// creating it if it doesn't exist.
fn get_audio_table(
    root: &mut toml::Table,
) -> anyhow::Result<&mut toml::Table> {
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    let plugins_array = match plugins {
        toml::Value::Array(arr) => arr,
        _ => anyhow::bail!("plugins is not an array"),
    };

    let idx = plugins_array.iter().position(|p| {
        p.get("id")
            .and_then(|v| v.as_str())
            .map(|id| id == "audio")
            .unwrap_or(false)
    });

    let idx = match idx {
        Some(i) => i,
        None => {
            let mut new_entry = toml::Table::new();
            new_entry.insert("id".to_string(), toml::Value::String("audio".to_string()));
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

/// Save virtual devices to config.toml, preserving other settings.
///
/// Reads the existing config file as raw TOML, updates the audio plugin
/// section with the new virtual_devices array, and writes back atomically.
pub fn save_virtual_devices(
    devices: &[VirtualDeviceConfig],
) -> anyhow::Result<()> {
    let (mut root, path) = read_config()?;
    let audio_table = get_audio_table(&mut root)?;

    if devices.is_empty() {
        audio_table.remove("virtual_devices");
    } else {
        let devices_value = toml::Value::try_from(devices)
            .map_err(|e| anyhow::anyhow!("failed to serialize virtual_devices: {e}"))?;
        audio_table.insert("virtual_devices".to_string(), devices_value);
    }

    write_config(&root, &path)?;

    log::debug!("[audio/config] wrote {} virtual devices to config", devices.len());
    Ok(())
}

/// Path to the PulseAudio user config file.
fn default_pa_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("pulse").join("default.pa"))
}

/// Sync `~/.config/pulse/default.pa` with waft-managed virtual device entries.
///
/// Removes all lines ending with `# waft-managed` and appends new load-module
/// lines for each configured device. If the file does not exist, creates it
/// with `.include /etc/pulse/default.pa` as the first line.
pub fn sync_default_pa(
    devices: &[VirtualDeviceConfig],
) -> anyhow::Result<()> {
    let pa_path = default_pa_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "config dir not found"))?;

    let existing_content = if pa_path.exists() {
        std::fs::read_to_string(&pa_path)?
    } else {
        ".include /etc/pulse/default.pa\n".to_string()
    };

    // Remove all waft-managed lines
    let mut lines: Vec<&str> = existing_content
        .lines()
        .filter(|line| !line.trim_end().ends_with("# waft-managed"))
        .collect();

    // Remove trailing empty lines to keep the file clean
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    let mut output = lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }

    // Append new waft-managed lines
    for device in devices {
        let line = match device.module_type.as_str() {
            "null-sink" => format!(
                "load-module module-null-sink sink_name={} sink_properties=device.description=\"{}\" # waft-managed",
                device.sink_name, device.label
            ),
            "null-source" => format!(
                "load-module module-null-source source_name={} source_properties=device.description=\"{}\" # waft-managed",
                device.sink_name, device.label
            ),
            other => {
                log::warn!("[audio/config] unknown module_type '{other}', skipping default.pa entry");
                continue;
            }
        };
        output.push_str(&line);
        output.push('\n');
    }

    // Write atomically
    if let Some(parent) = pa_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = pa_path.with_extension("tmp");
    std::fs::write(&tmp_path, &output)?;
    std::fs::rename(&tmp_path, &pa_path)?;

    log::debug!("[audio/config] synced {} entries to default.pa", devices.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_simple_label() {
        assert_eq!(sanitize_sink_name("Virtual Mic"), "waft_virtual_mic");
    }

    #[test]
    fn sanitize_strips_special_chars() {
        assert_eq!(
            sanitize_sink_name("My (Virtual) Source!"),
            "waft_my_virtual_source"
        );
    }

    #[test]
    fn sanitize_collapses_underscores() {
        assert_eq!(
            sanitize_sink_name("a   b---c"),
            "waft_a_b_c"
        );
    }

    #[test]
    fn sanitize_trims_trailing_underscores() {
        assert_eq!(sanitize_sink_name("test "), "waft_test");
    }

    #[test]
    fn sanitize_empty_label() {
        assert_eq!(sanitize_sink_name(""), "waft_");
    }

    #[test]
    fn sanitize_preserves_numbers() {
        assert_eq!(sanitize_sink_name("Source 42"), "waft_source_42");
    }

    #[test]
    fn unique_name_no_conflict() {
        let existing = vec![
            VirtualDeviceConfig {
                module_type: "null-sink".to_string(),
                sink_name: "waft_other".to_string(),
                label: "Other".to_string(),
            },
        ];
        assert_eq!(ensure_unique_sink_name("waft_test", &existing), "waft_test");
    }

    #[test]
    fn unique_name_with_conflict() {
        let existing = vec![
            VirtualDeviceConfig {
                module_type: "null-sink".to_string(),
                sink_name: "waft_test".to_string(),
                label: "Test".to_string(),
            },
        ];
        assert_eq!(
            ensure_unique_sink_name("waft_test", &existing),
            "waft_test_2"
        );
    }

    #[test]
    fn unique_name_with_multiple_conflicts() {
        let existing = vec![
            VirtualDeviceConfig {
                module_type: "null-sink".to_string(),
                sink_name: "waft_test".to_string(),
                label: "Test".to_string(),
            },
            VirtualDeviceConfig {
                module_type: "null-sink".to_string(),
                sink_name: "waft_test_2".to_string(),
                label: "Test 2".to_string(),
            },
        ];
        assert_eq!(
            ensure_unique_sink_name("waft_test", &existing),
            "waft_test_3"
        );
    }

    #[test]
    fn unique_name_empty_existing() {
        assert_eq!(ensure_unique_sink_name("waft_mic", &[]), "waft_mic");
    }
}
