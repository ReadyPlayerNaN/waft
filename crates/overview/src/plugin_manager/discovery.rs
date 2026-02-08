use std::path::PathBuf;

/// Information about a discovered plugin
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub name: String,
    pub socket_path: PathBuf,
}

/// Discovers plugins by scanning the runtime socket directory
///
/// Scans `/run/user/{uid}/waft/plugins/` for `*.sock` files and extracts
/// plugin names from filenames (e.g., `audio.sock` -> `audio`).
///
/// Returns an empty vector if the directory doesn't exist or cannot be read.
pub fn discover_plugins() -> Vec<PluginInfo> {
    let uid = unsafe { libc::getuid() };
    let plugin_dir = PathBuf::from(format!("/run/user/{}/waft/plugins", uid));

    log::debug!("[plugin-discovery] Scanning for plugins in: {}", plugin_dir.display());

    let plugins = discover_plugins_in_dir(&plugin_dir);

    if plugins.is_empty() {
        if !plugin_dir.exists() {
            log::info!(
                "[plugin-discovery] Plugin directory does not exist: {}",
                plugin_dir.display()
            );
        } else {
            log::debug!("[plugin-discovery] No plugin sockets found in {}", plugin_dir.display());
        }
    } else {
        log::info!(
            "[plugin-discovery] Found {} plugin(s): {:?}",
            plugins.len(),
            plugins.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }

    plugins
}

/// Discovers plugins in a specific directory (useful for testing)
fn discover_plugins_in_dir(dir: &PathBuf) -> Vec<PluginInfo> {
    let mut plugins = Vec::new();

    // If directory doesn't exist or can't be read, return empty vec
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::debug!("[plugin-discovery] Failed to read directory {}: {}", dir.display(), e);
            return plugins;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .sock files
        if path.extension().and_then(|s| s.to_str()) != Some("sock") {
            continue;
        }

        // Validate that it's actually a socket file (in production)
        // In tests, we accept regular files too
        match std::fs::metadata(&path) {
            Ok(metadata) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::FileTypeExt;
                    let file_type = metadata.file_type();
                    if !file_type.is_socket() && !file_type.is_file() {
                        log::warn!(
                            "[plugin-discovery] Skipping {}: not a Unix socket or regular file",
                            path.display()
                        );
                        continue;
                    }
                    if file_type.is_file() {
                        log::debug!(
                            "[plugin-discovery] Warning: {} is a regular file, not a socket (this is OK in tests)",
                            path.display()
                        );
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "[plugin-discovery] Failed to read metadata for {}: {}",
                    path.display(),
                    e
                );
                continue;
            }
        }

        // Extract plugin name from filename
        if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
            log::debug!("[plugin-discovery] Found plugin socket: {} ({})", file_stem, path.display());
            plugins.push(PluginInfo {
                name: file_stem.to_string(),
                socket_path: path,
            });
        } else {
            log::warn!("[plugin-discovery] Skipping {}: invalid filename", path.display());
        }
    }

    // Sort by name for consistent ordering
    plugins.sort_by(|a, b| a.name.cmp(&b.name));

    plugins
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_discover_plugins_empty_dir() {
        let temp_dir = std::env::temp_dir().join("waft_test_empty");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let plugins = discover_plugins_in_dir(&temp_dir);
        assert!(plugins.is_empty());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_discover_plugins_nonexistent_dir() {
        let temp_dir = std::env::temp_dir().join("waft_test_nonexistent");
        let _ = fs::remove_dir_all(&temp_dir);

        let plugins = discover_plugins_in_dir(&temp_dir);
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_discover_plugins_with_sockets() {
        let temp_dir = std::env::temp_dir().join("waft_test_sockets");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create some .sock files
        fs::write(temp_dir.join("audio.sock"), b"").unwrap();
        fs::write(temp_dir.join("battery.sock"), b"").unwrap();
        fs::write(temp_dir.join("clock.sock"), b"").unwrap();

        // Create a non-.sock file (should be ignored)
        fs::write(temp_dir.join("README.txt"), b"").unwrap();

        let plugins = discover_plugins_in_dir(&temp_dir);
        assert_eq!(plugins.len(), 3);

        // Check names (should be sorted)
        assert_eq!(plugins[0].name, "audio");
        assert_eq!(plugins[1].name, "battery");
        assert_eq!(plugins[2].name, "clock");

        // Check paths
        assert_eq!(plugins[0].socket_path, temp_dir.join("audio.sock"));
        assert_eq!(plugins[1].socket_path, temp_dir.join("battery.sock"));
        assert_eq!(plugins[2].socket_path, temp_dir.join("clock.sock"));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_discover_plugins_ignores_non_sock_files() {
        let temp_dir = std::env::temp_dir().join("waft_test_ignore");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create various non-.sock files
        fs::write(temp_dir.join("plugin.so"), b"").unwrap();
        fs::write(temp_dir.join("config.json"), b"").unwrap();
        fs::write(temp_dir.join("lock"), b"").unwrap();

        let plugins = discover_plugins_in_dir(&temp_dir);
        assert!(plugins.is_empty());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_discover_plugins_handles_subdirectories() {
        let temp_dir = std::env::temp_dir().join("waft_test_subdirs");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create a subdirectory (should be skipped)
        fs::create_dir_all(temp_dir.join("subdir")).unwrap();

        // Create actual socket files
        fs::write(temp_dir.join("valid.sock"), b"").unwrap();

        let plugins = discover_plugins_in_dir(&temp_dir);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "valid");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_plugin_info_equality() {
        let plugin1 = PluginInfo {
            name: "audio".to_string(),
            socket_path: PathBuf::from("/run/user/1000/waft/plugins/audio.sock"),
        };

        let plugin2 = PluginInfo {
            name: "audio".to_string(),
            socket_path: PathBuf::from("/run/user/1000/waft/plugins/audio.sock"),
        };

        let plugin3 = PluginInfo {
            name: "battery".to_string(),
            socket_path: PathBuf::from("/run/user/1000/waft/plugins/battery.sock"),
        };

        assert_eq!(plugin1, plugin2);
        assert_ne!(plugin1, plugin3);
    }
}
