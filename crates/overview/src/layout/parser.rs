use anyhow::{Result, anyhow};

use crate::layout::model::LayoutNode;

pub const DEFAULT_LAYOUT: &str = include_str!("default.xml");

/// Load the layout from user config or fall back to the embedded default.
pub fn load_layout() -> Result<LayoutNode> {
    let config_path = dirs::config_dir().map(|d| d.join("waft/layout.xml"));
    let xml = match config_path {
        Some(ref path) if path.exists() => std::fs::read_to_string(path)?,
        _ => DEFAULT_LAYOUT.to_string(),
    };
    parse_layout(&xml)
}

/// Parse an XML string into a LayoutNode tree.
pub fn parse_layout(xml: &str) -> Result<LayoutNode> {
    let doc = roxmltree::Document::parse(xml)?;
    let root = doc.root_element();
    parse_node(&root)
}

fn parse_node(node: &roxmltree::Node) -> Result<LayoutNode> {
    match node.tag_name().name() {
        "Overview" => Ok(LayoutNode::Overview {
            children: parse_children(node)?,
        }),
        "Header" => Ok(LayoutNode::Header {
            children: parse_children(node)?,
        }),
        "TwoColumns" => Ok(LayoutNode::TwoColumns {
            children: parse_children(node)?,
        }),
        "Box" => Ok(LayoutNode::Box {
            halign: node.attribute("halign").map(|s| s.to_string()),
            children: parse_children(node)?,
        }),
        "Row" => Ok(LayoutNode::Row {
            halign: node.attribute("halign").map(|s| s.to_string()),
            children: parse_children(node)?,
        }),
        "Col" => Ok(LayoutNode::Col {
            halign: node.attribute("halign").map(|s| s.to_string()),
            children: parse_children(node)?,
        }),
        "Divider" => Ok(LayoutNode::Divider),
        "FeatureToggleGrid" => Ok(LayoutNode::FeatureToggleGrid {
            children: parse_children(node)?,
        }),
        "Widget" => {
            let id = node
                .attribute("id")
                .ok_or_else(|| anyhow!("Widget element missing 'id' attribute"))?;
            Ok(LayoutNode::Widget { id: id.to_string() })
        }
        "RightColumnStack" => {
            let mut controls = Vec::new();
            let mut exit = Vec::new();
            for child in node.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "Controls" => controls = parse_children(&child)?,
                    "Exit" => exit = parse_children(&child)?,
                    tag => return Err(anyhow!("Unknown RightColumnStack child tag: {}", tag)),
                }
            }
            Ok(LayoutNode::RightColumnStack { controls, exit })
        }
        "ControlsTabButton" => Ok(LayoutNode::ControlsTabButton),
        "ExitTabButton" => Ok(LayoutNode::ExitTabButton),
        "Unmatched" => Ok(LayoutNode::Unmatched),
        tag @ ("Clock" | "Battery" | "Weather" | "KeyboardLayout" | "SettingsButton"
        | "SessionActions" | "SystemActions" | "Calendar" | "Agenda" | "Events"
        | "NotificationList" | "AudioSliders" | "BrightnessSliders" | "DndToggle"
        | "CaffeineToggle" | "DarkModeToggle" | "NightLightToggle" | "BluetoothToggles"
        | "WifiToggles" | "WiredToggles" | "VpnToggles" | "TetheringToggles"
        | "BackupToggle") => Ok(LayoutNode::Component {
            name: tag.to_string(),
        }),
        tag => Err(anyhow!("Unknown layout tag: {}", tag)),
    }
}

fn parse_children(node: &roxmltree::Node) -> Result<Vec<LayoutNode>> {
    node.children()
        .filter(|n| n.is_element())
        .map(|n| parse_node(&n))
        .collect()
}

