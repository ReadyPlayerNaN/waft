//! Niri config file parsing and modification.
//!
//! Reads and modifies the keyboard layout configuration in niri's
//! `~/.config/niri/config.kdl` file. Supports four configuration modes:
//! LayoutList, ExternalFile, SystemDefault, and Malformed.

use anyhow::{Context, Result};
use kdl::KdlDocument;
use std::path::{Path, PathBuf};

/// Detected configuration mode for keyboard layouts.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyboardConfigMode {
    /// `xkb { layout "us,de,cz" }` -- fully editable.
    LayoutList,
    /// `xkb { file "~/.config/keymap.xkb" }` -- read-only.
    ExternalFile,
    /// `xkb { }` or missing -- uses systemd-localed default.
    SystemDefault,
    /// Config exists but parsing failed.
    Malformed,
}

/// Parsed keyboard configuration state from the niri config file.
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

    let contents =
        std::fs::read_to_string(&config_path).context("Failed to read niri config file")?;

    parse_keyboard_config_from_string(&contents)
}

/// Parse keyboard config from a KDL string.
fn parse_keyboard_config_from_string(kdl_str: &str) -> Result<KeyboardConfig> {
    let doc: KdlDocument = kdl_str.parse().context("Failed to parse KDL")?;
    Ok(extract_keyboard_config(&doc))
}

/// Extract keyboard config from a parsed KDL document.
fn extract_keyboard_config(doc: &KdlDocument) -> KeyboardConfig {
    // Navigate to input.keyboard.xkb node
    let input_node = match doc.get("input") {
        Some(node) => node,
        None => return KeyboardConfig::default(),
    };

    let keyboard_node = match input_node.children().and_then(|c| c.get("keyboard")) {
        Some(node) => node,
        None => return KeyboardConfig::default(),
    };

    let xkb_node = match keyboard_node.children().and_then(|c| c.get("xkb")) {
        Some(node) => node,
        None => return KeyboardConfig::default(),
    };

    let xkb_children = match xkb_node.children() {
        Some(c) => c,
        None => return KeyboardConfig::default(),
    };

    // Check for "file" option first (ExternalFile mode)
    if let Some(file_value) = xkb_children.get_arg("file") {
        if let Some(file_path) = file_value.as_string() {
            return KeyboardConfig {
                mode: KeyboardConfigMode::ExternalFile,
                file_path: Some(file_path.to_string()),
                ..Default::default()
            };
        }
    }

    // Check for "layout" option (LayoutList mode)
    if let Some(layout_value) = xkb_children.get_arg("layout") {
        if let Some(layout_str) = layout_value.as_string() {
            let layouts: Vec<String> = layout_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let options = xkb_children
                .get_arg("options")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());

            let variant = xkb_children
                .get_arg("variant")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());

            return KeyboardConfig {
                mode: KeyboardConfigMode::LayoutList,
                layouts,
                variant,
                options,
                ..Default::default()
            };
        }
    }

    // Empty xkb section = SystemDefault
    KeyboardConfig::default()
}

/// Modify the keyboard layouts in a KDL document.
///
/// Updates the `layout` value inside `input.keyboard.xkb`, creating
/// the node hierarchy if it doesn't exist.
pub fn modify_keyboard_layouts(mut doc: KdlDocument, layouts: Vec<String>) -> Result<KdlDocument> {
    let layout_str = layouts.join(",");

    // Ensure input node exists
    if doc.get("input").is_none() {
        let node = kdl::KdlNode::new("input");
        doc.nodes_mut().push(node);
    }
    let input_node = doc.get_mut("input").expect("just created");
    let input_children = input_node.ensure_children();

    // Ensure keyboard node exists
    if input_children.get("keyboard").is_none() {
        let node = kdl::KdlNode::new("keyboard");
        input_children.nodes_mut().push(node);
    }
    let keyboard_node = input_children.get_mut("keyboard").expect("just created");
    let keyboard_children = keyboard_node.ensure_children();

    // Ensure xkb node exists
    if keyboard_children.get("xkb").is_none() {
        let node = kdl::KdlNode::new("xkb");
        keyboard_children.nodes_mut().push(node);
    }
    let xkb_node = keyboard_children.get_mut("xkb").expect("just created");
    let xkb_children = xkb_node.ensure_children();

    // Update or create layout node
    if let Some(existing_layout) = xkb_children.get_mut("layout") {
        existing_layout.entries_mut().clear();
        existing_layout.push(kdl::KdlEntry::new(layout_str));
    } else {
        let mut layout_node = kdl::KdlNode::new("layout");
        layout_node.push(kdl::KdlEntry::new(layout_str));
        xkb_children.nodes_mut().push(layout_node);
    }

    Ok(doc)
}

