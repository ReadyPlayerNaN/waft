/// Declarative layout tree node.
///
/// The overlay layout is described as a tree of nodes parsed from XML.
/// A default layout ships embedded in the binary. Users can override
/// it via `~/.config/waft/layout.xml`.
#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// Root container (vertical box, spacing 12)
    Overview { children: Vec<LayoutNode> },
    /// Header row (horizontal box, spacing 16, hexpand)
    Header { children: Vec<LayoutNode> },
    /// Two-column layout (horizontal box, spacing 24, two 480px children + spacer)
    TwoColumns { children: Vec<LayoutNode> },
    /// Layout-neutral box (vertical by default, spacing 12, optional halign)
    Box { halign: Option<String>, children: Vec<LayoutNode> },
    /// Horizontal layout box (spacing 12, optional halign)
    Row { halign: Option<String>, children: Vec<LayoutNode> },
    /// Vertical layout box (spacing 12, optional halign)
    Col { halign: Option<String>, children: Vec<LayoutNode> },
    /// Horizontal separator
    Divider,
    /// Groups child Widget patterns into a feature toggle grid
    FeatureToggleGrid { children: Vec<LayoutNode> },
    /// Widget placeholder, matches by ID pattern (supports `*` suffix wildcard)
    Widget { id: String },
    /// Catch-all for widgets not matched by any pattern
    Unmatched,
}
