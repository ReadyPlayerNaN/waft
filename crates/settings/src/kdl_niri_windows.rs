//! KDL I/O for niri window appearance settings.
//!
//! Reads and writes the `layout {}` block, `prefer-no-csd`, and `background-color`
//! from `~/.config/niri/config.kdl` using in-place editing to preserve unmanaged nodes.

use std::path::Path;

use crate::kdl_config::KdlConfigFile;

// ── Data model ──

/// Complete niri layout configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct NiriLayoutConfig {
    pub focus_ring: FocusRingConfig,
    pub border: BorderConfig,
    pub shadow: ShadowConfig,
    pub tab_indicator: TabIndicatorConfig,
    pub gaps: u32,
    pub struts: StrutsConfig,
}

impl Default for NiriLayoutConfig {
    fn default() -> Self {
        Self {
            focus_ring: FocusRingConfig::default(),
            border: BorderConfig::default(),
            shadow: ShadowConfig::default(),
            tab_indicator: TabIndicatorConfig::default(),
            gaps: 16,
            struts: StrutsConfig::default(),
        }
    }
}

/// Focus ring configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusRingConfig {
    pub enabled: bool,
    pub width: u32,
    pub active_color: String,
    pub inactive_color: String,
    pub urgent_color: Option<String>,
}

impl Default for FocusRingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 4,
            active_color: "#7fc8ff".to_string(),
            inactive_color: "#505050".to_string(),
            urgent_color: None,
        }
    }
}

/// Window border configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderConfig {
    pub enabled: bool,
    pub width: u32,
    pub active_color: String,
    pub inactive_color: String,
    pub urgent_color: Option<String>,
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            width: 4,
            active_color: "#ffc87f".to_string(),
            inactive_color: "#505050".to_string(),
            urgent_color: None,
        }
    }
}

/// Window shadow configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowConfig {
    pub enabled: bool,
    pub softness: u32,
    pub spread: u32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub color: String,
    pub inactive_color: Option<String>,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            softness: 30,
            spread: 5,
            offset_x: 0,
            offset_y: 0,
            color: "#0007".to_string(),
            inactive_color: None,
        }
    }
}

/// Tab indicator configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct TabIndicatorConfig {
    pub enabled: bool,
    pub position: String,
    pub gap: u32,
    pub width: u32,
    pub corner_radius: u32,
    pub active_color: String,
    pub inactive_color: String,
    pub urgent_color: Option<String>,
}

impl Default for TabIndicatorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: "left".to_string(),
            gap: 4,
            width: 4,
            corner_radius: 8,
            active_color: "#7fc8ff".to_string(),
            inactive_color: "#505050".to_string(),
            urgent_color: None,
        }
    }
}

/// Struts (reserved screen edges) configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct StrutsConfig {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

impl Default for StrutsConfig {
    fn default() -> Self {
        Self {
            left: 0,
            right: 0,
            top: 0,
            bottom: 0,
        }
    }
}

// ── KDL reading helpers ──

/// Get the first positional (unnamed) integer argument from a node, or `None`.
fn node_int_arg(node: &kdl::KdlNode) -> Option<i64> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_integer())
        .map(|v| v as i64)
}

/// Get the first positional string argument from a node, or `None`.
fn node_str_arg(node: &kdl::KdlNode) -> Option<&str> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_string())
}

/// Find a child node by name within a parent node's children block.
fn find_child<'a>(parent: &'a kdl::KdlNode, name: &str) -> Option<&'a kdl::KdlNode> {
    parent
        .children()
        .and_then(|doc| doc.nodes().iter().find(|n| n.name().value() == name))
}

/// Check if a section has an `on` or `off` child; returns None if neither.
fn parse_enabled(parent: &kdl::KdlNode) -> Option<bool> {
    if find_child(parent, "on").is_some() {
        Some(true)
    } else if find_child(parent, "off").is_some() {
        Some(false)
    } else {
        None
    }
}

/// Parse a color value from a named child node (e.g. `active-color "#7fc8ff"`).
fn parse_color_child(parent: &kdl::KdlNode, child_name: &str) -> Option<String> {
    find_child(parent, child_name).and_then(|n| node_str_arg(n).map(|s| s.to_string()))
}

/// Parse an integer from a named child node.
fn parse_int_child(parent: &kdl::KdlNode, child_name: &str) -> Option<i64> {
    find_child(parent, child_name).and_then(node_int_arg)
}

