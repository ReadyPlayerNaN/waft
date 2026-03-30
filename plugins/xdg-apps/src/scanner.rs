//! XDG application directory scanner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::desktop_file::{parse_desktop_entry, DesktopEntry};

/// A discovered application: its desktop stem and parsed entry.
#[derive(Debug, Clone)]
pub struct DiscoveredApp {
    /// Lowercase filename stem: `firefox` from `firefox.desktop`.
    pub stem: String,
    /// Full path to the `.desktop` file.
    pub path: PathBuf,
    pub entry: DesktopEntry,
}

/// Return all XDG application directories to scan, in priority order
/// (user directory first, system directories after).
pub fn xdg_app_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    // User applications directory
    let user_dir = dirs::data_local_dir()
        .map(|d| d.join("applications"))
        .unwrap_or_else(|| PathBuf::from("~/.local/share/applications"));
    result.push(user_dir);

    // System application directories from XDG_DATA_DIRS
    let system_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in system_dirs.split(':') {
        if !dir.is_empty() {
            result.push(PathBuf::from(dir).join("applications"));
        }
    }

    result
}

/// Scan `dirs` and return discovered apps deduplicated by stem.
/// Earlier dirs (higher priority) win on conflicts.
pub fn scan_apps(dirs: &[PathBuf]) -> Vec<DiscoveredApp> {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut apps = Vec::new();

    for dir in dirs {
        if let Ok(entries) = scan_dir(dir) {
            for app in entries {
                if seen.insert(app.stem.clone(), ()).is_none() {
                    apps.push(app);
                }
            }
        }
    }

    apps
}

fn scan_dir(dir: &Path) -> std::io::Result<Vec<DiscoveredApp>> {
    let mut apps = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        if stem.is_empty() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) && let Some(entry) = parse_desktop_entry(&content) {
            apps.push(DiscoveredApp { stem, path, entry });
        }
    }
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let apps = scan_apps(&[dir.path().to_path_buf()]);
        assert!(apps.is_empty());
    }

    #[test]
    fn scan_finds_desktop_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("myapp.desktop"),
            "[Desktop Entry]\nType=Application\nName=MyApp\nIcon=myapp\nExec=myapp\n",
        )
        .unwrap();
        let apps = scan_apps(&[dir.path().to_path_buf()]);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].stem, "myapp");
        assert_eq!(apps[0].entry.name, "MyApp");
    }

    #[test]
    fn scan_deduplicates_by_stem_priority() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        // Same stem in both dirs -- dir1 wins (higher priority)
        std::fs::write(
            dir1.path().join("app.desktop"),
            "[Desktop Entry]\nType=Application\nName=UserApp\nIcon=app\nExec=app\n",
        )
        .unwrap();
        std::fs::write(
            dir2.path().join("app.desktop"),
            "[Desktop Entry]\nType=Application\nName=SystemApp\nIcon=app\nExec=app\n",
        )
        .unwrap();
        let apps = scan_apps(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].entry.name, "UserApp");
    }

    #[test]
    fn scan_skips_nodisplay() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("hidden.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden\nIcon=h\nExec=h\nNoDisplay=true\n",
        )
        .unwrap();
        let apps = scan_apps(&[dir.path().to_path_buf()]);
        assert!(apps.is_empty());
    }
}
