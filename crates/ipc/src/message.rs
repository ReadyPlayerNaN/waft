//! Protocol message types for plugin-to-overview communication.
//!
//! This module defines the message protocol that will be used for IPC between
//! plugin daemons and the overview process in the future process isolation architecture.

use serde::{Deserialize, Serialize};
use super::widget::{Action, NamedWidget, Widget};

/// Protocol version for compatibility tracking
pub const PROTOCOL_VERSION: u32 = 1;

/// Messages from overview to plugin daemon
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum OverviewMessage {
    /// Request plugin's widget state
    GetWidgets,

    /// User triggered widget action
    TriggerAction {
        widget_id: String,
        action: Action,
    },
}

/// Messages from plugin daemon to overview
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum PluginMessage {
    /// Full widget set (replaces all)
    SetWidgets {
        widgets: Vec<NamedWidget>,
    },

    /// Update single widget
    UpdateWidget {
        id: String,
        widget: Widget,
    },

    /// Remove widget by ID
    RemoveWidget {
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{ActionParams, Slot};

    #[test]
    fn test_overview_message_get_widgets_serialization() {
        let msg = OverviewMessage::GetWidgets;
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: OverviewMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            OverviewMessage::GetWidgets => {},
            _ => panic!("Expected GetWidgets variant"),
        }
    }

    #[test]
    fn test_overview_message_trigger_action_serialization() {
        let msg = OverviewMessage::TriggerAction {
            widget_id: "test_widget".to_string(),
            action: Action {
                id: "test_action".to_string(),
                params: ActionParams::None,
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: OverviewMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            OverviewMessage::TriggerAction { widget_id, action } => {
                assert_eq!(widget_id, "test_widget");
                assert_eq!(action.id, "test_action");
            },
            _ => panic!("Expected TriggerAction variant"),
        }
    }

    #[test]
    fn test_plugin_message_set_widgets_serialization() {
        let msg = PluginMessage::SetWidgets {
            widgets: vec![
                NamedWidget {
                    id: "widget1".to_string(),
                    slot: Slot::Controls,
                    weight: 10,
                    widget: Widget::Label {
                        text: "Test".to_string(),
                        css_classes: vec![],
                    },
                },
            ],
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: PluginMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            PluginMessage::SetWidgets { widgets } => {
                assert_eq!(widgets.len(), 1);
                assert_eq!(widgets[0].id, "widget1");
            },
            _ => panic!("Expected SetWidgets variant"),
        }
    }

    #[test]
    fn test_plugin_message_update_widget_serialization() {
        let msg = PluginMessage::UpdateWidget {
            id: "widget1".to_string(),
            widget: Widget::Label {
                text: "Updated".to_string(),
                css_classes: vec![],
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: PluginMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            PluginMessage::UpdateWidget { id, .. } => {
                assert_eq!(id, "widget1");
            },
            _ => panic!("Expected UpdateWidget variant"),
        }
    }

    #[test]
    fn test_plugin_message_remove_widget_serialization() {
        let msg = PluginMessage::RemoveWidget {
            id: "widget1".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: PluginMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            PluginMessage::RemoveWidget { id } => {
                assert_eq!(id, "widget1");
            },
            _ => panic!("Expected RemoveWidget variant"),
        }
    }

    #[test]
    fn test_protocol_version_constant() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn test_message_json_format_has_type_tag() {
        let msg = OverviewMessage::GetWidgets;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\""));
    }

    #[test]
    fn test_trigger_action_with_value_params() {
        let msg = OverviewMessage::TriggerAction {
            widget_id: "slider1".to_string(),
            action: Action {
                id: "set_value".to_string(),
                params: ActionParams::Value(0.75),
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: OverviewMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            OverviewMessage::TriggerAction { action, .. } => {
                match action.params {
                    ActionParams::Value(v) => assert_eq!(v, 0.75),
                    _ => panic!("Expected Value params"),
                }
            },
            _ => panic!("Expected TriggerAction variant"),
        }
    }
}
