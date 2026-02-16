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
    pub layout_names: Vec<String>,
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
            layout_names: vec![],
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

/// Known xkb_symbols components that are not keyboard layouts.
const XKB_NON_LAYOUT_COMPONENTS: &[&str] = &[
    "pc",
    "inet",
    "ctrl",
    "altwin",
    "grp",
    "compose",
    "terminate",
    "keypad",
    "misc",
    "lv3",
    "shift",
    "capslock",
    "eurosign",
    "nbsp",
    "numpad",
    "compat",
    "sun_compat",
    "level3",
    "level5",
];

/// A single layout entry within an XKB include string.
#[derive(Debug, Clone, PartialEq)]
struct XkbLayout {
    code: String,
    variant: Option<String>,
}

/// Parsed XKB include string, split into prefix/layouts/suffix components.
///
/// Example: `"pc+us+cz(qwerty):2+inet(evdev)"` parses to:
/// - prefix: `["pc"]`
/// - layouts: `[XkbLayout { code: "us", variant: None }, XkbLayout { code: "cz", variant: Some("qwerty") }]`
/// - suffix: `["inet(evdev)"]`
#[derive(Debug, Clone, PartialEq)]
struct XkbInclude {
    prefix: Vec<String>,
    layouts: Vec<XkbLayout>,
    suffix: Vec<String>,
}

/// Parse an XKB include string into structured prefix/layouts/suffix components.
///
/// Classifies each `+`-separated component as either a known non-layout component
/// (prefix/suffix) or a layout entry. Group suffixes (`:N`) are stripped from layouts
/// since they are positional and reconstructed by `build_xkb_include`.
fn parse_xkb_include_full(include: &str) -> XkbInclude {
    let components: Vec<&str> = include.split('+').collect();

    struct Classified {
        layout: bool,
        base: String,
        name: String,
        variant: Option<String>,
    }

    let classified: Vec<Classified> = components
        .iter()
        .map(|component| {
            // Strip :N group suffix
            let base = component.split(':').next().unwrap_or(component);

            let (name, variant) = if let Some(paren_pos) = base.find('(') {
                let n = &base[..paren_pos];
                let v = base[paren_pos + 1..].trim_end_matches(')');
                (n.to_string(), Some(v.to_string()))
            } else {
                (base.to_string(), None)
            };

            let layout = !XKB_NON_LAYOUT_COMPONENTS.contains(&name.as_str());
            Classified {
                layout,
                base: base.to_string(),
                name,
                variant,
            }
        })
        .collect();

    let first_layout = classified.iter().position(|c| c.layout);
    let last_layout = classified.iter().rposition(|c| c.layout);

    match (first_layout, last_layout) {
        (Some(first), Some(last)) => {
            let prefix = classified[..first]
                .iter()
                .map(|c| c.base.clone())
                .collect();
            let suffix = classified[last + 1..]
                .iter()
                .map(|c| c.base.clone())
                .collect();
            let layouts = classified[first..=last]
                .iter()
                .filter(|c| c.layout)
                .map(|c| XkbLayout {
                    code: c.name.clone(),
                    variant: c.variant.clone(),
                })
                .collect();
            XkbInclude {
                prefix,
                layouts,
                suffix,
            }
        }
        _ => XkbInclude {
            prefix: classified.iter().map(|c| c.base.clone()).collect(),
            layouts: Vec::new(),
            suffix: Vec::new(),
        },
    }
}

/// Reconstruct an XKB include string from structured components.
///
/// Group suffixes (`:N`) are added to the 2nd+ layouts only when multiple
/// layouts are present. Single layouts have no group suffix.
fn build_xkb_include(parsed: &XkbInclude) -> String {
    let mut parts = Vec::new();

    for p in &parsed.prefix {
        parts.push(p.clone());
    }

    let layout_count = parsed.layouts.len();
    for (i, layout) in parsed.layouts.iter().enumerate() {
        let mut part = layout.code.clone();
        if let Some(ref variant) = layout.variant {
            part = format!("{}({})", part, variant);
        }
        if layout_count > 1 && i > 0 {
            part = format!("{}:{}", part, i + 1);
        }
        parts.push(part);
    }

    for s in &parsed.suffix {
        parts.push(s.clone());
    }

    parts.join("+")
}

