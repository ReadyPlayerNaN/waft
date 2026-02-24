//! Niri startup entry data model and KDL config I/O.
//!
//! Reads and writes `spawn-at-startup` nodes from `~/.config/niri/config.kdl`.

pub mod entry_dialog;
pub mod startup_row;

use std::path::Path;

/// A single `spawn-at-startup` entry from niri config.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupEntry {
    pub command: String,
    pub args: Vec<String>,
}

/// Load all `spawn-at-startup` entries from a niri KDL config file.
///
/// Returns an empty vec if the file does not exist.
/// Returns an error string if the file exists but cannot be parsed.
pub fn load_startup_entries(config_path: &Path) -> Result<Vec<StartupEntry>, String> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Failed to read config: {e}")),
    };

    let doc: kdl::KdlDocument = content.parse().map_err(|e| format!("KDL parse error: {e}"))?;

    let mut entries = Vec::new();
    for node in doc.nodes() {
        if node.name().value() == "spawn-at-startup" {
            let args: Vec<String> = node
                .entries()
                .iter()
                .filter(|e| e.name().is_none())
                .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                .collect();

            if let Some(command) = args.first() {
                entries.push(StartupEntry {
                    command: command.clone(),
                    args: args[1..].to_vec(),
                });
            }
        }
    }

    Ok(entries)
}

/// Save startup entries to a niri KDL config file.
///
/// Reads the existing document (preserving all non-startup content),
/// removes all `spawn-at-startup` nodes, appends the new entries,
/// creates a backup before writing.
pub fn save_startup_entries(config_path: &Path, entries: &[StartupEntry]) -> Result<(), String> {
    let mut doc: kdl::KdlDocument = if config_path.exists() {
        let content =
            std::fs::read_to_string(config_path).map_err(|e| format!("Failed to read config: {e}"))?;
        content.parse().map_err(|e| format!("KDL parse error: {e}"))?
    } else {
        kdl::KdlDocument::new()
    };

    // Remove all existing spawn-at-startup nodes
    doc.nodes_mut()
        .retain(|node| node.name().value() != "spawn-at-startup");

    // Append new entries
    for entry in entries {
        let mut node = kdl::KdlNode::new("spawn-at-startup");
        node.push(kdl::KdlEntry::new(entry.command.clone()));
        for arg in &entry.args {
            node.push(kdl::KdlEntry::new(arg.clone()));
        }
        doc.nodes_mut().push(node);
    }

    // Backup existing file
    if config_path.exists() {
        let backup_path = config_path.with_extension("kdl.bak");
        if let Err(e) = std::fs::copy(config_path, &backup_path) {
            log::warn!("[startup] Failed to create backup: {e}");
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(format!("Failed to create config directory: {e}"));
        }
    }

    let output = doc.to_string();
    std::fs::write(config_path, output).map_err(|e| format!("Failed to write config: {e}"))?;

    Ok(())
}

/// Default niri config path.
pub fn niri_config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    std::path::PathBuf::from(home)
        .join(".config")
        .join("niri")
        .join("config.kdl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::File::create(&path).unwrap();
        let entries = load_startup_entries(&path).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn load_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.kdl");
        let entries = load_startup_entries(&path).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn load_and_save_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"some-other-setting "value""#).unwrap();
        writeln!(f, r#"spawn-at-startup "waybar""#).unwrap();
        writeln!(f, r#"spawn-at-startup "sh" "-c" "echo hello""#).unwrap();
        drop(f);

        let entries = load_startup_entries(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "waybar");
        assert!(entries[0].args.is_empty());
        assert_eq!(entries[1].command, "sh");
        assert_eq!(entries[1].args, vec!["-c", "echo hello"]);

        // Save with modified entries
        let new_entries = vec![StartupEntry {
            command: "foot".to_string(),
            args: vec![],
        }];
        save_startup_entries(&path, &new_entries).unwrap();

        // Verify other settings preserved
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("some-other-setting"));
        assert!(!content.contains("waybar"));
        assert!(content.contains("foot"));

        // Verify backup was created
        let backup = path.with_extension("kdl.bak");
        assert!(backup.exists());
    }

    #[test]
    fn save_creates_file_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subdir").join("config.kdl");

        let entries = vec![StartupEntry {
            command: "waybar".to_string(),
            args: vec![],
        }];
        save_startup_entries(&path, &entries).unwrap();

        let loaded = load_startup_entries(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].command, "waybar");
    }
}
