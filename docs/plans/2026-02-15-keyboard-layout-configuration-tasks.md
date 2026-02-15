# Keyboard Layout Configuration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement keyboard layout configuration in waft-settings UI with add/remove/reorder capabilities, integrated with Niri's config file.

**Architecture:** Entity-based architecture with `keyboard-layout-config` entity type. Niri plugin parses/modifies `~/.config/niri/config.kdl` using kdl-rs. Settings UI provides CRUD operations. Handles four config modes: LayoutList (editable), ExternalFile (read-only), SystemDefault (bootstrappable), Malformed (error state).

**Tech Stack:** Rust, waft-protocol entities, kdl (v6.0), GTK4/libadwaita, XKB layout database parsing.

---

## Task 1: Add Protocol Entity Type

**Files:**
- Modify: `crates/protocol/src/entity/keyboard.rs`
- Test: Unit tests in same file

**Step 1: Write test for KeyboardLayoutConfig entity**

Add to `crates/protocol/src/entity/keyboard.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Existing tests...

    #[test]
    fn keyboard_layout_config_serde_roundtrip() {
        let config = KeyboardLayoutConfig {
            mode: "editable".to_string(),
            layouts: vec!["us".to_string(), "de".to_string(), "cz".to_string()],
            variant: Some("dvorak".to_string()),
            options: Some("grp:win_space_toggle".to_string()),
            file_path: None,
            error_message: None,
        };
        let json = serde_json::to_value(&config).unwrap();
        let decoded: KeyboardLayoutConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn keyboard_layout_config_external_file_mode() {
        let config = KeyboardLayoutConfig {
            mode: "external-file".to_string(),
            layouts: vec![],
            variant: None,
            options: None,
            file_path: Some("~/.config/keymap.xkb".to_string()),
            error_message: None,
        };
        let json = serde_json::to_value(&config).unwrap();
        let decoded: KeyboardLayoutConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn keyboard_layout_config_error_mode() {
        let config = KeyboardLayoutConfig {
            mode: "error".to_string(),
            layouts: vec![],
            variant: None,
            options: None,
            file_path: None,
            error_message: Some("Config file has syntax errors".to_string()),
        };
        let json = serde_json::to_value(&config).unwrap();
        let decoded: KeyboardLayoutConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p waft-protocol keyboard_layout_config`
Expected: FAIL with "KeyboardLayoutConfig not found"

**Step 3: Implement KeyboardLayoutConfig entity**

Add to `crates/protocol/src/entity/keyboard.rs` after `KeyboardLayout`:

```rust
/// Entity type identifier for keyboard layout configuration.
pub const CONFIG_ENTITY_TYPE: &str = "keyboard-layout-config";

/// Keyboard layout configuration entity.
/// Represents the configured layouts in compositor's config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayoutConfig {
    /// Configuration mode: "editable", "external-file", "system-default", "error"
    pub mode: String,
    /// Configured layout codes (e.g., ["us", "de", "cz"])
    pub layouts: Vec<String>,
    /// XKB variant (e.g., "dvorak")
    pub variant: Option<String>,
    /// XKB options (e.g., "grp:win_space_toggle,compose:ralt")
    pub options: Option<String>,
    /// Path to external keymap file (set if mode == "external-file")
    pub file_path: Option<String>,
    /// Error message (set if mode == "error")
    pub error_message: Option<String>,
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p waft-protocol keyboard_layout_config`
Expected: PASS (3 tests)

**Step 5: Commit**

```bash
git add crates/protocol/src/entity/keyboard.rs
git commit -m "feat(protocol): add KeyboardLayoutConfig entity type"
```

---

## Task 2: Add kdl Dependency to Niri Plugin

**Files:**
- Modify: `plugins/niri/Cargo.toml`

**Step 1: Add kdl dependency**

Add to `plugins/niri/Cargo.toml`:

```toml
[dependencies]
kdl = "6.0"
```

**Step 2: Verify dependency resolves**

Run: `cargo check -p waft-plugin-niri`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add plugins/niri/Cargo.toml Cargo.lock
git commit -m "build(niri): add kdl dependency for config parsing"
```

---

## Task 3: Implement Config Parsing Module

**Files:**
- Create: `plugins/niri/src/config.rs`
- Modify: `plugins/niri/src/lib.rs`

**Step 1: Write test for parsing layout list config**

Create `plugins/niri/src/config.rs`:

```rust
//! Niri config file parsing and modification.

use anyhow::{Context, Result};
use kdl::KdlDocument;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum KeyboardConfigMode {
    LayoutList,
    ExternalFile,
    SystemDefault,
    Malformed,
}

#[derive(Debug, Clone)]
pub struct KeyboardConfig {
    pub mode: KeyboardConfigMode,
    pub layouts: Vec<String>,
    pub variant: Option<String>,
    pub options: Option<String>,
    pub file_path: Option<String>,
    pub error_message: Option<String>,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            mode: KeyboardConfigMode::SystemDefault,
            layouts: vec![],
            variant: None,
            options: None,
            file_path: None,
            error_message: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_with_layout_list() {
        let kdl = r#"
            input {
                keyboard {
                    xkb {
                        layout "us,de,cz"
                        options "grp:win_space_toggle"
                    }
                }
            }
        "#;

        let config = parse_keyboard_config_from_string(kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::LayoutList);
        assert_eq!(config.layouts, vec!["us", "de", "cz"]);
        assert_eq!(config.options, Some("grp:win_space_toggle".to_string()));
    }

    #[test]
    fn parse_config_with_single_layout() {
        let kdl = r#"
            input {
                keyboard {
                    xkb {
                        layout "us"
                    }
                }
            }
        "#;

        let config = parse_keyboard_config_from_string(kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::LayoutList);
        assert_eq!(config.layouts, vec!["us"]);
    }

    #[test]
    fn parse_config_with_external_file() {
        let kdl = r#"
            input {
                keyboard {
                    xkb {
                        file "~/.config/keymap.xkb"
                    }
                }
            }
        "#;

        let config = parse_keyboard_config_from_string(kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::ExternalFile);
        assert_eq!(config.file_path, Some("~/.config/keymap.xkb".to_string()));
        assert!(config.layouts.is_empty());
    }

    #[test]
    fn parse_config_missing_keyboard_section() {
        let kdl = r#"
            input {
                touchpad {
                    tap
                }
            }
        "#;

        let config = parse_keyboard_config_from_string(kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::SystemDefault);
        assert!(config.layouts.is_empty());
    }