/// Parse an XKB include string like `"pc+us+cz(qwerty):2+inet(evdev)"`
/// into (layouts, variant_string).
fn parse_xkb_include(include: &str) -> (Vec<String>, Option<String>) {
    let parsed = parse_xkb_include_full(include);

    let layouts: Vec<String> = parsed.layouts.iter().map(|l| l.code.clone()).collect();
    let has_any_variant = parsed.layouts.iter().any(|l| l.variant.is_some());
    let variant_string = if has_any_variant {
        Some(
            parsed
                .layouts
                .iter()
                .map(|l| l.variant.as_deref().unwrap_or(""))
                .collect::<Vec<_>>()
                .join(","),
        )
    } else {
        None
    };

    (layouts, variant_string)
}

/// Parsed content from an XKB file's `xkb_symbols` section.
struct XkbParsedContent {
    layouts: Vec<String>,
    variant: Option<String>,
    /// Custom names parallel to layouts; missing entries are empty strings.
    names: Vec<String>,
}

/// Extract layouts, variants, and custom names from XKB file content by parsing the
/// `include` directive and `name[groupN]` entries inside the `xkb_symbols` section.
fn parse_xkb_content(content: &str) -> Option<XkbParsedContent> {
    let mut in_symbols = false;
    let mut layouts: Option<(Vec<String>, Option<String>)> = None;
    let mut name_map: std::collections::BTreeMap<usize, String> = std::collections::BTreeMap::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("xkb_symbols") {
            in_symbols = true;
            continue;
        }

        if in_symbols {
            if layouts.is_none() {
                if let Some(rest) = trimmed.strip_prefix("include \"") {
                    if let Some(include_str) = rest.strip_suffix('"') {
                        let (l, v) = parse_xkb_include(include_str);
                        if !l.is_empty() {
                            layouts = Some((l, v));
                        }
                    }
                }
            }

            // Parse name[groupN]="...";
            if let Some(rest) = trimmed.strip_prefix("name[group") {
                if let Some(bracket_pos) = rest.find(']') {
                    if let Ok(group_num) = rest[..bracket_pos].parse::<usize>() {
                        // Extract the quoted name between `="` and `";`
                        let after_bracket = &rest[bracket_pos + 1..];
                        if let Some(eq_quote) = after_bracket.find("=\"") {
                            let name_start = eq_quote + 2;
                            if let Some(quote_end) = after_bracket[name_start..].find('"') {
                                let name = &after_bracket[name_start..name_start + quote_end];
                                // group numbers are 1-based, convert to 0-based
                                name_map.insert(group_num.saturating_sub(1), name.to_string());
                            }
                        }
                    }
                }
            }

            // Stop at closing brace of xkb_symbols
            if trimmed == "};" || trimmed == "}" {
                in_symbols = false;
            }
        }
    }

    let (layout_list, variant) = layouts?;

    // Build names vec parallel to layouts
    let names: Vec<String> = (0..layout_list.len())
        .map(|i| name_map.remove(&i).unwrap_or_default())
        .collect();

    Some(XkbParsedContent {
        layouts: layout_list,
        variant,
        names,
    })
}