/// Parse the `offset` node which uses named params: `offset x=0 y=0`.
fn parse_offset(parent: &kdl::KdlNode) -> Option<(i32, i32)> {
    let offset_node = find_child(parent, "offset")?;
    let x = offset_node
        .entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some("x"))
        .and_then(|e| e.value().as_integer().map(|v| v as i32))
        .unwrap_or(0);
    let y = offset_node
        .entries()
        .iter()
        .find(|e| e.name().map(|n| n.value()) == Some("y"))
        .and_then(|e| e.value().as_integer().map(|v| v as i32))
        .unwrap_or(0);
    Some((x, y))
}

/// Parse focus-ring config from its KDL node.
fn parse_focus_ring(node: &kdl::KdlNode) -> FocusRingConfig {
    let defaults = FocusRingConfig::default();
    FocusRingConfig {
        enabled: parse_enabled(node).unwrap_or(defaults.enabled),
        width: parse_int_child(node, "width")
            .map(|v| v as u32)
            .unwrap_or(defaults.width),
        active_color: parse_color_child(node, "active-color")
            .unwrap_or(defaults.active_color),
        inactive_color: parse_color_child(node, "inactive-color")
            .unwrap_or(defaults.inactive_color),
        urgent_color: parse_color_child(node, "urgent-color"),
    }
}

/// Parse border config from its KDL node.
fn parse_border(node: &kdl::KdlNode) -> BorderConfig {
    let defaults = BorderConfig::default();
    BorderConfig {
        enabled: parse_enabled(node).unwrap_or(defaults.enabled),
        width: parse_int_child(node, "width")
            .map(|v| v as u32)
            .unwrap_or(defaults.width),
        active_color: parse_color_child(node, "active-color")
            .unwrap_or(defaults.active_color),
        inactive_color: parse_color_child(node, "inactive-color")
            .unwrap_or(defaults.inactive_color),
        urgent_color: parse_color_child(node, "urgent-color"),
    }
}

/// Parse shadow config from its KDL node.
fn parse_shadow(node: &kdl::KdlNode) -> ShadowConfig {
    let defaults = ShadowConfig::default();
    let (offset_x, offset_y) = parse_offset(node).unwrap_or((defaults.offset_x, defaults.offset_y));
    ShadowConfig {
        enabled: parse_enabled(node).unwrap_or(defaults.enabled),
        softness: parse_int_child(node, "softness")
            .map(|v| v as u32)
            .unwrap_or(defaults.softness),
        spread: parse_int_child(node, "spread")
            .map(|v| v as u32)
            .unwrap_or(defaults.spread),
        offset_x,
        offset_y,
        color: parse_color_child(node, "color")
            .unwrap_or(defaults.color),
        inactive_color: parse_color_child(node, "inactive-color"),
    }
}

/// Parse tab-indicator config from its KDL node.
fn parse_tab_indicator(node: &kdl::KdlNode) -> TabIndicatorConfig {
    let defaults = TabIndicatorConfig::default();
    TabIndicatorConfig {
        enabled: parse_enabled(node).unwrap_or(defaults.enabled),
        position: parse_color_child(node, "position")
            .unwrap_or(defaults.position),
        gap: parse_int_child(node, "gap")
            .map(|v| v as u32)
            .unwrap_or(defaults.gap),
        width: parse_int_child(node, "width")
            .map(|v| v as u32)
            .unwrap_or(defaults.width),
        corner_radius: parse_int_child(node, "corner-radius")
            .map(|v| v as u32)
            .unwrap_or(defaults.corner_radius),
        active_color: parse_color_child(node, "active-color")
            .unwrap_or(defaults.active_color),
        inactive_color: parse_color_child(node, "inactive-color")
            .unwrap_or(defaults.inactive_color),
        urgent_color: parse_color_child(node, "urgent-color"),
    }
}

/// Parse struts config from its KDL node.
fn parse_struts(node: &kdl::KdlNode) -> StrutsConfig {
    StrutsConfig {
        left: parse_int_child(node, "left").map(|v| v as u32).unwrap_or(0),
        right: parse_int_child(node, "right").map(|v| v as u32).unwrap_or(0),
        top: parse_int_child(node, "top").map(|v| v as u32).unwrap_or(0),
        bottom: parse_int_child(node, "bottom").map(|v| v as u32).unwrap_or(0),
    }
}

// ── Public API ──

