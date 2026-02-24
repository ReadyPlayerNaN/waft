//! Niri keyboard shortcuts data model and KDL config I/O.
//!
//! Reads and writes the `binds { }` block from `~/.config/niri/config.kdl`.

pub mod bind_editor;
pub mod bind_row;

use std::fmt;
use std::path::Path;

/// Keyboard modifier keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Modifier {
    Mod,
    Shift,
    Ctrl,
    Alt,
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Modifier::Mod => write!(f, "Mod"),
            Modifier::Shift => write!(f, "Shift"),
            Modifier::Ctrl => write!(f, "Ctrl"),
            Modifier::Alt => write!(f, "Alt"),
        }
    }
}

impl Modifier {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "Mod" | "Super" => Some(Modifier::Mod),
            "Shift" => Some(Modifier::Shift),
            "Ctrl" | "Control" => Some(Modifier::Ctrl),
            "Alt" => Some(Modifier::Alt),
            _ => None,
        }
    }
}

/// A keyboard shortcut action.
#[derive(Debug, Clone, PartialEq)]
pub enum BindAction {
    Spawn { command: String, args: Vec<String> },
    NiriAction { name: String, args: Vec<String> },
}

impl BindAction {
    /// Human-readable label for the action.
    pub fn label(&self) -> String {
        match self {
            BindAction::Spawn { command, args } => {
                if args.is_empty() {
                    format!("spawn {command}")
                } else {
                    format!("spawn {command} {}", args.join(" "))
                }
            }
            BindAction::NiriAction { name, args } => {
                if args.is_empty() {
                    name.clone()
                } else {
                    format!("{name} {}", args.join(" "))
                }
            }
        }
    }
}

/// A single keyboard shortcut entry.
#[derive(Debug, Clone, PartialEq)]
pub struct BindEntry {
    pub modifiers: Vec<Modifier>,
    pub key: String,
    pub action: BindAction,
    pub hotkey_overlay_title: Option<String>,
    pub allow_when_locked: bool,
    pub repeat: Option<bool>,
}

impl BindEntry {
    /// Format the key chord (e.g. "Mod+Shift+D").
    pub fn key_chord(&self) -> String {
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| m.to_string()).collect();
        parts.push(self.key.clone());
        parts.join("+")
    }
}

/// Result of loading binds: parsed entries and unparseable raw lines.
pub struct LoadedBinds {
    pub entries: Vec<BindEntry>,
    pub raw: Vec<String>,
}

/// Load keyboard shortcuts from the niri KDL config.
///
/// Returns parsed entries and a list of unparseable bind node names.
pub fn load_binds(config_path: &Path) -> Result<LoadedBinds, String> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadedBinds {
                entries: Vec::new(),
                raw: Vec::new(),
            })
        }
        Err(e) => return Err(format!("Failed to read config: {e}")),
    };

    let doc: kdl::KdlDocument = content.parse().map_err(|e| format!("KDL parse error: {e}"))?;

    let binds_node = doc.nodes().iter().find(|n| n.name().value() == "binds");
    let binds_doc = match binds_node {
        Some(node) => match node.children() {
            Some(children) => children,
            None => {
                return Ok(LoadedBinds {
                    entries: Vec::new(),
                    raw: Vec::new(),
                })
            }
        },
        None => {
            return Ok(LoadedBinds {
                entries: Vec::new(),
                raw: Vec::new(),
            })
        }
    };

    let mut entries = Vec::new();
    let mut raw = Vec::new();

    for node in binds_doc.nodes() {
        let key_chord = node.name().value();
        match parse_bind_node(key_chord, node) {
            Some(entry) => entries.push(entry),
            None => raw.push(key_chord.to_string()),
        }
    }

    Ok(LoadedBinds { entries, raw })
}