/// Expand `~` to home directory in a file path.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
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
            let expanded = expand_tilde(file_path);
            let (layouts, variant, layout_names) = match std::fs::read_to_string(&expanded) {
                Ok(content) => match parse_xkb_content(&content) {
                    Some(parsed) => (parsed.layouts, parsed.variant, parsed.names),
                    None => {
                        log::warn!(
                            "[niri] Could not extract layouts from XKB file: {}",
                            file_path
                        );
                        (vec![], None, vec![])
                    }
                },
                Err(e) => {
                    log::warn!("[niri] Failed to read XKB file '{}': {}", file_path, e);
                    (vec![], None, vec![])
                }
            };

            return KeyboardConfig {
                mode: KeyboardConfigMode::ExternalFile,
                file_path: Some(file_path.to_string()),
                layouts,
                layout_names,
                variant,
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

/// Create a KDL entry with an explicitly quoted string value.
///
/// KDL v2's default Display omits quotes for strings that happen to be valid
/// identifiers (e.g. `us,de` — commas are valid in KDL v2 identifiers).
/// Niri uses KDL v1 format where values like `layout "us,de"` must be quoted.
fn quoted_kdl_entry(value: &str) -> kdl::KdlEntry {
    let mut entry = kdl::KdlEntry::new(value);
    entry.set_format(kdl::KdlEntryFormat {
        leading: " ".into(),
        value_repr: format!("\"{}\"", value),
        ..Default::default()
    });
    entry
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
        existing_layout.push(quoted_kdl_entry(&layout_str));
    } else {
        let mut layout_node = kdl::KdlNode::new("layout");
        layout_node.push(quoted_kdl_entry(&layout_str));
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

/// Modify the `include` directive and `name[groupN]` lines inside the `xkb_symbols` section.
///
/// Parses the existing include string, updates layout codes (preserving variants for
/// layouts that remain, no variant for newly added ones), and reconstructs the include
/// line. Group suffixes (`:N`) are recalculated based on new layout positions.
/// Also replaces all `name[groupN]` lines with entries from `new_names`.
pub fn modify_xkb_content(
    content: &str,
    new_layouts: &[String],
    new_names: &[String],
) -> Result<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut in_symbols = false;
    let mut found = false;
    let mut indent = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("xkb_symbols") {
            in_symbols = true;
            lines.push(line.to_string());
            continue;
        }

        if in_symbols {
            // Skip existing name[groupN] lines -- we'll regenerate them
            if trimmed.starts_with("name[group") {
                continue;
            }

            if !found {
                if let Some(rest) = trimmed.strip_prefix("include \"") {
                    if let Some(include_str) = rest.strip_suffix('"') {
                        let mut parsed = parse_xkb_include_full(include_str);

                        let old_layouts = std::mem::take(&mut parsed.layouts);
                        let mut new_layout_structs = Vec::new();
                        for code in new_layouts {
                            let existing = old_layouts.iter().find(|l| l.code == *code);
                            new_layout_structs.push(XkbLayout {
                                code: code.clone(),
                                variant: existing.and_then(|l| l.variant.clone()),
                            });
                        }
                        parsed.layouts = new_layout_structs;

                        let new_include = build_xkb_include(&parsed);
                        indent = line[..line.len() - trimmed.len()].to_string();
                        lines.push(format!("{}include \"{}\"", indent, new_include));
                        found = true;
                        continue;
                    }
                }
            }

            // Insert new name lines before the closing brace
            if trimmed == "};" || trimmed == "}" {
                for (i, name) in new_names.iter().enumerate() {
                    if !name.is_empty() {
                        lines.push(format!(
                            "{}name[group{}]=\"{}\";",
                            indent,
                            i + 1,
                            name
                        ));
                    }
                }
                in_symbols = false;
            }
        }

        lines.push(line.to_string());
    }

    if !found {
        anyhow::bail!("Could not find 'include' directive in xkb_symbols section");
    }

    let mut result = lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }

    Ok(result)
}