/// Load layout configuration from a niri config file.
///
/// Returns defaults for any missing sections.
pub fn load_layout_config(path: &Path) -> Result<NiriLayoutConfig, String> {
    let config = KdlConfigFile::load(path)?;
    let doc = config.doc();

    let layout_node = doc.nodes().iter().find(|n| n.name().value() == "layout");
    let layout_node = match layout_node {
        Some(n) => n,
        None => return Ok(NiriLayoutConfig::default()),
    };

    let defaults = NiriLayoutConfig::default();

    let focus_ring = find_child(layout_node, "focus-ring")
        .map(parse_focus_ring)
        .unwrap_or(defaults.focus_ring);

    let border = find_child(layout_node, "border")
        .map(parse_border)
        .unwrap_or(defaults.border);

    let shadow = find_child(layout_node, "shadow")
        .map(parse_shadow)
        .unwrap_or(defaults.shadow);

    let tab_indicator = find_child(layout_node, "tab-indicator")
        .map(parse_tab_indicator)
        .unwrap_or(defaults.tab_indicator);

    let gaps = find_child(layout_node, "gaps")
        .and_then(node_int_arg)
        .map(|v| v as u32)
        .unwrap_or(defaults.gaps);

    let struts = find_child(layout_node, "struts")
        .map(parse_struts)
        .unwrap_or(defaults.struts);

    Ok(NiriLayoutConfig {
        focus_ring,
        border,
        shadow,
        tab_indicator,
        gaps,
        struts,
    })
}

// ── KDL writing helpers ──

/// Create a KDL node with a single positional integer argument.
fn int_node(name: &str, value: i128) -> kdl::KdlNode {
    let mut node = kdl::KdlNode::new(name);
    node.push(kdl::KdlEntry::new(value));
    node
}

/// Create a KDL node with a single positional string argument.
fn str_node(name: &str, value: &str) -> kdl::KdlNode {
    let mut node = kdl::KdlNode::new(name);
    node.push(kdl::KdlEntry::new(value.to_string()));
    node
}

/// Build the children document for a focus-ring or border section.
fn build_ring_or_border_children(
    enabled: bool,
    width: u32,
    active_color: &str,
    inactive_color: &str,
    urgent_color: &Option<String>,
) -> kdl::KdlDocument {
    let mut doc = kdl::KdlDocument::new();
    let nodes = doc.nodes_mut();
    nodes.push(kdl::KdlNode::new(if enabled { "on" } else { "off" }));
    nodes.push(int_node("width", width as i128));
    nodes.push(str_node("active-color", active_color));
    nodes.push(str_node("inactive-color", inactive_color));
    if let Some(color) = urgent_color {
        nodes.push(str_node("urgent-color", color));
    }
    doc
}

/// Build children for the shadow section.
fn build_shadow_children(shadow: &ShadowConfig) -> kdl::KdlDocument {
    let mut doc = kdl::KdlDocument::new();
    let nodes = doc.nodes_mut();
    nodes.push(kdl::KdlNode::new(if shadow.enabled { "on" } else { "off" }));
    nodes.push(int_node("softness", shadow.softness as i128));
    nodes.push(int_node("spread", shadow.spread as i128));

    let mut offset = kdl::KdlNode::new("offset");
    let mut entry_x = kdl::KdlEntry::new(shadow.offset_x as i128);
    entry_x.set_name(Some(kdl::KdlIdentifier::from("x")));
    offset.push(entry_x);
    let mut entry_y = kdl::KdlEntry::new(shadow.offset_y as i128);
    entry_y.set_name(Some(kdl::KdlIdentifier::from("y")));
    offset.push(entry_y);
    nodes.push(offset);

    nodes.push(str_node("color", &shadow.color));
    if let Some(ref inactive) = shadow.inactive_color {
        nodes.push(str_node("inactive-color", inactive));
    }
    doc
}

/// Build children for the tab-indicator section.
fn build_tab_indicator_children(ti: &TabIndicatorConfig) -> kdl::KdlDocument {
    let mut doc = kdl::KdlDocument::new();
    let nodes = doc.nodes_mut();
    nodes.push(kdl::KdlNode::new(if ti.enabled { "on" } else { "off" }));
    nodes.push(str_node("position", &ti.position));
    nodes.push(int_node("gap", ti.gap as i128));
    nodes.push(int_node("width", ti.width as i128));
    nodes.push(int_node("corner-radius", ti.corner_radius as i128));
    nodes.push(str_node("active-color", &ti.active_color));
    nodes.push(str_node("inactive-color", &ti.inactive_color));
    if let Some(ref color) = ti.urgent_color {
        nodes.push(str_node("urgent-color", color));
    }
    doc
}

