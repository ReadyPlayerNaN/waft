// Core type definitions for waft-ui-gtk declarative widget protocol
//
// These types form the vocabulary for describing UI widgets in a serializable,
// renderer-agnostic way. They represent the contract between plugins and the
// GTK renderer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The primary widget enum representing all supported widget types
#[derive(Serialize, Deserialize, Clone, Debug)]
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

    /// A layout container for organizing child widgets
    Container {
        orientation: Orientation,
        spacing: u32,
        css_classes: Vec<String>,
        children: Vec<Widget>,
    },

    /// A menu row with icon, labels, and optional trailing widget
    MenuRow {
        icon: Option<String>,
        label: String,
        sublabel: Option<String>,
        trailing: Option<Box<Widget>>, // Switch, Spinner, Checkmark
        sensitive: bool,
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
}

/// Represents a user action with parameters
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Action {
    pub id: String,
    pub params: ActionParams,
}

/// Parameters for an action
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ActionParams {
    None,
    Value(f64),
    String(String),
    Map(HashMap<String, serde_json::Value>),
}

/// Layout orientation for containers
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// UI slot where a widget should be placed
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Slot {
    FeatureToggles,
    Controls,
    Actions,
}

/// A named widget with placement metadata
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NamedWidget {
    pub id: String,
    pub slot: Slot,
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
    fn test_widget_container_serialization() {
        let widget = Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 8,
            css_classes: vec!["menu-container".to_string()],
            children: vec![
                Widget::Label {
                    text: "Header".to_string(),
                    css_classes: vec![],
                },
                Widget::Label {
                    text: "Footer".to_string(),
                    css_classes: vec![],
                },
            ],
        };

        let json = serde_json::to_string(&widget).unwrap();
        let deserialized: Widget = serde_json::from_str(&json).unwrap();

        match deserialized {
            Widget::Container {
                spacing, children, ..
            } => {
                assert_eq!(spacing, 8);
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected Widget::Container"),
        }
    }

    #[test]
    fn test_widget_menu_row_serialization() {
        let widget = Widget::MenuRow {
            icon: Some("settings".to_string()),
            label: "Settings".to_string(),
            sublabel: Some("Configure system".to_string()),
            trailing: Some(Box::new(Widget::Checkmark { visible: true })),
            sensitive: true,
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
            slot: Slot::FeatureToggles,
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
        match deserialized.slot {
            Slot::FeatureToggles => {}
            _ => panic!("Expected Slot::FeatureToggles"),
        }
    }

    #[test]
    fn test_widget_set_serialization() {
        let widget_set = WidgetSet {
            widgets: vec![
                NamedWidget {
                    id: "widget1".to_string(),
                    slot: Slot::Controls,
                    weight: 10,
                    widget: Widget::Label {
                        text: "Label 1".to_string(),
                        css_classes: vec![],
                    },
                },
                NamedWidget {
                    id: "widget2".to_string(),
                    slot: Slot::Actions,
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
            expanded_content: Some(Box::new(Widget::Container {
                orientation: Orientation::Vertical,
                spacing: 4,
                css_classes: vec![],
                children: vec![
                    Widget::MenuRow {
                        icon: None,
                        label: "Child 1".to_string(),
                        sublabel: None,
                        trailing: None,
                        sensitive: true,
                        on_click: None,
                    },
                    Widget::MenuRow {
                        icon: None,
                        label: "Child 2".to_string(),
                        sublabel: None,
                        trailing: Some(Box::new(Widget::Switch {
                            active: true,
                            sensitive: true,
                            on_toggle: Action {
                                id: "toggle_child".to_string(),
                                params: ActionParams::None,
                            },
                        })),
                        sensitive: true,
                        on_click: None,
                    },
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
                    Widget::Container { children, .. } => {
                        assert_eq!(children.len(), 2);
                    }
                    _ => panic!("Expected Container in expanded_content"),
                }
            }
            _ => panic!("Expected FeatureToggle"),
        }
    }

    #[test]
    fn test_orientation_values() {
        let horizontal = Orientation::Horizontal;
        let vertical = Orientation::Vertical;

        let h_json = serde_json::to_string(&horizontal).unwrap();
        let v_json = serde_json::to_string(&vertical).unwrap();

        let _: Orientation = serde_json::from_str(&h_json).unwrap();
        let _: Orientation = serde_json::from_str(&v_json).unwrap();
    }

    #[test]
    fn test_slot_values() {
        let slots = vec![Slot::FeatureToggles, Slot::Controls, Slot::Actions];

        for slot in slots {
            let json = serde_json::to_string(&slot).unwrap();
            let _: Slot = serde_json::from_str(&json).unwrap();
        }
    }
}
