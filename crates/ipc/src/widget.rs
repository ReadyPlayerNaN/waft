// Core widget protocol types for waft IPC
//
// These types form the vocabulary for describing UI widgets in a serializable,
// renderer-agnostic way. They represent the contract between plugins and the
// GTK renderer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The primary widget enum representing all supported widget types
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Widget {
    /// A toggleable feature card with optional expanded content
    FeatureToggle {
        title: String,
        icon: String,
        details: Option<String>,
        active: bool,
        busy: bool,
        expandable: bool,
        expanded_content: Option<Box<Widget>>,
        on_toggle: Action,
    },

    /// A slider control with icon and optional expanded content
    Slider {
        icon: String,
        value: f64,
        muted: bool, // Semantic state, renderer picks icon
        expandable: bool,
        expanded_content: Option<Box<Widget>>,
        on_value_change: Action,
        on_icon_click: Action,
    },

    /// A menu row with icon, label, and optional trailing widget
    MenuRow {
        icon: Option<String>,
        label: String,
        trailing: Option<Box<Widget>>, // Switch, Spinner, Checkmark
        sensitive: bool,
        busy: bool,
        on_click: Option<Action>,
    },

    /// A simple toggle switch
    Switch {
        active: bool,
        sensitive: bool,
        on_toggle: Action,
    },

    /// A loading spinner
    Spinner { spinning: bool },

    /// A checkmark indicator
    Checkmark { visible: bool },

    /// A clickable button
    Button {
        label: Option<String>,
        icon: Option<String>,
        on_click: Action,
    },

    /// A text label
    Label {
        text: String,
        css_classes: Vec<String>,
    },

    /// A horizontal layout container (shorthand for Container with Horizontal orientation)
    Row {
        spacing: u32,
        css_classes: Vec<String>,
        children: Vec<Node>,
    },

    /// A vertical layout container (shorthand for Container with Vertical orientation)
    Col {
        spacing: u32,
        css_classes: Vec<String>,
        children: Vec<Node>,
    },

    /// A button that cycles through a list of options
    StatusCycleButton {
        value: String,
        icon: String,
        options: Vec<StatusOption>,
        on_cycle: Action,
    },

    /// A horizontal row of child widgets with CSS classes
    ListRow {
        children: Vec<Node>,
        css_classes: Vec<String>,
    },

    /// A list with a leading icon and child widgets
    IconList {
        icon: String,
        icon_size: i32,
        children: Vec<Node>,
    },

    /// A button styled for use in lists
    ListButton {
        label: String,
        icon: Option<String>,
        css_classes: Vec<String>,
        on_click: Action,
    },

    /// A card with icon, title, optional description, and optional click action
    InfoCard {
        icon: String,
        title: String,
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        on_click: Option<Action>,
    },
}

/// An option for StatusCycleButton
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StatusOption {
    pub id: String,
    pub label: String,
}

/// Represents a user action with parameters
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Action {
    pub id: String,
    pub params: ActionParams,
}

/// Parameters for an action
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ActionParams {
    None,
    Value(f64),
    String(String),
    Map(HashMap<String, serde_json::Value>),
}

/// A widget tree node with an optional key for reconciliation.
///
/// Wraps a `Widget` with an optional string key, similar to React keys.
/// Keys enable the GTK reconciler to match children across updates and
/// update widgets in-place instead of recreating them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub key: Option<String>,
    pub widget: Widget,
}

impl Node {
    /// Create a keyed node.
    pub fn keyed(key: impl Into<String>, widget: Widget) -> Self {
        Node {
            key: Some(key.into()),
            widget,
        }
    }
}

impl From<Widget> for Node {
    fn from(widget: Widget) -> Self {
        Node { key: None, widget }
    }
}

/// A named widget with placement metadata.
///
/// The layout XML decides where widgets appear based on their `id`.
/// The `weight` field controls ordering within a layout slot.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NamedWidget {
    pub id: String,
    pub weight: u32,
    pub widget: Widget,
}