/// Build children for the struts section.
fn build_struts_children(struts: &StrutsConfig) -> kdl::KdlDocument {
    let mut doc = kdl::KdlDocument::new();
    let nodes = doc.nodes_mut();
    nodes.push(int_node("left", struts.left as i128));
    nodes.push(int_node("right", struts.right as i128));
    nodes.push(int_node("top", struts.top as i128));
    nodes.push(int_node("bottom", struts.bottom as i128));
    doc
}

/// Find or create a child node within a parent node's children.
/// Returns a mutable reference to the child node.
fn ensure_child_node(parent: &mut kdl::KdlNode, name: &str) -> usize {
    let children = parent.ensure_children();
    let pos = children
        .nodes()
        .iter()
        .position(|n| n.name().value() == name);
    match pos {
        Some(idx) => idx,
        None => {
            children.nodes_mut().push(kdl::KdlNode::new(name));
            children.nodes().len() - 1
        }
    }
}

/// Save layout configuration to a niri config file.
///
/// Uses in-place editing: loads the existing document, finds or creates the
/// `layout` block, updates each sub-section, and saves with validation and backup.
/// Preserves unmanaged nodes inside `layout {}` (e.g. `preset-column-widths`,
/// `default-column-width`, `center-focused-column`).
pub fn save_layout_config(path: &Path, config: &NiriLayoutConfig) -> Result<(), String> {
    let mut kdl_config = KdlConfigFile::load(path)?;

    // Find or create top-level `layout` node
    let layout_idx = {
        let doc = kdl_config.doc();
        doc.nodes()
            .iter()
            .position(|n| n.name().value() == "layout")
    };
    let layout_idx = match layout_idx {
        Some(idx) => idx,
        None => {
            let mut node = kdl::KdlNode::new("layout");
            node.set_children(kdl::KdlDocument::new());
            kdl_config.doc_mut().nodes_mut().push(node);
            kdl_config.doc().nodes().len() - 1
        }
    };

    let layout = &mut kdl_config.doc_mut().nodes_mut()[layout_idx];

    // Update focus-ring
    let idx = ensure_child_node(layout, "focus-ring");
    let fr_children = build_ring_or_border_children(
        config.focus_ring.enabled,
        config.focus_ring.width,
        &config.focus_ring.active_color,
        &config.focus_ring.inactive_color,
        &config.focus_ring.urgent_color,
    );
    layout.ensure_children().nodes_mut()[idx].set_children(fr_children);

    // Update border
    let idx = ensure_child_node(layout, "border");
    let border_children = build_ring_or_border_children(
        config.border.enabled,
        config.border.width,
        &config.border.active_color,
        &config.border.inactive_color,
        &config.border.urgent_color,
    );
    layout.ensure_children().nodes_mut()[idx].set_children(border_children);

    // Update shadow
    let idx = ensure_child_node(layout, "shadow");
    let shadow_children = build_shadow_children(&config.shadow);
    layout.ensure_children().nodes_mut()[idx].set_children(shadow_children);

    // Update tab-indicator
    let idx = ensure_child_node(layout, "tab-indicator");
    let ti_children = build_tab_indicator_children(&config.tab_indicator);
    layout.ensure_children().nodes_mut()[idx].set_children(ti_children);

    // Update gaps (replace the whole node since it's just a single arg)
    let idx = ensure_child_node(layout, "gaps");
    layout.ensure_children().nodes_mut()[idx] = int_node("gaps", config.gaps as i128);

    // Update struts
    let idx = ensure_child_node(layout, "struts");
    let struts_children = build_struts_children(&config.struts);
    layout.ensure_children().nodes_mut()[idx].set_children(struts_children);

    kdl_config.save()
}

/// Load the `prefer-no-csd` setting from a niri config file.
///
/// Returns `true` if the top-level `prefer-no-csd` node exists.
pub fn load_prefer_no_csd(path: &Path) -> Result<bool, String> {
    let config = KdlConfigFile::load(path)?;
    let found = config
        .doc()
        .nodes()
        .iter()
        .any(|n| n.name().value() == "prefer-no-csd");
    Ok(found)
}

/// Save the `prefer-no-csd` setting to a niri config file.
///
/// If `enabled` is `true`, ensures the top-level node exists.
/// If `false`, removes it.
pub fn save_prefer_no_csd(path: &Path, enabled: bool) -> Result<(), String> {
    let mut config = KdlConfigFile::load(path)?;

    config.remove_nodes_by_name("prefer-no-csd");
    if enabled {
        config
            .doc_mut()
            .nodes_mut()
            .push(kdl::KdlNode::new("prefer-no-csd"));
    }

    config.save()
}