/// Simple glob matching with suffix wildcard.
///
/// `brightness:*` matches `brightness:control`, `brightness:intel_backlight`, etc.
/// `clock:main` matches only `clock:main`.
#[allow(dead_code)]
pub fn glob_match(text: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        text.starts_with(prefix)
    } else {
        text == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_exact() {
        assert!(glob_match("clock:main", "clock:main"));
        assert!(!glob_match("clock:main", "clock:other"));
    }

    #[test]
    fn glob_match_wildcard() {
        assert!(glob_match("brightness:control", "brightness:*"));
        assert!(glob_match("brightness:intel_backlight", "brightness:*"));
        assert!(!glob_match("audio:volume", "brightness:*"));
    }

    #[test]
    fn glob_match_bare_star() {
        assert!(glob_match("anything", "*"));
        assert!(glob_match("", "*"));
    }

    #[test]
    fn glob_match_empty_pattern() {
        assert!(glob_match("", ""));
        assert!(!glob_match("something", ""));
    }

    #[test]
    fn parse_default_layout() {
        let root = parse_layout(DEFAULT_LAYOUT).expect("default layout should parse");
        match &root {
            LayoutNode::Overview { children } => {
                assert_eq!(
                    children.len(),
                    3,
                    "Overview should have Header, Divider, TwoColumns"
                );
                assert!(matches!(&children[0], LayoutNode::Header { .. }));
                assert!(matches!(&children[1], LayoutNode::Divider));
                assert!(matches!(&children[2], LayoutNode::TwoColumns { .. }));

                // Header should contain Rows with Component children
                if let LayoutNode::Header {
                    children: header_children,
                } = &children[0]
                {
                    assert_eq!(header_children.len(), 2);
                    if let LayoutNode::Row { children, .. } = &header_children[0] {
                        assert_eq!(children.len(), 3);
                        assert!(
                            matches!(&children[0], LayoutNode::Component { name } if name == "Clock")
                        );
                        assert!(
                            matches!(&children[1], LayoutNode::Component { name } if name == "Battery")
                        );
                        assert!(
                            matches!(&children[2], LayoutNode::Component { name } if name == "Weather")
                        );
                    }
                    if let LayoutNode::Row { halign, children } = &header_children[1] {
                        assert_eq!(halign.as_deref(), Some("end"));
                        assert_eq!(children.len(), 4);
                        assert!(
                            matches!(&children[0], LayoutNode::Component { name } if name == "KeyboardLayout")
                        );
                        assert!(
                            matches!(&children[1], LayoutNode::Component { name } if name == "SettingsButton")
                        );
                        assert!(matches!(&children[2], LayoutNode::ControlsTabButton));
                        assert!(matches!(&children[3], LayoutNode::ExitTabButton));
                    }
                }

                // TwoColumns should contain Cols
                if let LayoutNode::TwoColumns {
                    children: col_children,
                } = &children[2]
                {
                    assert_eq!(col_children.len(), 2);
                    // Left column: Events, NotificationList
                    if let LayoutNode::Col { children, .. } = &col_children[0] {
                        assert_eq!(children.len(), 2);
                        assert!(
                            matches!(&children[0], LayoutNode::Component { name } if name == "Events")
                        );
                        assert!(
                            matches!(&children[1], LayoutNode::Component { name } if name == "NotificationList")
                        );
                    }
                    // Right column: RightColumnStack inside a Col
                    if let LayoutNode::Col { children, .. } = &col_children[1] {
                        assert_eq!(children.len(), 1);
                        if let LayoutNode::RightColumnStack { controls, exit } = &children[0] {
                            // Controls page: AudioSliders, BrightnessSliders, FeatureToggleGrid, Unmatched
                            assert_eq!(controls.len(), 4);
                            assert!(
                                matches!(&controls[0], LayoutNode::Component { name } if name == "AudioSliders")
                            );
                            assert!(
                                matches!(&controls[1], LayoutNode::Component { name } if name == "BrightnessSliders")
                            );
                            assert!(matches!(&controls[2], LayoutNode::FeatureToggleGrid { .. }));
                            assert!(matches!(&controls[3], LayoutNode::Unmatched));

                            // FeatureToggleGrid children
                            if let LayoutNode::FeatureToggleGrid {
                                children: toggle_children,
                            } = &controls[2]
                            {
                                assert_eq!(toggle_children.len(), 10);
                                assert!(
                                    matches!(&toggle_children[0], LayoutNode::Component { name } if name == "DndToggle")
                                );
                                assert!(
                                    matches!(&toggle_children[5], LayoutNode::Component { name } if name == "WifiToggles")
                                );
                                assert!(
                                    matches!(&toggle_children[6], LayoutNode::Component { name } if name == "WiredToggles")
                                );
                                assert!(
                                    matches!(&toggle_children[7], LayoutNode::Component { name } if name == "VpnToggles")
                                );
                                assert!(
                                    matches!(&toggle_children[8], LayoutNode::Component { name } if name == "TetheringToggles")
                                );
                                assert!(
                                    matches!(&toggle_children[9], LayoutNode::Component { name } if name == "BackupToggle")
                                );
                            }

                            // Exit page: SessionActions, SystemActions
                            assert_eq!(exit.len(), 2);
                            assert!(
                                matches!(&exit[0], LayoutNode::Component { name } if name == "SessionActions")
                            );
                            assert!(
                                matches!(&exit[1], LayoutNode::Component { name } if name == "SystemActions")
                            );
                        } else {
                            panic!("expected RightColumnStack in right column");
                        }
                    }
                }
            }
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_custom_xml() {
        let xml = r#"<Overview><Divider /><Divider /></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => {
                assert_eq!(children.len(), 2);
                assert!(matches!(&children[0], LayoutNode::Divider));
                assert!(matches!(&children[1], LayoutNode::Divider));
            }
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_box_with_halign() {
        let xml = r#"<Overview><Box halign="end"><Widget id="test:a" /></Box></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => match &children[0] {
                LayoutNode::Box { halign, children } => {
                    assert_eq!(halign.as_deref(), Some("end"));
                    assert_eq!(children.len(), 1);
                }
                _ => panic!("expected Box"),
            },
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_box_without_halign() {
        let xml = r#"<Overview><Box><Widget id="test:a" /></Box></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => match &children[0] {
                LayoutNode::Box { halign, .. } => {
                    assert!(halign.is_none());
                }
                _ => panic!("expected Box"),
            },
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_row() {
        let xml = r#"<Overview><Row><Widget id="a:b" /><Widget id="c:d" /></Row></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => match &children[0] {
                LayoutNode::Row { halign, children } => {
                    assert!(halign.is_none());
                    assert_eq!(children.len(), 2);
                }
                _ => panic!("expected Row"),
            },
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_row_with_halign() {
        let xml = r#"<Overview><Row halign="end"><Widget id="a:b" /></Row></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => match &children[0] {
                LayoutNode::Row { halign, children } => {
                    assert_eq!(halign.as_deref(), Some("end"));
                    assert_eq!(children.len(), 1);
                }
                _ => panic!("expected Row"),
            },
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_col() {
        let xml = r#"<Overview><Col><Widget id="a:b" /><Widget id="c:d" /></Col></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => match &children[0] {
                LayoutNode::Col { halign, children } => {
                    assert!(halign.is_none());
                    assert_eq!(children.len(), 2);
                }
                _ => panic!("expected Col"),
            },
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_error_on_unknown_tag() {
        let xml = r#"<Overview><FooBar /></Overview>"#;
        let err = parse_layout(xml).unwrap_err();
        assert!(
            err.to_string().contains("Unknown layout tag: FooBar"),
            "error was: {}",
            err
        );
    }

    #[test]
    fn parse_error_on_widget_without_id() {
        let xml = r#"<Overview><Widget /></Overview>"#;
        let err = parse_layout(xml).unwrap_err();
        assert!(
            err.to_string().contains("missing 'id' attribute"),
            "error was: {}",
            err
        );
    }

    #[test]
    fn parse_feature_toggle_grid() {
        let xml = r#"<Overview><FeatureToggleGrid><Widget id="a:b" /><Widget id="c:d" /></FeatureToggleGrid></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => match &children[0] {
                LayoutNode::FeatureToggleGrid { children } => {
                    assert_eq!(children.len(), 2);
                }
                _ => panic!("expected FeatureToggleGrid"),
            },
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_feature_toggle_grid_with_components() {
        let xml = r#"<Overview><FeatureToggleGrid><DndToggle /><CaffeineToggle /><BluetoothToggles /></FeatureToggleGrid></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => match &children[0] {
                LayoutNode::FeatureToggleGrid { children } => {
                    assert_eq!(children.len(), 3);
                    assert!(
                        matches!(&children[0], LayoutNode::Component { name } if name == "DndToggle")
                    );
                    assert!(
                        matches!(&children[1], LayoutNode::Component { name } if name == "CaffeineToggle")
                    );
                    assert!(
                        matches!(&children[2], LayoutNode::Component { name } if name == "BluetoothToggles")
                    );
                }
                _ => panic!("expected FeatureToggleGrid"),
            },
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_component() {
        let xml = r#"<Overview><Clock /></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => {
                assert_eq!(children.len(), 1);
                match &children[0] {
                    LayoutNode::Component { name } => assert_eq!(name, "Clock"),
                    _ => panic!("expected Component"),
                }
            }
            _ => panic!("root should be Overview"),
        }
    }

    #[test]
    fn parse_unmatched() {
        let xml = r#"<Overview><Unmatched /></Overview>"#;
        let root = parse_layout(xml).expect("should parse");
        match root {
            LayoutNode::Overview { children } => {
                assert!(matches!(&children[0], LayoutNode::Unmatched));
            }
            _ => panic!("root should be Overview"),
        }
    }
}