/// Write keyboard layouts and names to an external XKB file.
///
/// Reads the file, modifies the `include` directive and `name[groupN]` lines
/// in the `xkb_symbols` section, creates a `.backup` copy, and writes the
/// modified content back.
pub fn write_xkb_layouts(file_path: &str, new_layouts: &[String], new_names: &[String]) -> Result<()> {
    let expanded = expand_tilde(file_path);
    let path = Path::new(&expanded);

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read XKB file: {}", expanded))?;

    let modified = modify_xkb_content(&content, new_layouts, new_names)?;

    let backup_path = format!("{}.backup", expanded);
    std::fs::copy(path, &backup_path)
        .with_context(|| format!("Failed to create XKB backup: {}", backup_path))?;

    std::fs::write(path, &modified)
        .with_context(|| format!("Failed to write XKB file: {}", expanded))?;

    Ok(())
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
        let output = modified.to_string();
        assert!(
            output.contains(r#"layout "us,de,fr""#),
            "Expected quoted layout value, got:\n{}",
            output
        );
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

        // Verify layout value is quoted
        assert!(
            modified_str.contains(r#"layout "fr,de""#),
            "Expected quoted layout value in:\n{}",
            modified_str
        );
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
    fn modify_config_quotes_layout_in_v2_config() {
        // Pure v2 config — no v1 features. KDL v2 would write string values
        // as bare identifiers if they're valid idents. We must force quotes
        // so niri can parse the file.
        let kdl = r#"input {
    keyboard {
        xkb {
            layout "us,cz"
            variant ",qwerty"
        }
    }
}
"#;

        let doc: KdlDocument = kdl.parse().unwrap();
        let modified =
            modify_keyboard_layouts(doc, vec!["cz".into(), "us".into()]).unwrap();
        let output = modified.to_string();
        eprintln!("=== v2 config output ===\n{}", output);

        assert!(
            output.contains(r#"layout "cz,us""#),
            "Expected quoted layout, got:\n{}",
            output
        );
        // variant is not touched by modify_keyboard_layouts, check it stays quoted
        assert!(
            output.contains(r#"variant ",qwerty""#),
            "Expected quoted variant, got:\n{}",
            output
        );
    }

    #[test]
    fn modify_config_preserves_quotes_in_v1_config() {
        // Real niri configs use KDL v1 raw strings, which trigger v1-fallback.
        // Verify that modifying layouts still produces valid quoted output.
        let kdl = r##"input {
    keyboard {
        xkb {
            layout "us,cz"
            variant ",qwerty"
            options "grp:win_space_toggle"
        }
    }
}
window-rule {
    match app-id=r#"^org\.wezfurlong\.wezterm$"#
    default-column-width {}
}
"##;

        let doc: KdlDocument = kdl.parse().unwrap();
        let modified =
            modify_keyboard_layouts(doc, vec!["cz".into(), "us".into()]).unwrap();
        let output = modified.to_string();
        eprintln!("=== v1 config output ===\n{}", output);

        // Layout must be quoted
        assert!(
            output.contains(r#"layout "cz,us""#),
            "Expected quoted layout, got:\n{}",
            output
        );
        // Variant and options must stay quoted
        assert!(
            output.contains(r#"variant ",qwerty""#),
            "Expected quoted variant, got:\n{}",
            output
        );
        assert!(
            output.contains(r#"options "grp:win_space_toggle""#),
            "Expected quoted options, got:\n{}",
            output
        );
    }

    #[test]
    fn parse_config_with_v1_raw_strings() {
        // Niri configs use KDL v1 raw strings (r#"..."#) which are invalid in KDL v2.
        // The v1-fallback feature makes .parse() fall back to v1 when v2 parsing fails.
        let kdl = r##"
            input {
                keyboard {
                    xkb {
                        layout "us,cz"
                    }
                }
            }
            window-rule {
                match app-id=r#"^org\.wezfurlong\.wezterm$"#
                default-column-width {}
            }
        "##;

        let config = parse_keyboard_config_from_string(kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::LayoutList);
        assert_eq!(config.layouts, vec!["us", "cz"]);
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

    #[test]
    fn parse_xkb_symbols_include_basic() {
        let (layouts, variant) = parse_xkb_include("pc+us+cz(qwerty):2+inet(evdev)");
        assert_eq!(layouts, vec!["us", "cz"]);
        assert_eq!(variant, Some(",qwerty".to_string()));
    }

    #[test]
    fn parse_xkb_symbols_include_no_variants() {
        let (layouts, variant) = parse_xkb_include("pc+us+de+inet(evdev)");
        assert_eq!(layouts, vec!["us", "de"]);
        assert_eq!(variant, None);
    }

    #[test]
    fn parse_xkb_symbols_include_single_layout() {
        let (layouts, variant) = parse_xkb_include("pc+us+inet(evdev)");
        assert_eq!(layouts, vec!["us"]);
        assert_eq!(variant, None);
    }

    #[test]
    fn parse_xkb_symbols_include_all_variants() {
        let (layouts, variant) = parse_xkb_include("pc+us(dvorak)+de(nodeadkeys)+inet(evdev)");
        assert_eq!(layouts, vec!["us", "de"]);
        assert_eq!(variant, Some("dvorak,nodeadkeys".to_string()));
    }

    #[test]
    fn parse_xkb_file_content() {
        let xkb = r#"
xkb_keymap {
    xkb_keycodes  { include "evdev+aliases(qwerty)" };
    xkb_types     { include "complete" };
    xkb_compat    { include "complete" };
    xkb_symbols   {
        include "pc+us+cz(qwerty):2+inet(evdev)"
        name[group1]="English (US)";
        name[group2]="Czech (QWERTY)";
    };
    xkb_geometry  { include "pc(pc105)" };
};
"#;

        let result = parse_xkb_content(xkb);
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.layouts, vec!["us", "cz"]);
        assert_eq!(parsed.variant, Some(",qwerty".to_string()));
        assert_eq!(parsed.names, vec!["English (US)", "Czech (QWERTY)"]);
    }

    #[test]
    fn parse_xkb_content_with_names() {
        let xkb = r#"xkb_keymap {
    xkb_symbols   {
        include "pc+us+de:2+fr:3+inet(evdev)"
        name[group1]="English (US)";
        name[group2]="German";
        name[group3]="French";
    };
};
"#;

        let parsed = parse_xkb_content(xkb).unwrap();
        assert_eq!(parsed.layouts, vec!["us", "de", "fr"]);
        assert_eq!(parsed.names, vec!["English (US)", "German", "French"]);
    }

    #[test]
    fn parse_xkb_content_partial_names() {
        // 3 layouts but only 2 name entries
        let xkb = r#"xkb_keymap {
    xkb_symbols   {
        include "pc+us+cz(qwerty):2+sk:3+inet(evdev)"
        name[group1]="English (US)";
        name[group2]="Czech (QWERTY)";
    };
};
"#;

        let parsed = parse_xkb_content(xkb).unwrap();
        assert_eq!(parsed.layouts, vec!["us", "cz", "sk"]);
        assert_eq!(parsed.names, vec!["English (US)", "Czech (QWERTY)", ""]);
    }

    #[test]
    fn parse_config_external_file_with_xkb_layouts() {
        use std::io::Write;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let xkb_path = temp_dir.path().join("keymap.xkb");

        let mut xkb_file = std::fs::File::create(&xkb_path).unwrap();
        write!(
            xkb_file,
            r#"xkb_keymap {{
    xkb_keycodes  {{ include "evdev+aliases(qwerty)" }};
    xkb_types     {{ include "complete" }};
    xkb_compat    {{ include "complete" }};
    xkb_symbols   {{
        include "pc+us+cz(qwerty):2+inet(evdev)"
        name[group1]="English (US)";
        name[group2]="Czech (QWERTY)";
    }};
    xkb_geometry  {{ include "pc(pc105)" }};
}};
"#
        )
        .unwrap();
        drop(xkb_file);

        let kdl = format!(
            r#"
            input {{
                keyboard {{
                    xkb {{
                        file "{}"
                    }}
                }}
            }}
        "#,
            xkb_path.display()
        );

        let config = parse_keyboard_config_from_string(&kdl).unwrap();
        assert_eq!(config.mode, KeyboardConfigMode::ExternalFile);
        assert_eq!(config.layouts, vec!["us", "cz"]);
        assert_eq!(config.variant, Some(",qwerty".to_string()));
        assert!(config.file_path.is_some());
    }

    #[test]
    fn parse_xkb_include_full_basic() {
        let parsed = parse_xkb_include_full("pc+us+cz(qwerty):2+inet(evdev)");
        assert_eq!(parsed.prefix, vec!["pc"]);
        assert_eq!(
            parsed.layouts,
            vec![
                XkbLayout {
                    code: "us".to_string(),
                    variant: None
                },
                XkbLayout {
                    code: "cz".to_string(),
                    variant: Some("qwerty".to_string())
                },
            ]
        );
        assert_eq!(parsed.suffix, vec!["inet(evdev)"]);
    }

    #[test]
    fn build_xkb_include_roundtrip() {
        let include = "pc+us+cz(qwerty):2+inet(evdev)";
        let parsed = parse_xkb_include_full(include);
        let rebuilt = build_xkb_include(&parsed);
        assert_eq!(rebuilt, include);
    }

    #[test]
    fn build_xkb_include_single_layout() {
        let parsed = XkbInclude {
            prefix: vec!["pc".to_string()],
            layouts: vec![XkbLayout {
                code: "us".to_string(),
                variant: None,
            }],
            suffix: vec!["inet(evdev)".to_string()],
        };
        let result = build_xkb_include(&parsed);
        assert_eq!(result, "pc+us+inet(evdev)");
    }

    #[test]
    fn modify_xkb_content_add_layout() {
        let xkb = r#"xkb_keymap {
    xkb_keycodes  { include "evdev+aliases(qwerty)" };
    xkb_types     { include "complete" };
    xkb_compat    { include "complete" };
    xkb_symbols   {
        include "pc+us+cz(qwerty):2+inet(evdev)"
        name[group1]="English (US)";
        name[group2]="Czech (QWERTY)";
    };
    xkb_geometry  { include "pc(pc105)" };
};
"#;

        let modified = modify_xkb_content(
            xkb,
            &["us".into(), "cz".into(), "de".into()],
            &["English (US)".into(), "Czech (QWERTY)".into(), "German".into()],
        )
        .unwrap();
        assert!(modified.contains(r#"include "pc+us+cz(qwerty):2+de:3+inet(evdev)""#));
        assert!(modified.contains(r#"name[group1]="English (US)";"#));
        assert!(modified.contains(r#"name[group2]="Czech (QWERTY)";"#));
        assert!(modified.contains(r#"name[group3]="German";"#));
        // Old name lines should be removed (no duplicates)
        assert_eq!(modified.matches("name[group1]").count(), 1);
        assert_eq!(modified.matches("name[group2]").count(), 1);
    }

    #[test]
    fn modify_xkb_content_remove_layout() {
        let xkb = r#"xkb_keymap {
    xkb_symbols   {
        include "pc+us+cz(qwerty):2+inet(evdev)"
        name[group1]="English (US)";
        name[group2]="Czech (QWERTY)";
    };
};
"#;

        let modified = modify_xkb_content(
            xkb,
            &["us".into()],
            &["English (US)".into()],
        )
        .unwrap();
        assert!(modified.contains(r#"include "pc+us+inet(evdev)""#));
        assert!(modified.contains(r#"name[group1]="English (US)";"#));
        assert!(!modified.contains("name[group2]"));
    }

    #[test]
    fn modify_xkb_content_reorder_layouts() {
        let xkb = r#"xkb_keymap {
    xkb_symbols   {
        include "pc+us+cz(qwerty):2+inet(evdev)"
        name[group1]="English (US)";
        name[group2]="Czech (QWERTY)";
    };
};
"#;

        let modified = modify_xkb_content(
            xkb,
            &["cz".into(), "us".into()],
            &["Czech (QWERTY)".into(), "English (US)".into()],
        )
        .unwrap();
        // cz should keep its qwerty variant, now in position 1 (no suffix)
        // us should be in position 2 (with :2 suffix)
        assert!(modified.contains(r#"include "pc+cz(qwerty)+us:2+inet(evdev)""#));
        assert!(modified.contains(r#"name[group1]="Czech (QWERTY)";"#));
        assert!(modified.contains(r#"name[group2]="English (US)";"#));
    }

    #[test]
    fn modify_xkb_content_no_names() {
        // When names are empty, no name lines should be written
        let xkb = r#"xkb_keymap {
    xkb_symbols   {
        include "pc+us+de:2+inet(evdev)"
    };
};
"#;

        let modified = modify_xkb_content(xkb, &["us".into(), "de".into()], &[]).unwrap();
        assert!(modified.contains(r#"include "pc+us+de:2+inet(evdev)""#));
        assert!(!modified.contains("name[group"));
    }

    #[test]
    fn write_xkb_layouts_creates_backup() {
        use std::io::Write;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let xkb_path = temp_dir.path().join("keymap.xkb");
        let backup_path = temp_dir.path().join("keymap.xkb.backup");

        let original = r#"xkb_keymap {
    xkb_symbols   {
        include "pc+us+cz(qwerty):2+inet(evdev)"
    };
};
"#;

        let mut file = std::fs::File::create(&xkb_path).unwrap();
        write!(file, "{}", original).unwrap();
        drop(file);

        write_xkb_layouts(
            xkb_path.to_str().unwrap(),
            &["us".into(), "cz".into(), "de".into()],
            &["English (US)".into(), "Czech (QWERTY)".into(), "German".into()],
        )
        .unwrap();

        // Backup should contain original content
        assert!(backup_path.exists());
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, original);

        // New content should have 3 layouts
        let new_content = std::fs::read_to_string(&xkb_path).unwrap();
        assert!(new_content.contains(r#"include "pc+us+cz(qwerty):2+de:3+inet(evdev)""#));
        assert!(new_content.contains(r#"name[group1]="English (US)";"#));
        assert!(new_content.contains(r#"name[group3]="German";"#));
    }
}