/// Load the background color from a niri config file.
///
/// The background color lives inside `layout { background-color "..." }`.
pub fn load_background_color(path: &Path) -> Result<Option<String>, String> {
    let config = KdlConfigFile::load(path)?;
    let layout = config
        .doc()
        .nodes()
        .iter()
        .find(|n| n.name().value() == "layout");
    let layout = match layout {
        Some(n) => n,
        None => return Ok(None),
    };

    let color = find_child(layout, "background-color")
        .and_then(node_str_arg)
        .map(|s| s.to_string());
    Ok(color)
}

/// Save the background color to a niri config file.
///
/// If `color` is `Some`, sets `layout { background-color "..." }`.
/// If `None`, removes the `background-color` node from `layout`.
pub fn save_background_color(path: &Path, color: Option<&str>) -> Result<(), String> {
    let mut kdl_config = KdlConfigFile::load(path)?;

    // Find or create layout node
    let layout_idx = {
        let doc = kdl_config.doc();
        doc.nodes()
            .iter()
            .position(|n| n.name().value() == "layout")
    };
    let layout_idx = match layout_idx {
        Some(idx) => idx,
        None => {
            if color.is_none() {
                // No layout, no color to remove
                return Ok(());
            }
            let mut node = kdl::KdlNode::new("layout");
            node.set_children(kdl::KdlDocument::new());
            kdl_config.doc_mut().nodes_mut().push(node);
            kdl_config.doc().nodes().len() - 1
        }
    };

    let layout = &mut kdl_config.doc_mut().nodes_mut()[layout_idx];
    let children = layout.ensure_children();

    // Remove existing background-color nodes
    children
        .nodes_mut()
        .retain(|n| n.name().value() != "background-color");

    // Add new one if color is provided
    if let Some(color) = color {
        children.nodes_mut().push(str_node("background-color", color));
    }

    kdl_config.save()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(dir: &tempfile::TempDir, content: &str) -> std::path::PathBuf {
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn load_empty_config_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.kdl");
        let config = load_layout_config(&path).unwrap();
        assert_eq!(config, NiriLayoutConfig::default());
    }

    #[test]
    fn load_populated_config_parses_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(
            &dir,
            r##"
layout {
    gaps 8

    focus-ring {
        on
        width 2
        active-color "#ff0000"
        inactive-color "#00ff00"
        urgent-color "#0000ff"
    }

    border {
        on
        width 1
        active-color "#aabbcc"
        inactive-color "#ddeeff"
    }

    shadow {
        on
        softness 20
        spread 10
        offset x=5 y=-3
        color "#000a"
        inactive-color "#0005"
    }

    tab-indicator {
        on
        position "right"
        gap 8
        width 6
        corner-radius 4
        active-color "#112233"
        inactive-color "#445566"
        urgent-color "#778899"
    }

    struts {
        left 10
        right 20
        top 30
        bottom 40
    }
}
"##,
        );

        let config = load_layout_config(&path).unwrap();
        assert_eq!(config.gaps, 8);

        assert!(config.focus_ring.enabled);
        assert_eq!(config.focus_ring.width, 2);
        assert_eq!(config.focus_ring.active_color, "#ff0000");
        assert_eq!(config.focus_ring.inactive_color, "#00ff00");
        assert_eq!(config.focus_ring.urgent_color, Some("#0000ff".to_string()));

        assert!(config.border.enabled);
        assert_eq!(config.border.width, 1);
        assert_eq!(config.border.active_color, "#aabbcc");
        assert_eq!(config.border.inactive_color, "#ddeeff");
        assert_eq!(config.border.urgent_color, None);

        assert!(config.shadow.enabled);
        assert_eq!(config.shadow.softness, 20);
        assert_eq!(config.shadow.spread, 10);
        assert_eq!(config.shadow.offset_x, 5);
        assert_eq!(config.shadow.offset_y, -3);
        assert_eq!(config.shadow.color, "#000a");
        assert_eq!(config.shadow.inactive_color, Some("#0005".to_string()));

        assert!(config.tab_indicator.enabled);
        assert_eq!(config.tab_indicator.position, "right");
        assert_eq!(config.tab_indicator.gap, 8);
        assert_eq!(config.tab_indicator.width, 6);
        assert_eq!(config.tab_indicator.corner_radius, 4);
        assert_eq!(config.tab_indicator.active_color, "#112233");
        assert_eq!(config.tab_indicator.inactive_color, "#445566");
        assert_eq!(
            config.tab_indicator.urgent_color,
            Some("#778899".to_string())
        );

        assert_eq!(config.struts.left, 10);
        assert_eq!(config.struts.right, 20);
        assert_eq!(config.struts.top, 30);
        assert_eq!(config.struts.bottom, 40);
    }

    #[test]
    fn save_round_trips_through_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");

        let config = NiriLayoutConfig {
            focus_ring: FocusRingConfig {
                enabled: true,
                width: 3,
                active_color: "#aaa".to_string(),
                inactive_color: "#bbb".to_string(),
                urgent_color: Some("#ccc".to_string()),
            },
            border: BorderConfig {
                enabled: true,
                width: 2,
                active_color: "#111".to_string(),
                inactive_color: "#222".to_string(),
                urgent_color: None,
            },
            shadow: ShadowConfig {
                enabled: true,
                softness: 15,
                spread: 8,
                offset_x: 3,
                offset_y: -2,
                color: "#333".to_string(),
                inactive_color: Some("#444".to_string()),
            },
            tab_indicator: TabIndicatorConfig {
                enabled: true,
                position: "top".to_string(),
                gap: 2,
                width: 3,
                corner_radius: 6,
                active_color: "#555".to_string(),
                inactive_color: "#666".to_string(),
                urgent_color: Some("#777".to_string()),
            },
            gaps: 24,
            struts: StrutsConfig {
                left: 5,
                right: 10,
                top: 15,
                bottom: 20,
            },
        };

        save_layout_config(&path, &config).unwrap();
        let loaded = load_layout_config(&path).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn save_preserves_unrelated_layout_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(
            &dir,
            r#"
layout {
    preset-column-widths {
        proportion 0.33333
        proportion 0.5
        proportion 0.66667
    }
    default-column-width {
        proportion 0.5
    }
    center-focused-column "never"
    gaps 16
}
binds {
    Mod+Return {
        spawn "foot"
    }
}
"#,
        );

        let config = NiriLayoutConfig::default();
        save_layout_config(&path, &config).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("preset-column-widths"), "preset-column-widths lost");
        assert!(content.contains("default-column-width"), "default-column-width lost");
        assert!(content.contains("center-focused-column"), "center-focused-column lost");
        assert!(content.contains("binds"), "binds lost");
    }

    #[test]
    fn prefer_no_csd_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");

        // Initially absent
        assert!(!load_prefer_no_csd(&path).unwrap());

        // Enable
        save_prefer_no_csd(&path, true).unwrap();
        assert!(load_prefer_no_csd(&path).unwrap());

        // Disable
        save_prefer_no_csd(&path, false).unwrap();
        assert!(!load_prefer_no_csd(&path).unwrap());
    }

    #[test]
    fn prefer_no_csd_preserves_other_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "binds {\n}\n");

        save_prefer_no_csd(&path, true).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("binds"));
        assert!(content.contains("prefer-no-csd"));
    }

    #[test]
    fn background_color_set_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.kdl");

        // Initially absent
        assert_eq!(load_background_color(&path).unwrap(), None);

        // Set
        save_background_color(&path, Some("#ff5500")).unwrap();
        assert_eq!(
            load_background_color(&path).unwrap(),
            Some("#ff5500".to_string())
        );

        // Update
        save_background_color(&path, Some("#00ff00")).unwrap();
        assert_eq!(
            load_background_color(&path).unwrap(),
            Some("#00ff00".to_string())
        );

        // Remove
        save_background_color(&path, None).unwrap();
        assert_eq!(load_background_color(&path).unwrap(), None);
    }

    #[test]
    fn background_color_preserves_layout_siblings() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(
            &dir,
            r#"
layout {
    gaps 8
}
"#,
        );

        save_background_color(&path, Some("#123456")).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("gaps"), "gaps node lost");
        assert!(content.contains("#123456"));
    }

    #[test]
    fn load_defaults_for_missing_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(
            &dir,
            r#"
layout {
    gaps 32
}
"#,
        );

        let config = load_layout_config(&path).unwrap();
        assert_eq!(config.gaps, 32);
        // Everything else should be defaults
        assert_eq!(config.focus_ring, FocusRingConfig::default());
        assert_eq!(config.border, BorderConfig::default());
        assert_eq!(config.shadow, ShadowConfig::default());
        assert_eq!(config.tab_indicator, TabIndicatorConfig::default());
        assert_eq!(config.struts, StrutsConfig::default());
    }
}
