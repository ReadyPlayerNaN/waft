//! Active profile state persistence.

use std::path::{Path, PathBuf};

/// Get the active profile state file path.
pub fn get_active_profile_path() -> PathBuf {
    let state_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    state_dir.join("waft").join("notification-profile")
}

/// Load active profile ID from state file.
pub fn load_active_profile() -> Option<String> {
    load_active_profile_from_path(&get_active_profile_path())
}

fn load_active_profile_from_path(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Save active profile ID to state file.
pub fn save_active_profile(profile_id: &str) -> std::io::Result<()> {
    let path = get_active_profile_path();
    save_active_profile_to_path(&path, profile_id)
}

fn save_active_profile_to_path(path: &Path, profile_id: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Atomic write via temp file
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, profile_id)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_active_profile_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent-waft-test-profile");
        let profile = load_active_profile_from_path(&path);
        assert_eq!(profile, None);
    }

    #[test]
    fn save_and_load_active_profile() {
        let path = PathBuf::from("/tmp/waft-test-profile-roundtrip");
        save_active_profile_to_path(&path, "work").unwrap();

        let loaded = load_active_profile_from_path(&path);
        assert_eq!(loaded, Some("work".to_string()));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_active_profile_trims_whitespace() {
        let path = PathBuf::from("/tmp/waft-test-profile-whitespace");
        std::fs::write(&path, "  work  \n").unwrap();

        let loaded = load_active_profile_from_path(&path);
        assert_eq!(loaded, Some("work".to_string()));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_active_profile_empty_file_returns_none() {
        let path = PathBuf::from("/tmp/waft-test-profile-empty");
        std::fs::write(&path, "   \n").unwrap();

        let loaded = load_active_profile_from_path(&path);
        assert_eq!(loaded, None);

        let _ = std::fs::remove_file(&path);
    }
}
