//! App launch usage tracking.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Usage record for a single app.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppUsage {
    pub launches: u64,
    pub last_used_secs: u64,
}

/// All usage records.
pub type UsageMap = HashMap<String, AppUsage>;

/// Return the path to the usage file.
pub fn usage_file_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("waft")
        .join("launcher-usage.json")
}

/// Load usage data from `path`. Returns empty map on any error.
pub fn load_usage_from(path: &Path) -> UsageMap {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Load usage data from the default path.
pub fn load_usage() -> UsageMap {
    load_usage_from(&usage_file_path())
}

/// Save usage data to `path`. Creates parent directories if needed.
pub fn save_usage_to(path: &Path, map: &UsageMap) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(map)
        .map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// Record a launch for `urn`. Increments count and updates timestamp.
pub fn record_launch_in(map: &mut UsageMap, urn: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = map.entry(urn.to_string()).or_default();
    entry.launches += 1;
    entry.last_used_secs = now;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_file_returns_empty_map() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let map = load_usage_from(&path);
        assert!(map.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("usage.json");

        let mut map = UsageMap::new();
        map.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 5,
                last_used_secs: 1000,
            },
        );

        save_usage_to(&path, &map).unwrap();
        let loaded = load_usage_from(&path);

        assert_eq!(loaded["xdg-apps/app/firefox"].launches, 5);
        assert_eq!(loaded["xdg-apps/app/firefox"].last_used_secs, 1000);
    }

    #[test]
    fn record_launch_increments_count() {
        let mut map = UsageMap::new();
        record_launch_in(&mut map, "xdg-apps/app/firefox");
        assert_eq!(map["xdg-apps/app/firefox"].launches, 1);
        record_launch_in(&mut map, "xdg-apps/app/firefox");
        assert_eq!(map["xdg-apps/app/firefox"].launches, 2);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("waft").join("nested").join("usage.json");
        let map = UsageMap::new();
        save_usage_to(&path, &map).unwrap();
        assert!(path.exists());
    }
}