/// Parse a single bind node into a BindEntry.
fn parse_bind_node(key_chord: &str, node: &kdl::KdlNode) -> Option<BindEntry> {
    // Parse modifiers and key from the node name (e.g. "Mod+Shift+D")
    let parts: Vec<&str> = key_chord.split('+').collect();
    if parts.is_empty() {
        return None;
    }

    let mut modifiers = Vec::new();
    let mut key = None;

    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part is always the key
            key = Some(part.to_string());
        } else if let Some(m) = Modifier::from_str(part) {
            modifiers.push(m);
        } else {
            // Unknown modifier, skip this bind
            return None;
        }
    }

    let key = key?;

    // Check for cooldown-ms or other complex properties we don't support
    for entry in node.entries() {
        if let Some(name) = entry.name() {
            let name_val = name.value();
            if name_val == "cooldown-ms" {
                return None;
            }
        }
    }

    // Parse optional properties from entries
    let mut hotkey_overlay_title = None;
    let mut allow_when_locked = false;
    let mut repeat = None;

    for entry in node.entries() {
        if let Some(name) = entry.name() {
            match name.value() {
                "hotkey-overlay-title" => {
                    hotkey_overlay_title = entry.value().as_string().map(|s| s.to_string());
                }
                "allow-when-locked" => {
                    if let Some(v) = entry.value().as_bool() {
                        allow_when_locked = v;
                    }
                }
                "repeat" => {
                    repeat = entry.value().as_bool();
                }
                _ => {}
            }
        }
    }

    // Parse the action from the child document
    let children = node.children()?;
    let action_node = children.nodes().first()?;
    let action_name = action_node.name().value();

    let action = if action_name == "spawn" {
        let args: Vec<String> = action_node
            .entries()
            .iter()
            .filter(|e| e.name().is_none())
            .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
            .collect();
        if args.is_empty() {
            return None;
        }
        BindAction::Spawn {
            command: args[0].clone(),
            args: args[1..].to_vec(),
        }
    } else {
        let args: Vec<String> = action_node
            .entries()
            .iter()
            .filter(|e| e.name().is_none())
            .filter_map(|e| {
                if let Some(s) = e.value().as_string() {
                    Some(s.to_string())
                } else if let Some(i) = e.value().as_integer() {
                    Some(i.to_string())
                } else {
                    None
                }
            })
            .collect();
        BindAction::NiriAction {
            name: action_name.to_string(),
            args,
        }
    };

    Some(BindEntry {
        modifiers,
        key,
        action,
        hotkey_overlay_title,
        allow_when_locked,
        repeat,
    })
}

