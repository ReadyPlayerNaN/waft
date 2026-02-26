//! Settings-app-specific preferences.
//!
//! Stored separately from the main waft config in `~/.config/waft/settings-app.toml`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Preferences for the waft-settings application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[derive(Default)]
pub struct SettingsPrefs {
    /// Whether to derive window appearance colours from the GTK accent colour.
    #[serde(default)]
    pub derive_window_colors_from_gtk: bool,
}


fn prefs_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("waft")
        .join("settings-app.toml")
}

impl SettingsPrefs {
    /// Load preferences from disk.
    ///
    /// Returns defaults if the file is missing or invalid.
    pub fn load() -> Self {
        let path = prefs_path();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("[prefs] Failed to read settings-app.toml: {e}");
                }
                return Self::default();
            }
        };

        match toml::from_str(&content) {
            Ok(prefs) => prefs,
            Err(e) => {
                log::warn!("[prefs] Failed to parse settings-app.toml: {e}");
                Self::default()
            }
        }
    }

    /// Save preferences to disk.
    ///
    /// Creates the parent directory if needed.
    pub fn save(&self) -> Result<(), String> {
        let path = prefs_path();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
        }

        let content =
            toml::to_string_pretty(self).map_err(|e| format!("Failed to serialize prefs: {e}"))?;

        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write settings-app.toml: {e}"))
    }
}