    #[test]
    fn parse_config_empty_xkb_section() {
        let kdl = r#"
            input {
                keyboard {
                    xkb {
                    }
                }
            }
        "#;

        let config = parse_keyboard_config_from_string(kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::SystemDefault);
    }

    #[test]
    fn parse_malformed_kdl() {
        let kdl = r#"
            input {
                keyboard {
                    xkb { layout "us,de"
                }
            // Missing closing braces
        "#;

        let result = parse_keyboard_config_from_string(kdl);
        assert!(result.is_err());
    }
}

fn parse_keyboard_config_from_string(kdl: &str) -> Result<KeyboardConfig> {
    todo!("Implement in next step")
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p waft-plugin-niri config::tests`
Expected: FAIL with "not yet implemented"

**Step 3: Implement config parsing**

Add to `plugins/niri/src/config.rs`:

```rust
fn parse_keyboard_config_from_string(kdl_str: &str) -> Result<KeyboardConfig> {
    let doc: KdlDocument = kdl_str.parse().context("Failed to parse KDL")?;
    extract_keyboard_config(&doc)
}

fn extract_keyboard_config(doc: &KdlDocument) -> Result<KeyboardConfig> {
    // Navigate to input.keyboard.xkb node
    let input_node = match doc.get("input") {
        Some(node) => node,
        None => return Ok(KeyboardConfig::default()),
    };

    let keyboard_node = match input_node.children().and_then(|c| c.get("keyboard")) {
        Some(node) => node,
        None => return Ok(KeyboardConfig::default()),
    };

    let xkb_node = match keyboard_node.children().and_then(|c| c.get("xkb")) {
        Some(node) => node,
        None => return Ok(KeyboardConfig::default()),
    };

    // Check for "file" option first (ExternalFile mode)
    if let Some(file_node) = xkb_node.children().and_then(|c| c.get("file")) {
        if let Some(file_path) = file_node.entries().first().and_then(|e| e.value().as_string()) {
            return Ok(KeyboardConfig {
                mode: KeyboardConfigMode::ExternalFile,
                file_path: Some(file_path.to_string()),
                ..Default::default()
            });
        }
    }

    // Check for "layout" option (LayoutList mode)
    if let Some(layout_node) = xkb_node.children().and_then(|c| c.get("layout")) {
        if let Some(layout_str) = layout_node.entries().first().and_then(|e| e.value().as_string()) {
            let layouts: Vec<String> = layout_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();

            let options = xkb_node
                .children()
                .and_then(|c| c.get("options"))
                .and_then(|n| n.entries().first())
                .and_then(|e| e.value().as_string())
                .map(|s| s.to_string());

            let variant = xkb_node
                .children()
                .and_then(|c| c.get("variant"))
                .and_then(|n| n.entries().first())
                .and_then(|e| e.value().as_string())
                .map(|s| s.to_string());

            return Ok(KeyboardConfig {
                mode: KeyboardConfigMode::LayoutList,
                layouts,
                variant,
                options,
                ..Default::default()
            });
        }
    }

    // Empty xkb section = SystemDefault
    Ok(KeyboardConfig::default())
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p waft-plugin-niri config::tests`
Expected: PASS (6 tests)

**Step 5: Export config module**

Add to `plugins/niri/src/lib.rs`:

```rust
pub mod config;
```

**Step 6: Verify workspace builds**

Run: `cargo build -p waft-plugin-niri`
Expected: SUCCESS

**Step 7: Commit**

```bash
git add plugins/niri/src/config.rs plugins/niri/src/lib.rs
git commit -m "feat(niri): add config parsing with mode detection"
```

---

## Task 4: Implement Config File Reading

**Files:**
- Modify: `plugins/niri/src/config.rs`

**Step 1: Write test for reading config file**

Add to `plugins/niri/src/config.rs` tests:

```rust
#[test]
fn get_niri_config_path() {
    let path = niri_config_path();
    assert!(path.to_str().unwrap().contains(".config/niri/config.kdl"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p waft-plugin-niri get_niri_config_path`
Expected: FAIL with "niri_config_path not found"

**Step 3: Implement config path helper**

Add to `plugins/niri/src/config.rs`:

```rust
/// Get the path to niri config file.
pub fn niri_config_path() -> PathBuf {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".config")
        });
    config_dir.join("niri").join("config.kdl")
}

/// Parse keyboard config from niri config file.
pub fn parse_niri_keyboard_config() -> Result<KeyboardConfig> {
    let config_path = niri_config_path();

    if !config_path.exists() {
        return Ok(KeyboardConfig::default());
    }

    let contents = std::fs::read_to_string(&config_path)
        .context("Failed to read niri config file")?;

    parse_keyboard_config_from_string(&contents)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p waft-plugin-niri get_niri_config_path`
Expected: PASS

**Step 5: Commit**

```bash
git add plugins/niri/src/config.rs
git commit -m "feat(niri): add config file path resolution and reading"
```

---

## Task 5: Implement Config Modification

**Files:**
- Modify: `plugins/niri/src/config.rs`

**Step 1: Write tests for config modification**

Add to `plugins/niri/src/config.rs` tests:

```rust
#[test]
fn modify_config_add_layout() {
    let kdl = r#"
        input {
            keyboard {
                xkb {
                    layout "us,de"
                }
            }
        }
    "#;

    let doc: KdlDocument = kdl.parse().unwrap();
    let modified = modify_keyboard_layouts(doc, vec!["us".into(), "de".into(), "fr".into()]).unwrap();
    let config = extract_keyboard_config(&modified).unwrap();

    assert_eq!(config.layouts, vec!["us", "de", "fr"]);
}

#[test]
fn modify_config_remove_layout() {
    let kdl = r#"
        input {
            keyboard {
                xkb {
                    layout "us,de,cz"
                }
            }
        }
    "#;

    let doc: KdlDocument = kdl.parse().unwrap();
    let modified = modify_keyboard_layouts(doc, vec!["us".into(), "cz".into()]).unwrap();
    let config = extract_keyboard_config(&modified).unwrap();

    assert_eq!(config.layouts, vec!["us", "cz"]);
}

#[test]
fn modify_config_reorder_layouts() {
    let kdl = r#"
        input {
            keyboard {
                xkb {
                    layout "us,de,cz"
                }
            }
        }
    "#;

    let doc: KdlDocument = kdl.parse().unwrap();
    let modified = modify_keyboard_layouts(doc, vec!["cz".into(), "us".into(), "de".into()]).unwrap();
    let config = extract_keyboard_config(&modified).unwrap();

    assert_eq!(config.layouts, vec!["cz", "us", "de"]);
}

#[test]
fn modify_config_preserves_other_settings() {
    let kdl = r#"
        input {
            keyboard {
                xkb {
                    layout "us,de"
                    variant "dvorak"
                    options "grp:win_space_toggle"
                }
            }
            touchpad {
                tap
            }
        }
        output "DP-1" {
            mode "1920x1080@60"
        }
    "#;

    let doc: KdlDocument = kdl.parse().unwrap();
    let modified = modify_keyboard_layouts(doc, vec!["fr".into(), "de".into()]).unwrap();
    let config = extract_keyboard_config(&modified).unwrap();

    assert_eq!(config.layouts, vec!["fr", "de"]);
    assert_eq!(config.variant, Some("dvorak".to_string()));
    assert_eq!(config.options, Some("grp:win_space_toggle".to_string()));

    // Verify other sections preserved
    let modified_str = modified.to_string();
    assert!(modified_str.contains("touchpad"));
    assert!(modified_str.contains("output"));
}

#[test]
fn modify_config_bootstrap_from_system_default() {
    let kdl = r#"
        input {
            keyboard {
                xkb {
                }
            }
        }
    "#;

    let doc: KdlDocument = kdl.parse().unwrap();
    let modified = modify_keyboard_layouts(doc, vec!["us".into(), "de".into()]).unwrap();
    let config = extract_keyboard_config(&modified).unwrap();

    assert_eq!(config.mode, KeyboardConfigMode::LayoutList);
    assert_eq!(config.layouts, vec!["us", "de"]);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p waft-plugin-niri modify_config`
Expected: FAIL with "modify_keyboard_layouts not found"

**Step 3: Implement config modification**

Add to `plugins/niri/src/config.rs`:

```rust
use kdl::{KdlEntry, KdlNode, KdlValue};

/// Modify the keyboard layouts in a KDL document.
pub fn modify_keyboard_layouts(mut doc: KdlDocument, layouts: Vec<String>) -> Result<KdlDocument> {
    // Ensure input node exists
    let input_node = ensure_node(&mut doc, "input");
    let input_children = input_node.ensure_children();

    // Ensure keyboard node exists
    let keyboard_node = ensure_node(input_children, "keyboard");
    let keyboard_children = keyboard_node.ensure_children();

    // Ensure xkb node exists
    let xkb_node = ensure_node(keyboard_children, "xkb");
    let xkb_children = xkb_node.ensure_children();

    // Update or create layout node
    let layout_str = layouts.join(",");
    let layout_entry = KdlEntry::new(KdlValue::String(layout_str));

    if let Some(existing_layout) = xkb_children.get_mut("layout") {
        existing_layout.clear_entries();
        existing_layout.push(layout_entry);
    } else {
        let mut layout_node = KdlNode::new("layout");
        layout_node.push(layout_entry);
        xkb_children.nodes_mut().push(layout_node);
    }

    Ok(doc)
}

/// Helper to ensure a node exists, creating it if necessary.
fn ensure_node<'a>(parent: &'a mut impl NodeContainer, name: &str) -> &'a mut KdlNode {
    if parent.get(name).is_none() {
        parent.nodes_mut().push(KdlNode::new(name));
    }
    parent.get_mut(name).expect("Node was just created")
}

/// Trait for types that can contain KDL nodes.
trait NodeContainer {
    fn get(&self, name: &str) -> Option<&KdlNode>;
    fn get_mut(&mut self, name: &str) -> Option<&mut KdlNode>;
    fn nodes_mut(&mut self) -> &mut Vec<KdlNode>;
}

impl NodeContainer for KdlDocument {
    fn get(&self, name: &str) -> Option<&KdlNode> {
        self.get(name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut KdlNode> {
        self.get_mut(name)
    }

    fn nodes_mut(&mut self) -> &mut Vec<KdlNode> {
        self.nodes_mut()
    }
}

impl NodeContainer for kdl::KdlDocument {
    fn get(&self, name: &str) -> Option<&KdlNode> {
        self.nodes().iter().find(|n| n.name().value() == name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut KdlNode> {
        self.nodes_mut().iter_mut().find(|n| n.name().value() == name)
    }

    fn nodes_mut(&mut self) -> &mut Vec<KdlNode> {
        self.nodes_mut()
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p waft-plugin-niri modify_config`
Expected: PASS (5 tests)

**Step 5: Commit**

```bash
git add plugins/niri/src/config.rs
git commit -m "feat(niri): add config modification with layout updates"
```

---

## Task 6: Implement Config Writing with Backup

**Files:**
- Modify: `plugins/niri/src/config.rs`

**Step 1: Write test for config writing**

Add to `plugins/niri/src/config.rs` tests:

```rust
#[test]
fn write_config_creates_backup() {
    use std::io::Write;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.kdl");
    let backup_path = temp_dir.path().join("config.kdl.backup");

    // Write initial config
    let mut file = std::fs::File::create(&config_path).unwrap();
    write!(file, "input {{ }}").unwrap();
    drop(file);

    // Modify and write
    let doc: KdlDocument = "input { keyboard { xkb { layout \"fr\" } } }".parse().unwrap();
    write_niri_config_with_backup(&config_path, &doc).unwrap();

    // Verify backup exists
    assert!(backup_path.exists());
    let backup_content = std::fs::read_to_string(&backup_path).unwrap();
    assert_eq!(backup_content, "input { }");

    // Verify new content written
    let new_content = std::fs::read_to_string(&config_path).unwrap();
    assert!(new_content.contains("layout \"fr\""));
}
```

**Step 2: Add tempfile dev dependency**

Add to `plugins/niri/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

**Step 3: Run test to verify it fails**

Run: `cargo test -p waft-plugin-niri write_config_creates_backup`
Expected: FAIL with "write_niri_config_with_backup not found"

**Step 4: Implement config writing with backup**

Add to `plugins/niri/src/config.rs`:

```rust
/// Write KDL document to niri config file with backup.
pub fn write_niri_config_with_backup(config_path: &Path, doc: &KdlDocument) -> Result<()> {
    let backup_path = config_path.with_extension("kdl.backup");

    // Create backup if original exists
    if config_path.exists() {
        std::fs::copy(config_path, &backup_path)
            .context("Failed to create config backup")?;
    }

    // Write new config
    match std::fs::write(config_path, doc.to_string()) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Attempt to restore from backup
            if backup_path.exists() {
                if let Err(restore_err) = std::fs::copy(&backup_path, config_path) {
                    log::error!("[niri] Failed to restore backup after write failure: {}", restore_err);
                }
            }
            Err(e).context("Failed to write config file")
        }
    }
}

/// Write keyboard layouts to niri config file.
pub fn write_keyboard_layouts(layouts: Vec<String>) -> Result<()> {
    let config_path = niri_config_path();

    let doc = if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path)?;
        contents.parse::<KdlDocument>()?
    } else {
        KdlDocument::new()
    };

    let modified = modify_keyboard_layouts(doc, layouts)?;
    write_niri_config_with_backup(&config_path, &modified)
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p waft-plugin-niri write_config`
Expected: PASS

**Step 6: Commit**

```bash
git add plugins/niri/Cargo.toml plugins/niri/src/config.rs
git commit -m "feat(niri): add config writing with backup"
```

---

## Task 7: Update Niri State with Config

**Files:**
- Modify: `plugins/niri/src/state.rs`

**Step 1: Add KeyboardConfig to NiriState**

Modify `plugins/niri/src/state.rs`:

```rust
use crate::config::KeyboardConfig;

pub struct NiriState {
    pub keyboard: KeyboardLayoutState,
    pub keyboard_config: KeyboardConfig,  // NEW
    pub outputs: HashMap<String, OutputState>,
}

impl Default for NiriState {
    fn default() -> Self {
        Self {
            keyboard: KeyboardLayoutState::default(),
            keyboard_config: KeyboardConfig::default(),  // NEW
            outputs: HashMap::new(),
        }
    }
}
```

**Step 2: Verify builds**

Run: `cargo check -p waft-plugin-niri`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add plugins/niri/src/state.rs
git commit -m "feat(niri): add keyboard config to plugin state"
```

---

## Task 8: Add Config Entity Conversion

**Files:**
- Modify: `plugins/niri/src/keyboard.rs`

**Step 1: Write test for config entity conversion**

Add to `plugins/niri/src/keyboard.rs` tests:

```rust
use waft_protocol::entity::keyboard::KeyboardLayoutConfig as ProtoConfig;

#[test]
fn config_to_entity_editable_mode() {
    let config = crate::config::KeyboardConfig {
        mode: crate::config::KeyboardConfigMode::LayoutList,
        layouts: vec!["us".into(), "de".into()],
        variant: Some("dvorak".into()),
        options: Some("grp:win_space_toggle".into()),
        file_path: None,
        error_message: None,
    };

    let entity = to_config_entity(&config);
    assert_eq!(entity.mode, "editable");
    assert_eq!(entity.layouts, vec!["us", "de"]);
    assert_eq!(entity.variant, Some("dvorak".to_string()));
    assert_eq!(entity.options, Some("grp:win_space_toggle".to_string()));
}

#[test]
fn config_to_entity_external_file_mode() {
    let config = crate::config::KeyboardConfig {
        mode: crate::config::KeyboardConfigMode::ExternalFile,
        layouts: vec![],
        variant: None,
        options: None,
        file_path: Some("~/.config/keymap.xkb".into()),
        error_message: None,
    };

    let entity = to_config_entity(&config);
    assert_eq!(entity.mode, "external-file");
    assert_eq!(entity.file_path, Some("~/.config/keymap.xkb".to_string()));
}

#[test]
fn config_to_entity_error_mode() {
    let config = crate::config::KeyboardConfig {
        mode: crate::config::KeyboardConfigMode::Malformed,
        layouts: vec![],
        variant: None,
        options: None,
        file_path: None,
        error_message: Some("Parse error".into()),
    };

    let entity = to_config_entity(&config);
    assert_eq!(entity.mode, "error");
    assert_eq!(entity.error_message, Some("Parse error".to_string()));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p waft-plugin-niri config_to_entity`
Expected: FAIL with "to_config_entity not found"

**Step 3: Implement config entity conversion**

Add to `plugins/niri/src/keyboard.rs`:

```rust
use waft_protocol::entity::keyboard::KeyboardLayoutConfig as ProtoConfig;
use crate::config::{KeyboardConfig, KeyboardConfigMode};

/// Convert keyboard config state to a protocol entity.
pub fn to_config_entity(config: &KeyboardConfig) -> ProtoConfig {
    let mode_str = match config.mode {
        KeyboardConfigMode::LayoutList => "editable",
        KeyboardConfigMode::ExternalFile => "external-file",
        KeyboardConfigMode::SystemDefault => "system-default",
        KeyboardConfigMode::Malformed => "error",
    };

    ProtoConfig {
        mode: mode_str.to_string(),
        layouts: config.layouts.clone(),
        variant: config.variant.clone(),
        options: config.options.clone(),
        file_path: config.file_path.clone(),
        error_message: config.error_message.clone(),
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p waft-plugin-niri config_to_entity`
Expected: PASS (3 tests)

**Step 5: Commit**

```bash
git add plugins/niri/src/keyboard.rs
git commit -m "feat(niri): add config entity conversion"
```

---

## Task 9: Load Config on Plugin Startup

**Files:**
- Modify: `plugins/niri/bin/waft-niri-daemon.rs`

**Step 1: Load keyboard config on startup**

Modify the `main()` function in `plugins/niri/bin/waft-niri-daemon.rs`:

After loading keyboard layouts (around line 128), add:

```rust
        // Load keyboard config
        match waft_plugin_niri::config::parse_niri_keyboard_config() {
            Ok(kb_config) => {
                let mut s = match state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[niri] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                s.keyboard_config = kb_config.clone();
                info!(
                    "[niri] Loaded keyboard config: mode={:?}, {} layouts",
                    kb_config.mode,
                    kb_config.layouts.len()
                );
            }
            Err(e) => {
                warn!("[niri] Failed to parse keyboard config: {e}");
                let mut s = match state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[niri] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                s.keyboard_config = waft_plugin_niri::config::KeyboardConfig {
                    mode: waft_plugin_niri::config::KeyboardConfigMode::Malformed,
                    error_message: Some(e.to_string()),
                    ..Default::default()
                };
            }
        }
```

**Step 2: Emit config entity in get_entities**

Modify the `Plugin::get_entities()` implementation (around line 44):

```rust
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        let mut entities = Vec::new();

        // Keyboard layout entity (existing)
        if !state.keyboard.names.is_empty() {
            let layout = keyboard::to_entity(&state.keyboard);
            let urn = Urn::new("niri", KEYBOARD_ENTITY_TYPE, "default");
            entities.push(Entity::new(urn, KEYBOARD_ENTITY_TYPE, &layout));
        }

        // NEW: Keyboard config entity
        let config_entity = keyboard::to_config_entity(&state.keyboard_config);
        let config_urn = Urn::new("niri", entity::keyboard::CONFIG_ENTITY_TYPE, "default");
        entities.push(Entity::new(
            config_urn,
            entity::keyboard::CONFIG_ENTITY_TYPE,
            &config_entity,
        ));

        // Display output entities (existing)
        for (name, output_state) in &state.outputs {
            let output = display::to_entity(output_state);
            let urn = Urn::new("niri", DisplayOutput::ENTITY_TYPE, name);
            entities.push(Entity::new(urn, DisplayOutput::ENTITY_TYPE, &output));
        }

        entities
    }
```

**Step 3: Update provides manifest**

Modify the `main()` function:

```rust
    if waft_plugin::manifest::handle_provides(&[
        KEYBOARD_ENTITY_TYPE,
        entity::keyboard::CONFIG_ENTITY_TYPE,  // NEW
        DisplayOutput::ENTITY_TYPE
    ]) {
        return Ok(());
    }
```

**Step 4: Verify builds**

Run: `cargo build -p waft-plugin-niri`
Expected: SUCCESS

**Step 5: Commit**

```bash
git add plugins/niri/bin/waft-niri-daemon.rs
git commit -m "feat(niri): load and emit keyboard config entity on startup"
```

---

## Task 10: Handle Config Actions

**Files:**
- Modify: `plugins/niri/bin/waft-niri-daemon.rs`

**Step 1: Add action handling for keyboard-layout-config**

Modify the `Plugin::handle_action()` implementation:

```rust
    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let entity_type = urn.entity_type();

        if entity_type == KEYBOARD_ENTITY_TYPE {
            match action.as_str() {
                "cycle" => {
                    debug!("[niri] Cycling keyboard layout");
                    keyboard::switch_next().await?;
                }
                _ => {
                    debug!("[niri] Unknown keyboard action: {}", action);
                }
            }
        } else if entity_type == entity::keyboard::CONFIG_ENTITY_TYPE {
            // NEW: Handle keyboard config actions
            self.handle_keyboard_config_action(&action, params).await?;
        } else if entity_type == DisplayOutput::ENTITY_TYPE {
            // ... existing display output handling ...
        }

        Ok(())
    }
```

Add new method to `NiriPlugin`:

```rust
impl NiriPlugin {
    async fn handle_keyboard_config_action(
        &self,
        action: &str,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use waft_plugin_niri::config::{KeyboardConfigMode, write_keyboard_layouts};

        // Check if config is in editable mode
        let current_mode = {
            let state = self.lock_state();
            state.keyboard_config.mode.clone()
        };

        if !matches!(current_mode, KeyboardConfigMode::LayoutList | KeyboardConfigMode::SystemDefault) {
            return Err(format!(
                "Cannot modify layouts in {:?} mode. {}",
                current_mode,
                match current_mode {
                    KeyboardConfigMode::ExternalFile =>
                        "Remove the 'file' option from niri config to enable editing.",
                    KeyboardConfigMode::Malformed =>
                        "Fix config file errors first.",
                    _ => "",
                }
            ).into());
        }

        match action {
            "add" => {
                let layout: String = serde_json::from_value(
                    params.get("layout").cloned().ok_or("Missing 'layout' parameter")?
                )?;

                let mut new_layouts = {
                    let state = self.lock_state();
                    state.keyboard_config.layouts.clone()
                };

                if !new_layouts.contains(&layout) {
                    new_layouts.push(layout.clone());
                    write_keyboard_layouts(new_layouts)?;
                    info!("[niri] Added keyboard layout: {}", layout);

                    // Reload niri config
                    self.reload_niri_config().await?;
                }
            }
            "remove" => {
                let layout: String = serde_json::from_value(
                    params.get("layout").cloned().ok_or("Missing 'layout' parameter")?
                )?;

                let mut new_layouts = {
                    let state = self.lock_state();
                    state.keyboard_config.layouts.clone()
                };

                new_layouts.retain(|l| l != &layout);
                write_keyboard_layouts(new_layouts)?;
                info!("[niri] Removed keyboard layout: {}", layout);

                self.reload_niri_config().await?;
            }
            "reorder" => {
                let layouts: Vec<String> = serde_json::from_value(
                    params.get("layouts").cloned().ok_or("Missing 'layouts' parameter")?
                )?;

                write_keyboard_layouts(layouts)?;
                info!("[niri] Reordered keyboard layouts");

                self.reload_niri_config().await?;
            }
            "set-options" => {
                let options: String = serde_json::from_value(
                    params.get("options").cloned().ok_or("Missing 'options' parameter")?
                )?;

                // TODO: Implement options modification
                warn!("[niri] set-options not yet implemented");
            }
            _ => {
                warn!("[niri] Unknown keyboard config action: {}", action);
            }
        }

        Ok(())
    }

    async fn reload_niri_config(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use waft_plugin_niri::commands::niri_action;

        match niri_action(&["reload-config"]).await {
            Ok(()) => {
                info!("[niri] Config reloaded successfully");
                Ok(())
            }
            Err(e) => {
                warn!("[niri] Config reload failed (config saved but not applied): {}", e);
                // Don't fail - config was saved, user can reload manually
                Ok(())
            }
        }
    }
}
```

**Step 2: Verify builds**

Run: `cargo build -p waft-plugin-niri`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add plugins/niri/bin/waft-niri-daemon.rs
git commit -m "feat(niri): implement keyboard config actions (add/remove/reorder)"
```

---

## Task 11: Handle ConfigReloaded Event

**Files:**
- Modify: `plugins/niri/bin/waft-niri-daemon.rs`

**Step 1: Add config re-parsing on ConfigReloaded event**

Modify the event stream handler (around line 230):

```rust
                    NiriEvent::ConfigReloaded => {
                        // Re-parse keyboard config (NEW)
                        match waft_plugin_niri::config::parse_niri_keyboard_config() {
                            Ok(new_config) => {
                                let should_notify = {
                                    let mut s = match event_state.lock() {
                                        Ok(g) => g,
                                        Err(e) => {
                                            warn!("[niri] mutex poisoned, recovering: {e}");
                                            e.into_inner()
                                        }
                                    };

                                    let changed = s.keyboard_config.mode != new_config.mode
                                        || s.keyboard_config.layouts != new_config.layouts;

                                    if changed {
                                        info!(
                                            "[niri] Keyboard config changed externally: mode={:?}, {} layouts",
                                            new_config.mode,
                                            new_config.layouts.len()
                                        );
                                        s.keyboard_config = new_config;
                                    }

                                    changed
                                };

                                if should_notify {
                                    event_notifier.notify();
                                }
                            }
                            Err(e) => {
                                warn!("[niri] Failed to re-parse keyboard config after reload: {}", e);
                                let mut s = match event_state.lock() {
                                    Ok(g) => g,
                                    Err(e) => {
                                        warn!("[niri] mutex poisoned, recovering: {e}");
                                        e.into_inner()
                                    }
                                };
                                s.keyboard_config = waft_plugin_niri::config::KeyboardConfig {
                                    mode: waft_plugin_niri::config::KeyboardConfigMode::Malformed,
                                    error_message: Some(e.to_string()),
                                    ..Default::default()
                                };
                                event_notifier.notify();
                            }
                        }

                        // Re-query outputs when config changes (existing)
                        match display::query_outputs().await {
                            // ... existing code ...
                        }
                    }
```

**Step 2: Verify builds**

Run: `cargo build -p waft-plugin-niri`
Expected: SUCCESS

**Step 3: Test manually**

1. Start plugin: `cargo run --bin waft-niri-daemon`
2. Edit `~/.config/niri/config.kdl` to change layouts
3. Run: `niri msg action reload-config`
4. Check logs for "Keyboard config changed externally"

**Step 4: Commit**

```bash
git add plugins/niri/bin/waft-niri-daemon.rs
git commit -m "feat(niri): re-parse keyboard config on ConfigReloaded event"
```

---

## Task 12: Add Keyboard Settings Page

**Files:**
- Create: `crates/settings/src/pages/keyboard.rs`
- Modify: `crates/settings/src/pages/mod.rs`

**Step 1: Create keyboard page skeleton**

Create `crates/settings/src/pages/keyboard.rs`:

```rust
//! Keyboard settings page -- smart container.
//!
//! Subscribes to EntityStore for `keyboard-layout-config` entity type.
//! On entity changes, reconciles keyboard layout list.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::entity::keyboard::{KeyboardLayoutConfig, CONFIG_ENTITY_TYPE};
use waft_protocol::Urn;

/// Smart container for the Keyboard settings page.
pub struct KeyboardPage {
    pub root: gtk::Box,
}

/// Internal mutable state.
struct KeyboardPageState {
    layout_list: gtk::ListBox,
    add_button: gtk::Button,
    mode_banner: Option<adw::Banner>,
}

impl KeyboardPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Mode banner (hidden by default)
        let mode_banner = adw::Banner::builder()
            .revealed(false)
            .build();
        root.append(&mode_banner);

        // Layout list group
        let list_group = adw::PreferencesGroup::builder()
            .title("Keyboard Layouts")
            .description("Configure available keyboard layouts for switching")
            .build();

        let layout_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(vec!["boxed-list"])
            .build();
        list_group.add(&layout_list);

        root.append(&list_group);

        // Add layout button
        let add_button = gtk::Button::builder()
            .label("Add Layout")
            .halign(gtk::Align::Start)
            .margin_top(12)
            .build();
        root.append(&add_button);

        let state = Rc::new(RefCell::new(KeyboardPageState {
            layout_list,
            add_button: add_button.clone(),
            mode_banner: Some(mode_banner),
        }));

        // Subscribe to keyboard-layout-config changes
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let state_clone = state.clone();

            entity_store.subscribe_type(CONFIG_ENTITY_TYPE, move || {
                let configs: Vec<(Urn, KeyboardLayoutConfig)> =
                    store.get_entities_typed(CONFIG_ENTITY_TYPE);

                log::debug!(
                    "[keyboard-page] Config subscription triggered: {} configs",
                    configs.len()
                );

                if let Some((urn, config)) = configs.first() {
                    Self::reconcile(&state_clone, urn, config, &cb);
                }
            });
        }

        // Initial reconciliation
        {
            let state_clone = state.clone();
            let store_clone = entity_store.clone();
            let cb_clone = action_callback.clone();

            gtk::glib::idle_add_local_once(move || {
                let configs: Vec<(Urn, KeyboardLayoutConfig)> =
                    store_clone.get_entities_typed(CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    log::debug!("[keyboard-page] Initial reconciliation with mode: {}", config.mode);
                    Self::reconcile(&state_clone, urn, config, &cb_clone);
                }
            });
        }

        Self { root }
    }

    fn reconcile(
        state: &Rc<RefCell<KeyboardPageState>>,
        urn: &Urn,
        config: &KeyboardLayoutConfig,
        action_callback: &EntityActionCallback,
    ) {
        let mut state_mut = state.borrow_mut();

        // Update mode banner
        if let Some(banner) = &state_mut.mode_banner {
            match config.mode.as_str() {
                "external-file" => {
                    banner.set_title("Using Custom XKB File");
                    banner.set_button_label(None::<&str>);
                    if let Some(path) = &config.file_path {
                        banner.set_revealed(true);
                        // TODO: Show file path in banner
                    }
                    state_mut.add_button.set_sensitive(false);
                }
                "error" => {
                    banner.set_title("Configuration Error");
                    if let Some(error_msg) = &config.error_message {
                        // TODO: Set error message
                    }
                    banner.set_revealed(true);
                    state_mut.add_button.set_sensitive(false);
                }
                "system-default" => {
                    banner.set_title("Using System Defaults");
                    banner.set_revealed(true);
                    state_mut.add_button.set_sensitive(true);
                }
                "editable" => {
                    banner.set_revealed(false);
                    state_mut.add_button.set_sensitive(true);
                }
                _ => {
                    banner.set_revealed(false);
                    state_mut.add_button.set_sensitive(false);
                }
            }
        }

        // Update layout list
        // TODO: Reconcile layout rows
    }
}
```

**Step 2: Register keyboard page module**

Add to `crates/settings/src/pages/mod.rs`:

```rust
pub mod keyboard;
```

**Step 3: Verify builds**

Run: `cargo build -p waft-settings`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/settings/src/pages/keyboard.rs crates/settings/src/pages/mod.rs
git commit -m "feat(settings): add keyboard page skeleton with entity subscription"
```

---

## Task 13: Add Keyboard to Sidebar

**Files:**
- Modify: `crates/settings/src/sidebar.rs`

**Step 1: Add Keyboard row to sidebar**

Add after the Display row (around line 80):

```rust
        // Keyboard row
        let keyboard_icon = IconWidget::from_name("input-keyboard-symbolic", 16);
        let keyboard_row = adw::ActionRow::builder()
            .title("Keyboard")
            .activatable(true)
            .build();
        keyboard_row.add_prefix(keyboard_icon.widget());
        list_box.append(&keyboard_row);
```

**Step 2: Verify builds**

Run: `cargo build -p waft-settings`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add crates/settings/src/sidebar.rs
git commit -m "feat(settings): add Keyboard to sidebar navigation"
```

---

## Task 14: Wire Keyboard Page to Window

**Files:**
- Modify: `crates/settings/src/window.rs`

**Step 1: Import KeyboardPage**

Add to imports:

```rust
use crate::pages::keyboard::KeyboardPage;
```

**Step 2: Create keyboard page**

Add after creating other pages (around line 60):

```rust
        let keyboard_page = KeyboardPage::new(&entity_store, &action_callback);
        stack.add_titled(&keyboard_page.root, Some("keyboard"), "Keyboard");
```

**Step 3: Handle sidebar selection**

Add case to sidebar output handler (around line 90):

```rust
            "Keyboard" => {
                stack.set_visible_child_name("keyboard");
            }
```

**Step 4: Verify builds**

Run: `cargo build -p waft-settings`
Expected: SUCCESS

**Step 5: Test manually**

1. Run: `cargo run --bin waft-settings`
2. Click "Keyboard" in sidebar
3. Verify page switches

**Step 6: Commit**

```bash
git add crates/settings/src/window.rs
git commit -m "feat(settings): wire keyboard page to window navigation"
```

---

## Task 15: Implement Layout Row Widget

**Files:**
- Create: `crates/settings/src/keyboard/layout_row.rs`
- Create: `crates/settings/src/keyboard/mod.rs`

**Step 1: Create layout row widget**

Create `crates/settings/src/keyboard/mod.rs`:

```rust
pub mod layout_row;
```

Create `crates/settings/src/keyboard/layout_row.rs`:

```rust
//! Layout row widget - displays a single keyboard layout with remove button.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Output events from layout row.
pub enum LayoutRowOutput {
    Remove(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(LayoutRowOutput)>>>>;

/// Props for layout row.
pub struct LayoutRowProps {
    pub code: String,
    pub full_name: String,
}

/// Single layout row widget.
pub struct LayoutRow {
    pub root: adw::ActionRow,
    output_cb: OutputCallback,
    code: String,
}

impl LayoutRow {
    pub fn new(props: LayoutRowProps) -> Self {
        let row = adw::ActionRow::builder()
            .title(&props.full_name)
            .subtitle(&props.code)
            .activatable(false)
            .build();

        // Remove button
        let remove_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(vec!["flat"])
            .build();
        row.add_suffix(&remove_btn);

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let code_clone = props.code.clone();
        let cb_clone = output_cb.clone();

        remove_btn.connect_clicked(move |_| {
            if let Some(ref callback) = *cb_clone.borrow() {
                callback(LayoutRowOutput::Remove(code_clone.clone()));
            }
        });

        Self {
            root: row,
            output_cb,
            code: props.code,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn connect_output<F: Fn(LayoutRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
```

**Step 2: Register keyboard module**

Add to `crates/settings/src/lib.rs` or create if needed:

```rust
pub mod keyboard;
```

**Step 3: Verify builds**

Run: `cargo build -p waft-settings`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/settings/src/keyboard/
git commit -m "feat(settings): add layout row widget"
```

---

## Task 16: Implement Layout List Reconciliation

**Files:**
- Modify: `crates/settings/src/pages/keyboard.rs`

**Step 1: Add layout reconciliation logic**

Update the `reconcile` method:

```rust
use crate::keyboard::layout_row::{LayoutRow, LayoutRowOutput, LayoutRowProps};
use std::collections::HashMap;

struct KeyboardPageState {
    layout_list: gtk::ListBox,
    add_button: gtk::Button,
    mode_banner: Option<adw::Banner>,
    layout_rows: HashMap<String, LayoutRow>,  // NEW
}

impl KeyboardPage {
    fn reconcile(
        state: &Rc<RefCell<KeyboardPageState>>,
        urn: &Urn,
        config: &KeyboardLayoutConfig,
        action_callback: &EntityActionCallback,
    ) {
        let mut state_mut = state.borrow_mut();

        // Update mode banner (existing code...)

        // Update layout list
        let mut seen_codes = std::collections::HashSet::new();

        for layout_code in &config.layouts {
            seen_codes.insert(layout_code.clone());

            if !state_mut.layout_rows.contains_key(layout_code) {
                // Create new row
                let full_name = Self::layout_code_to_name(layout_code);
                let row = LayoutRow::new(LayoutRowProps {
                    code: layout_code.clone(),
                    full_name,
                });

                // Connect remove handler
                let urn_clone = urn.clone();
                let code_clone = layout_code.clone();
                let cb_clone = action_callback.clone();

                row.connect_output(move |output| {
                    if let LayoutRowOutput::Remove(code) = output {
                        log::debug!("[keyboard-page] Removing layout: {}", code);
                        let params = serde_json::json!({ "layout": code });
                        cb_clone(urn_clone.clone(), "remove".to_string(), params);
                    }
                });

                state_mut.layout_list.append(row.widget());
                state_mut.layout_rows.insert(layout_code.clone(), row);
            }
        }

        // Remove rows for layouts that no longer exist
        let mut to_remove = Vec::new();
        for code in state_mut.layout_rows.keys() {
            if !seen_codes.contains(code) {
                to_remove.push(code.clone());
            }
        }

        for code in to_remove {
            if let Some(row) = state_mut.layout_rows.remove(&code) {
                state_mut.layout_list.remove(row.widget());
            }
        }
    }

    fn layout_code_to_name(code: &str) -> String {
        // Simple mapping for common layouts
        match code {
            "us" => "English (US)".to_string(),
            "gb" => "English (UK)".to_string(),
            "de" => "German".to_string(),
            "fr" => "French".to_string(),
            "cz" => "Czech".to_string(),
            "es" => "Spanish".to_string(),
            "it" => "Italian".to_string(),
            "pl" => "Polish".to_string(),
            "ru" => "Russian".to_string(),
            _ => code.to_uppercase(),
        }
    }
}
```

**Step 2: Initialize layout_rows map**

Update state initialization:

```rust
        let state = Rc::new(RefCell::new(KeyboardPageState {
            layout_list,
            add_button: add_button.clone(),
            mode_banner: Some(mode_banner),
            layout_rows: HashMap::new(),
        }));
```

**Step 3: Verify builds**

Run: `cargo build -p waft-settings`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/settings/src/pages/keyboard.rs
git commit -m "feat(settings): implement layout list reconciliation"
```

---

## Task 17: Implement Add Layout Dialog

**Files:**
- Create: `crates/settings/src/keyboard/add_layout_dialog.rs`
- Modify: `crates/settings/src/keyboard/mod.rs`

**Step 1: Create add layout dialog**

Create `crates/settings/src/keyboard/add_layout_dialog.rs`:

```rust
//! Add layout dialog - shows searchable list of available XKB layouts.

use gtk::prelude::*;

/// Show add layout dialog and return selected layout code.
pub fn show_add_layout_dialog(parent: &gtk::Window) -> Option<String> {
    let dialog = adw::AlertDialog::builder()
        .heading("Add Keyboard Layout")
        .build();

    // Create search entry
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search layouts...")
        .build();

    // Create scrolled list
    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .min_content_height(300)
        .build();

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    scrolled.set_child(Some(&list_box));

    // Load available layouts
    let available_layouts = get_available_layouts();
    for (code, name) in &available_layouts {
        let row = adw::ActionRow::builder()
            .title(name)
            .subtitle(code)
            .activatable(true)
            .build();
        list_box.append(&row);
    }

    // Search filtering
    list_box.set_filter_func(Some(Box::new({
        let search_entry = search_entry.clone();
        move |row| {
            let search_text = search_entry.text().to_lowercase();
            if search_text.is_empty() {
                return true;
            }

            if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
                let title = action_row.title().to_lowercase();
                let subtitle = action_row.subtitle().to_lowercase();
                title.contains(&search_text) || subtitle.contains(&search_text)
            } else {
                false
            }
        }
    })));

    search_entry.connect_search_changed({
        let list_box = list_box.clone();
        move |_| {
            list_box.invalidate_filter();
        }
    });

    // Layout for dialog content
    let content_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    content_box.append(&search_entry);
    content_box.append(&scrolled);

    dialog.set_extra_child(Some(&content_box));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("add", "Add");
    dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("add"));

    // Handle response
    let selected_code = std::rc::Rc::new(std::cell::RefCell::new(None));
    let selected_code_clone = selected_code.clone();

    dialog.connect_response(None, move |dialog, response| {
        if response == "add" {
            if let Some(selected_row) = list_box.selected_row() {
                if let Some(action_row) = selected_row.downcast_ref::<adw::ActionRow>() {
                    let code = action_row.subtitle().to_string();
                    *selected_code_clone.borrow_mut() = Some(code);
                }
            }
        }
        dialog.close();
    });

    dialog.present(Some(parent));

    // TODO: Make this async or use a different pattern
    // For now, return None as dialogs are async
    None
}

fn get_available_layouts() -> Vec<(String, String)> {
    // Hardcoded fallback list
    vec![
        ("us".into(), "English (US)".into()),
        ("gb".into(), "English (UK)".into()),
        ("de".into(), "German".into()),
        ("fr".into(), "French".into()),
        ("es".into(), "Spanish".into()),
        ("it".into(), "Italian".into()),
        ("pl".into(), "Polish".into()),
        ("ru".into(), "Russian".into()),
        ("cz".into(), "Czech".into()),
        ("sk".into(), "Slovak".into()),
        ("pt".into(), "Portuguese".into()),
        ("nl".into(), "Dutch".into()),
        ("se".into(), "Swedish".into()),
        ("no".into(), "Norwegian".into()),
        ("dk".into(), "Danish".into()),
        ("fi".into(), "Finnish".into()),
        ("jp".into(), "Japanese".into()),
        ("kr".into(), "Korean".into()),
        ("cn".into(), "Chinese".into()),
    ]
}
```

**Step 2: Export dialog module**

Add to `crates/settings/src/keyboard/mod.rs`:

```rust
pub mod add_layout_dialog;
```

**Step 3: Wire up add button**

Modify `crates/settings/src/pages/keyboard.rs`:

```rust
use crate::keyboard::add_layout_dialog::show_add_layout_dialog;

// In KeyboardPage::new(), connect add button:
{
    let urn_clone = /* ... */;
    let cb_clone = action_callback.clone();
    let root_clone = root.clone();

    add_button.connect_clicked(move |_| {
        if let Some(toplevel) = root_clone.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
            if let Some(layout_code) = show_add_layout_dialog(&toplevel) {
                log::debug!("[keyboard-page] Adding layout: {}", layout_code);
                let params = serde_json::json!({ "layout": layout_code });
                cb_clone(urn_clone.clone(), "add".to_string(), params);
            }
        }
    });
}
```

**Step 4: Verify builds**

Run: `cargo build -p waft-settings`
Expected: SUCCESS (note: dialog won't work yet due to async issues)

**Step 5: Commit**

```bash
git add crates/settings/src/keyboard/
git commit -m "feat(settings): add layout dialog (WIP - needs async handling)"
```

---

## Task 18: Add Integration Test

**Files:**
- Create: `plugins/niri/tests/keyboard_config_integration.rs`

**Step 1: Write integration test**

Create `plugins/niri/tests/keyboard_config_integration.rs`:

```rust
//! Integration tests for keyboard config actions.

use tempfile::TempDir;
use waft_plugin::Plugin;
use waft_protocol::Urn;

// TODO: This requires refactoring NiriPlugin to accept custom config path
// Skipping for now, will implement after making plugin testable

#[ignore]
#[tokio::test]
async fn test_add_layout_full_flow() {
    todo!("Implement after making plugin testable with custom config path");
}
```

**Step 2: Commit**

```bash
git add plugins/niri/tests/
git commit -m "test(niri): add integration test placeholder"
```

---

## Summary

This plan implements keyboard layout configuration for Niri compositor with:

- ✅ Entity-based architecture with `keyboard-layout-config` entity
- ✅ KDL config parsing with mode detection (LayoutList/ExternalFile/SystemDefault/Malformed)
- ✅ Config modification with backup and error recovery
- ✅ Action handling: add, remove, reorder layouts
- ✅ External config change detection via ConfigReloaded event
- ✅ Settings UI page with layout list and add/remove buttons
- ⚠️ Add layout dialog (needs async refactoring)
- ⚠️ Integration tests (needs plugin refactoring for testability)

**Next steps:**
1. Fix add layout dialog async handling
2. Implement drag-and-drop reordering in layout list
3. Add XKB database parsing for full layout list
4. Implement set-options action
5. Add comprehensive integration tests