/// Save keyboard shortcuts to the niri KDL config.
///
/// Preserves all non-binds content. Replaces the `binds { }` block with
/// the given entries, appending any raw (unparseable) nodes at the end.
pub fn save_binds(
    config_path: &Path,
    entries: &[BindEntry],
    raw_nodes: &[String],
) -> Result<(), String> {
    let mut doc: kdl::KdlDocument = if config_path.exists() {
        let content =
            std::fs::read_to_string(config_path).map_err(|e| format!("Failed to read config: {e}"))?;
        content.parse().map_err(|e| format!("KDL parse error: {e}"))?
    } else {
        kdl::KdlDocument::new()
    };

    // Remove existing binds block
    doc.nodes_mut()
        .retain(|node| node.name().value() != "binds");

    // Build new binds block
    let mut binds_node = kdl::KdlNode::new("binds");
    let mut binds_children = kdl::KdlDocument::new();

    for entry in entries {
        let key_chord = entry.key_chord();
        let mut bind_node = kdl::KdlNode::new(key_chord.as_str());

        // Add optional properties
        if let Some(ref title) = entry.hotkey_overlay_title {
            bind_node.push(kdl::KdlEntry::new_prop("hotkey-overlay-title", title.clone()));
        }
        if entry.allow_when_locked {
            bind_node.push(kdl::KdlEntry::new_prop("allow-when-locked", true));
        }
        if let Some(repeat_val) = entry.repeat {
            bind_node.push(kdl::KdlEntry::new_prop("repeat", repeat_val));
        }

        // Add action as child node
        let mut action_doc = kdl::KdlDocument::new();
        match &entry.action {
            BindAction::Spawn { command, args } => {
                let mut spawn_node = kdl::KdlNode::new("spawn");
                spawn_node.push(kdl::KdlEntry::new(command.clone()));
                for arg in args {
                    spawn_node.push(kdl::KdlEntry::new(arg.clone()));
                }
                action_doc.nodes_mut().push(spawn_node);
            }
            BindAction::NiriAction { name, args } => {
                let mut action_node = kdl::KdlNode::new(name.as_str());
                for arg in args {
                    // Try to parse as integer for numeric args
                    if let Ok(num) = arg.parse::<i128>() {
                        action_node.push(kdl::KdlEntry::new(num));
                    } else {
                        action_node.push(kdl::KdlEntry::new(arg.clone()));
                    }
                }
                action_doc.nodes_mut().push(action_node);
            }
        }

        bind_node.set_children(action_doc);
        binds_children.nodes_mut().push(bind_node);
    }

    // Append raw (unparseable) nodes
    for raw_name in raw_nodes {
        let raw_node = kdl::KdlNode::new(raw_name.as_str());
        binds_children.nodes_mut().push(raw_node);
    }

    binds_node.set_children(binds_children);
    doc.nodes_mut().push(binds_node);

    // Backup existing file
    if config_path.exists() {
        let backup_path = config_path.with_extension("kdl.bak");
        if let Err(e) = std::fs::copy(config_path, &backup_path) {
            log::warn!("[keyboard-shortcuts] Failed to create backup: {e}");
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

/// Curated allowlist of XKB key names.
pub const XKB_KEY_ALLOWLIST: &[&str] = &[
    // Function keys
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    // Arrow keys
    "Left", "Right", "Up", "Down",
    // Navigation
    "Return", "Escape", "Tab", "BackSpace", "Delete", "Home", "End",
    "Page_Up", "Page_Down", "Insert",
    // Common special
    "Space", "Slash", "Comma", "Period", "Semicolon", "Minus", "Equal",
    "BracketLeft", "BracketRight", "Backslash", "Apostrophe", "Grave",
    // Letters
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M",
    "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
    // Digits
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
    // XF86 media keys
    "XF86AudioRaiseVolume", "XF86AudioLowerVolume", "XF86AudioMute",
    "XF86AudioMicMute", "XF86AudioPlay", "XF86AudioPause", "XF86AudioStop",
    "XF86AudioNext", "XF86AudioPrev",
    "XF86MonBrightnessUp", "XF86MonBrightnessDown",
    "Print",
];

/// Check if a key name is in the XKB allowlist.
pub fn validate_key(key: &str) -> bool {
    XKB_KEY_ALLOWLIST.contains(&key)
}

/// Common niri action names grouped by category.
pub const NIRI_ACTIONS: &[(&str, &[&str])] = &[
    (
        "Focus",
        &[
            "focus-column-left",
            "focus-column-right",
            "focus-column-first",
            "focus-column-last",
            "focus-window-down",
            "focus-window-up",
            "focus-window-or-workspace-down",
            "focus-window-or-workspace-up",
        ],
    ),
    (
        "Window",
        &[
            "close-window",
            "move-column-left",
            "move-column-right",
            "move-column-to-first",
            "move-column-to-last",
            "move-window-down",
            "move-window-up",
            "move-window-down-or-to-workspace-down",
            "move-window-up-or-to-workspace-up",
            "consume-or-expel-window-left",
            "consume-or-expel-window-right",
            "maximize-column",
            "fullscreen-window",
            "center-column",
        ],
    ),
    (
        "Workspace",
        &[
            "focus-workspace-down",
            "focus-workspace-up",
            "focus-workspace",
            "move-column-to-workspace-down",
            "move-column-to-workspace-up",
            "move-column-to-workspace",
            "move-workspace-down",
            "move-workspace-up",
        ],
    ),
    (
        "Layout",
        &[
            "switch-preset-column-width",
            "set-column-width",
            "reset-window-height",
            "switch-layout",
        ],
    ),
    (
        "Session",
        &["quit", "power-off-monitors", "suspend", "toggle-debug-tint"],
    ),
    (
        "Display",
        &[
            "focus-monitor-left",
            "focus-monitor-right",
            "focus-monitor-down",
            "focus-monitor-up",
            "move-column-to-monitor-left",
            "move-column-to-monitor-right",
            "move-column-to-monitor-down",
            "move-column-to-monitor-up",
            "move-workspace-to-monitor-left",
            "move-workspace-to-monitor-right",
            "move-workspace-to-monitor-down",
            "move-workspace-to-monitor-up",
        ],
    ),
    ("Screenshot", &["screenshot", "screenshot-screen", "screenshot-window"]),
];

/// Get a flat list of all niri action names.
pub fn all_action_names() -> Vec<&'static str> {
    let mut names = vec!["spawn"];
    for (_, actions) in NIRI_ACTIONS {
        names.extend_from_slice(actions);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        let loaded = load_binds(&path).unwrap();
        assert!(loaded.entries.is_empty());
        assert!(loaded.raw.is_empty());
    }

    #[test]
    fn load_no_binds_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"some-setting "value""#).unwrap();
        drop(f);

        let loaded = load_binds(&path).unwrap();
        assert!(loaded.entries.is_empty());
    }

    #[test]
    fn parse_simple_spawn_bind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"binds {{
    Mod+T {{
        spawn "foot";
    }}
}}"#
        )
        .unwrap();
        drop(f);

        let loaded = load_binds(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].modifiers, vec![Modifier::Mod]);
        assert_eq!(loaded.entries[0].key, "T");
        assert_eq!(
            loaded.entries[0].action,
            BindAction::Spawn {
                command: "foot".to_string(),
                args: vec![],
            }
        );
    }

    #[test]
    fn parse_niri_action_bind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"binds {{
    Mod+Left {{
        focus-column-left;
    }}
}}"#
        )
        .unwrap();
        drop(f);

        let loaded = load_binds(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(
            loaded.entries[0].action,
            BindAction::NiriAction {
                name: "focus-column-left".to_string(),
                args: vec![],
            }
        );
    }

    #[test]
    fn round_trip_binds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"some-other-setting "keep-me""#).unwrap();
        writeln!(
            f,
            r#"binds {{
    Mod+T {{
        spawn "foot";
    }}
}}"#
        )
        .unwrap();
        drop(f);

        let loaded = load_binds(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);

        // Save with modified entries
        let new_entries = vec![BindEntry {
            modifiers: vec![Modifier::Mod, Modifier::Shift],
            key: "D".to_string(),
            action: BindAction::Spawn {
                command: "fuzzel".to_string(),
                args: vec![],
            },
            hotkey_overlay_title: Some("Launcher".to_string()),
            allow_when_locked: false,
            repeat: None,
        }];
        save_binds(&path, &new_entries, &[]).unwrap();

        // Verify other settings preserved
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("some-other-setting"));
        assert!(content.contains("fuzzel"));
        assert!(!content.contains("foot"));

        // Reload and verify
        let reloaded = load_binds(&path).unwrap();
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.entries[0].key_chord(), "Mod+Shift+D");
        assert_eq!(
            reloaded.entries[0].hotkey_overlay_title,
            Some("Launcher".to_string())
        );
    }

    #[test]
    fn validate_key_accepts_known() {
        assert!(validate_key("T"));
        assert!(validate_key("F1"));
        assert!(validate_key("XF86AudioMute"));
        assert!(validate_key("Return"));
    }

    #[test]
    fn validate_key_rejects_unknown() {
        assert!(!validate_key("FooBar"));
        assert!(!validate_key(""));
        assert!(!validate_key("mouse_button_1"));
    }

    #[test]
    fn key_chord_formatting() {
        let entry = BindEntry {
            modifiers: vec![Modifier::Mod, Modifier::Shift],
            key: "T".to_string(),
            action: BindAction::NiriAction {
                name: "quit".to_string(),
                args: vec![],
            },
            hotkey_overlay_title: None,
            allow_when_locked: false,
            repeat: None,
        };
        assert_eq!(entry.key_chord(), "Mod+Shift+T");
    }
}