/// Write KDL document to niri config file with backup.
pub fn write_niri_config_with_backup(config_path: &Path, doc: &KdlDocument) -> Result<()> {
    let backup_path = config_path.with_extension("kdl.backup");

    // Create backup if original exists
    if config_path.exists() {
        std::fs::copy(config_path, &backup_path).context("Failed to create config backup")?;
    }

    // Write new config
    match std::fs::write(config_path, doc.to_string()) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Attempt to restore from backup
            if backup_path.exists() {
                if let Err(restore_err) = std::fs::copy(&backup_path, config_path) {
                    log::error!(
                        "[niri] Failed to restore backup after write failure: {}",
                        restore_err
                    );
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
        let kdl = "input { keyboard { xkb { layout \"us,de\"";

        let result = parse_keyboard_config_from_string(kdl);
        assert!(result.is_err());
    }

    #[test]
    fn get_niri_config_path() {
        let path = niri_config_path();
        assert!(path.to_str().unwrap().contains("niri/config.kdl"));
    }

    #[test]
    fn modify_config_add_layout() {
        let kdl = r#"input {
    keyboard {
        xkb {
            layout "us,de"
        }
    }
}
"#;

        let doc: KdlDocument = kdl.parse().unwrap();
        let modified =
            modify_keyboard_layouts(doc, vec!["us".into(), "de".into(), "fr".into()]).unwrap();
        let config = extract_keyboard_config(&modified);

        assert_eq!(config.layouts, vec!["us", "de", "fr"]);
    }

    #[test]
    fn modify_config_remove_layout() {
        let kdl = r#"input {
    keyboard {
        xkb {
            layout "us,de,cz"
        }
    }
}
"#;

        let doc: KdlDocument = kdl.parse().unwrap();
        let modified = modify_keyboard_layouts(doc, vec!["us".into(), "cz".into()]).unwrap();
        let config = extract_keyboard_config(&modified);

        assert_eq!(config.layouts, vec!["us", "cz"]);
    }

    #[test]
    fn modify_config_reorder_layouts() {
        let kdl = r#"input {
    keyboard {
        xkb {
            layout "us,de,cz"
        }
    }
}
"#;

        let doc: KdlDocument = kdl.parse().unwrap();
        let modified =
            modify_keyboard_layouts(doc, vec!["cz".into(), "us".into(), "de".into()]).unwrap();
        let config = extract_keyboard_config(&modified);

        assert_eq!(config.layouts, vec!["cz", "us", "de"]);
    }

    #[test]
    fn modify_config_preserves_other_settings() {
        let kdl = r#"input {
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
        let modified =
            modify_keyboard_layouts(doc, vec!["fr".into(), "de".into()]).unwrap();
        let config = extract_keyboard_config(&modified);

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
        let kdl = r#"input {
    keyboard {
        xkb {
        }
    }
}
"#;

        let doc: KdlDocument = kdl.parse().unwrap();
        let modified =
            modify_keyboard_layouts(doc, vec!["us".into(), "de".into()]).unwrap();
        let config = extract_keyboard_config(&modified);

        assert_eq!(config.mode, KeyboardConfigMode::LayoutList);
        assert_eq!(config.layouts, vec!["us", "de"]);
    }

    #[test]
    fn write_config_creates_backup() {
        use std::io::Write;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.kdl");
        let backup_path = temp_dir.path().join("config.kdl.backup");

        // Write initial config
        let mut file = std::fs::File::create(&config_path).unwrap();
        write!(file, "input {{ }}").unwrap();
        drop(file);

        // Modify and write
        let doc: KdlDocument = r#"input {
    keyboard {
        xkb {
            layout "fr"
        }
    }
}"#
        .parse()
        .unwrap();
        write_niri_config_with_backup(&config_path, &doc).unwrap();

        // Verify backup exists
        assert!(backup_path.exists());
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, "input { }");

        // Verify new content written
        let new_content = std::fs::read_to_string(&config_path).unwrap();
        assert!(new_content.contains("layout"));
    }

    #[test]
    fn parse_config_with_variant() {
        let kdl = r#"
            input {
                keyboard {
                    xkb {
                        layout "us"
                        variant "dvorak"
                    }
                }
            }
        "#;

        let config = parse_keyboard_config_from_string(kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::LayoutList);
        assert_eq!(config.layouts, vec!["us"]);
        assert_eq!(config.variant, Some("dvorak".to_string()));
    }
}