/// A set of widgets from a plugin
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WidgetSet {
    pub widgets: Vec<NamedWidget>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_serialization_none() {
        let action = Action {
            id: "toggle_power".to_string(),
            params: ActionParams::None,
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        assert_eq!(action.id, deserialized.id);
        match deserialized.params {
            ActionParams::None => {}
            _ => panic!("Expected ActionParams::None"),
        }
    }

    #[test]
    fn test_action_serialization_value() {
        let action = Action {
            id: "set_volume".to_string(),
            params: ActionParams::Value(0.75),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        assert_eq!(action.id, deserialized.id);
        match deserialized.params {
            ActionParams::Value(v) => assert_eq!(v, 0.75),
            _ => panic!("Expected ActionParams::Value"),
        }
    }

    #[test]
    fn test_action_serialization_string() {
        let action = Action {
            id: "select_device".to_string(),
            params: ActionParams::String("device_123".to_string()),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        assert_eq!(action.id, deserialized.id);
        match deserialized.params {
            ActionParams::String(s) => assert_eq!(s, "device_123"),
            _ => panic!("Expected ActionParams::String"),
        }
    }

    #[test]
    fn test_action_serialization_map() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), serde_json::json!("value"));

        let action = Action {
            id: "complex_action".to_string(),
            params: ActionParams::Map(map.clone()),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        assert_eq!(action.id, deserialized.id);
        match deserialized.params {
            ActionParams::Map(m) => {
                assert_eq!(m.get("key").unwrap(), &serde_json::json!("value"));
            }
            _ => panic!("Expected ActionParams::Map"),
        }
    }

    #[test]
    fn test_widget_feature_toggle_serialization() {
        let widget = Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-active".to_string(),
            details: Some("Connected to 2 devices".to_string()),
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(Widget::Label {
                text: "Devices".to_string(),
                css_classes: vec![],
            })),
            on_toggle: Action {
                id: "toggle_bluetooth".to_string(),
                params: ActionParams::None,
            },
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::FeatureToggle {
                title,
                active,
                expandable,
                ..
            } => {
                assert_eq!(title, "Bluetooth");
                assert!(active);
                assert!(expandable);
            }
            _ => panic!("Expected Widget::FeatureToggle"),
        }
    }

    #[test]
    fn test_widget_slider_serialization() {
        let widget = Widget::Slider {
            icon: "volume-high".to_string(),
            value: 0.65,
            muted: false,
            expandable: false,
            expanded_content: None,
            on_value_change: Action {
                id: "set_volume".to_string(),
                params: ActionParams::Value(0.65),
            },
            on_icon_click: Action {
                id: "toggle_mute".to_string(),
                params: ActionParams::None,
            },
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::Slider { value, muted, .. } => {
                assert_eq!(value, 0.65);
                assert!(!muted);
            }
            _ => panic!("Expected Widget::Slider"),
        }
    }

    #[test]
    fn test_widget_col_serialization() {
        let widget = Widget::Col {
            spacing: 8,
            css_classes: vec!["menu-container".to_string()],
            children: vec![
                Widget::Label {
                    text: "Header".to_string(),
                    css_classes: vec![],
                }
                .into(),
                Widget::Label {
                    text: "Footer".to_string(),
                    css_classes: vec![],
                }
                .into(),
            ],
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::Col {
                spacing, children, ..
            } => {
                assert_eq!(spacing, 8);
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected Widget::Col"),
        }
    }

    #[test]
    fn test_widget_menu_row_serialization() {
        let widget = Widget::MenuRow {
            icon: Some("settings".to_string()),
            label: "Settings".to_string(),
            trailing: Some(Box::new(Widget::Checkmark { visible: true })),
            sensitive: true,
            busy: false,
            on_click: Some(Action {
                id: "open_settings".to_string(),
                params: ActionParams::None,
            }),
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::MenuRow {
                label, sensitive, ..
            } => {
                assert_eq!(label, "Settings");
                assert!(sensitive);
            }
            _ => panic!("Expected Widget::MenuRow"),
        }
    }

    #[test]
    fn test_widget_primitives_serialization() {
        let widgets = vec![
            Widget::Switch {
                active: true,
                sensitive: true,
                on_toggle: Action {
                    id: "toggle".to_string(),
                    params: ActionParams::None,
                },
            },
            Widget::Spinner { spinning: true },
            Widget::Checkmark { visible: false },
            Widget::Button {
                label: Some("Click me".to_string()),
                icon: None,
                on_click: Action {
                    id: "button_click".to_string(),
                    params: ActionParams::None,
                },
            },
            Widget::Label {
                text: "Hello".to_string(),
                css_classes: vec!["bold".to_string()],
            },
        ];

        for widget in widgets {
            let json = serde_json::to_string(&widget).unwrap();
            let _: Widget = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_named_widget_serialization() {
        let named_widget = NamedWidget {
            id: "bluetooth:adapter0".to_string(),
            weight: 100,
            widget: Widget::Label {
                text: "Test".to_string(),
                css_classes: vec![],
            },
        };

        let json = serde_json::to_string(&named_widget).unwrap();
        let deserialized: NamedWidget = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "bluetooth:adapter0");
        assert_eq!(deserialized.weight, 100);
    }

    #[test]
    fn test_widget_set_serialization() {
        let widget_set = WidgetSet {
            widgets: vec![
                NamedWidget {
                    id: "widget1".to_string(),
                    weight: 10,
                    widget: Widget::Label {
                        text: "Label 1".to_string(),
                        css_classes: vec![],
                    },
                },
                NamedWidget {
                    id: "widget2".to_string(),
                    weight: 20,
                    widget: Widget::Button {
                        label: Some("Button".to_string()),
                        icon: None,
                        on_click: Action {
                            id: "click".to_string(),
                            params: ActionParams::None,
                        },
                    },
                },
            ],
        };

        let json = serde_json::to_string(&widget_set).unwrap();
        let deserialized: WidgetSet = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.widgets.len(), 2);
        assert_eq!(deserialized.widgets[0].id, "widget1");
        assert_eq!(deserialized.widgets[1].id, "widget2");
    }

    #[test]
    fn test_recursive_widget_structure() {
        let widget = Widget::FeatureToggle {
            title: "Parent".to_string(),
            icon: "icon".to_string(),
            details: None,
            active: false,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(Widget::Col {
                spacing: 4,
                css_classes: vec![],
                children: vec![
                    Widget::MenuRow {
                        icon: None,
                        label: "Child 1".to_string(),
                        trailing: None,
                        sensitive: true,
                        busy: false,
                        on_click: None,
                    }
                    .into(),
                    Widget::MenuRow {
                        icon: None,
                        label: "Child 2".to_string(),
                        trailing: Some(Box::new(Widget::Switch {
                            active: true,
                            sensitive: true,
                            on_toggle: Action {
                                id: "toggle_child".to_string(),
                                params: ActionParams::None,
                            },
                        })),
                        sensitive: true,
                        busy: false,
                        on_click: None,
                    }
                    .into(),
                ],
            })),
            on_toggle: Action {
                id: "toggle_parent".to_string(),
                params: ActionParams::None,
            },
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::FeatureToggle {
                expanded_content, ..
            } => {
                assert!(expanded_content.is_some());
                match *expanded_content.unwrap() {
                    Widget::Col { children, .. } => {
                        assert_eq!(children.len(), 2);
                    }
                    _ => panic!("Expected Col in expanded_content"),
                }
            }
            _ => panic!("Expected FeatureToggle"),
        }
    }

    #[test]
    fn test_node_from_widget() {
        let widget = Widget::Label {
            text: "test".to_string(),
            css_classes: vec![],
        };
        let node = Node::from(widget.clone());
        assert_eq!(node.key, None);
        assert_eq!(node.widget, widget);
    }

    #[test]
    fn test_node_keyed() {
        let widget = Widget::Label {
            text: "test".to_string(),
            css_classes: vec![],
        };
        let node = Node::keyed("my-key", widget.clone());
        assert_eq!(node.key, Some("my-key".to_string()));
        assert_eq!(node.widget, widget);
    }

    #[test]
    fn test_node_serialization_without_key() {
        let node = Node::from(Widget::Label {
            text: "test".to_string(),
            css_classes: vec![],
        });

        let json = serde_json::to_string(&node).unwrap();
        // Key should be omitted when None
        assert!(!json.contains("\"key\""));

        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.key, None);
    }

    #[test]
    fn test_node_serialization_with_key() {
        let node = Node::keyed(
            "device-1",
            Widget::Label {
                text: "test".to_string(),
                css_classes: vec![],
            },
        );

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"key\""));
        assert!(json.contains("device-1"));

        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.key, Some("device-1".to_string()));
    }

    #[test]
    fn test_widget_info_card_serialization() {
        let widget = Widget::InfoCard {
            icon: "weather-clear-symbolic".to_string(),
            title: "Sunny".to_string(),
            description: Some("25°C, clear skies".to_string()),
            on_click: None,
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::InfoCard {
                icon,
                title,
                description,
                ..
            } => {
                assert_eq!(icon, "weather-clear-symbolic");
                assert_eq!(title, "Sunny");
                assert_eq!(description, Some("25°C, clear skies".to_string()));
            }
            _ => panic!("Expected Widget::InfoCard"),
        }
    }

    #[test]
    fn test_widget_info_card_no_description() {
        let widget = Widget::InfoCard {
            icon: "battery-full-symbolic".to_string(),
            title: "Battery Full".to_string(),
            description: None,
            on_click: None,
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::InfoCard {
                title, description, ..
            } => {
                assert_eq!(title, "Battery Full");
                assert!(description.is_none());
            }
            _ => panic!("Expected Widget::InfoCard"),
        }
    }

}
