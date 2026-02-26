//! Shared KDL config file I/O with validation and backup.
//!
//! Provides [`KdlConfigFile`] for safe read-modify-write cycles on KDL files.
//! All writes go through [`KdlConfigFile::save`], which enforces v1 format,
//! validates the serialized output by re-parsing, creates a `.bak` backup,
//! and only then writes to disk.

use std::path::{Path, PathBuf};

/// A KDL config file with load, modify, validate, backup, and write support.
pub struct KdlConfigFile {
    path: PathBuf,
    doc: kdl::KdlDocument,
}

impl KdlConfigFile {
    /// Load a KDL config file from disk.
    ///
    /// Returns an empty document if the file does not exist.
    /// Returns an error if the file exists but cannot be parsed.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path: path.to_path_buf(),
                    doc: kdl::KdlDocument::new(),
                });
            }
            Err(e) => return Err(format!("Failed to read config: {e}")),
        };

        let doc: kdl::KdlDocument = content
            .parse()
            .map_err(|e| format!("KDL parse error: {e}"))?;

        Ok(Self {
            path: path.to_path_buf(),
            doc,
        })
    }

    /// Get a reference to the underlying KDL document.
    pub fn doc(&self) -> &kdl::KdlDocument {
        &self.doc
    }

    /// Get a mutable reference to the underlying KDL document.
    pub fn doc_mut(&mut self) -> &mut kdl::KdlDocument {
        &mut self.doc
    }

    /// Remove all top-level nodes with the given name.
    pub fn remove_nodes_by_name(&mut self, name: &str) {
        self.doc
            .nodes_mut()
            .retain(|node| node.name().value() != name);
    }

    /// Save the document to disk with validation and backup.
    ///
    /// Steps:
    /// 1. Convert all nodes to KDL v1 format via `ensure_v1()`
    /// 2. Serialize to string
    /// 3. Validate by re-parsing the output -- abort without writing if invalid
    /// 4. Create a `.bak` backup if the original file exists
    /// 5. Ensure the parent directory exists
    /// 6. Write the validated output to disk
    pub fn save(&mut self) -> Result<(), String> {
        self.doc.ensure_v1();

        let output = self.doc.to_string();

        // Validate: re-parse the serialized output to catch serialization issues
        if let Err(e) = output.parse::<kdl::KdlDocument>() {
            return Err(format!(
                "Generated KDL failed validation (file not written): {e}"
            ));
        }

        // Backup existing file
        if self.path.exists() {
            let backup_path = self.path.with_extension("kdl.bak");
            if let Err(e) = std::fs::copy(&self.path, &backup_path) {
                log::warn!("[kdl-config] Failed to create backup: {e}");
            }
        }

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent()
            && let Err(e) = std::fs::create_dir_all(parent) {
                return Err(format!("Failed to create config directory: {e}"));
            }

        std::fs::write(&self.path, output)
            .map_err(|e| format!("Failed to write config: {e}"))?;

        Ok(())
    }
}

/// Default niri config path (`~/.config/niri/config.kdl`).
pub fn niri_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("niri")
        .join("config.kdl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_nonexistent_returns_empty_doc() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.kdl");
        let config = KdlConfigFile::load(&path).unwrap();
        assert!(config.doc.nodes().is_empty());
    }

    #[test]
    fn load_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, "node-a\nnode-b\n").unwrap();

        let config = KdlConfigFile::load(&path).unwrap();
        assert_eq!(config.doc.nodes().len(), 2);
    }

    #[test]
    fn load_invalid_kdl_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, "{{{{ not valid kdl").unwrap();

        let err = KdlConfigFile::load(&path).err().expect("should fail on invalid KDL");
        assert!(err.contains("KDL parse error"), "unexpected error: {err}");
    }

    #[test]
    fn remove_nodes_by_name_filters_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, "keep-me\nremove-me\nkeep-me-too\nremove-me\n").unwrap();

        let mut config = KdlConfigFile::load(&path).unwrap();
        assert_eq!(config.doc.nodes().len(), 4);

        config.remove_nodes_by_name("remove-me");
        assert_eq!(config.doc.nodes().len(), 2);
        assert!(config.doc.nodes().iter().all(|n| n.name().value() != "remove-me"));
    }

    #[test]
    fn save_creates_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, "original\n").unwrap();

        let mut config = KdlConfigFile::load(&path).unwrap();
        config.save().unwrap();

        let backup = path.with_extension("kdl.bak");
        assert!(backup.exists());
        let backup_content = std::fs::read_to_string(&backup).unwrap();
        assert!(backup_content.contains("original"));
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("dir").join("config.kdl");

        let mut config = KdlConfigFile::load(&path).unwrap();
        config.doc_mut().nodes_mut().push(kdl::KdlNode::new("test-node"));
        config.save().unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test-node"));
    }

    #[test]
    fn save_no_backup_for_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new-config.kdl");

        let mut config = KdlConfigFile::load(&path).unwrap();
        config.doc_mut().nodes_mut().push(kdl::KdlNode::new("node"));
        config.save().unwrap();

        let backup = path.with_extension("kdl.bak");
        assert!(!backup.exists());
    }

    #[test]
    fn save_produces_quoted_strings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");

        // KDL v2 allows bare identifiers as values, but v1 requires quoted strings.
        // Build a node with a string argument and verify ensure_v1() produces quotes.
        let mut config = KdlConfigFile::load(&path).unwrap();
        let mut node = kdl::KdlNode::new("spawn-at-startup");
        node.push(kdl::KdlEntry::new("bash".to_string()));
        config.doc_mut().nodes_mut().push(node);
        config.save().unwrap();

        let raw_bytes = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw_bytes.contains('"'),
            "saved file must contain quoted strings for v1 compatibility, got: {raw_bytes}"
        );
        assert!(
            raw_bytes.contains("\"bash\""),
            "expected quoted \"bash\" in output, got: {raw_bytes}"
        );
    }

    #[test]
    fn save_round_trips_through_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");

        // Write a config, save, reload, verify content preserved
        let mut config = KdlConfigFile::load(&path).unwrap();
        let mut node = kdl::KdlNode::new("binds");
        let mut child_doc = kdl::KdlDocument::new();
        child_doc.nodes_mut().push(kdl::KdlNode::new("Mod+Return"));
        node.set_children(child_doc);
        config.doc_mut().nodes_mut().push(node);
        config.save().unwrap();

        // Reload and verify
        let reloaded = KdlConfigFile::load(&path).unwrap();
        assert_eq!(reloaded.doc.nodes().len(), 1);
        assert_eq!(reloaded.doc.nodes()[0].name().value(), "binds");
        assert!(reloaded.doc.nodes()[0].children().is_some());
    }
}
