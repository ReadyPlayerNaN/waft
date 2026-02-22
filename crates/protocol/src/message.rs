//! Protocol messages for communication between apps, waft daemon, and plugins.
//!
//! Four message enums define the protocol:
//! - [`AppMessage`]: App -> waft daemon
//! - [`PluginMessage`]: Plugin -> waft daemon
//! - [`AppNotification`]: waft daemon -> App
//! - [`PluginCommand`]: waft daemon -> Plugin

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::description::PluginDescription;
use crate::urn::Urn;

/// Messages sent from an app to the waft daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum AppMessage {
    /// Subscribe to updates for an entity type.
    Subscribe { entity_type: String },

    /// Unsubscribe from updates for an entity type.
    Unsubscribe { entity_type: String },

    /// Request current state of all entities of a given type.
    Status { entity_type: String },

    /// Trigger an action on a specific entity.
    TriggerAction {
        urn: Urn,
        action: String,
        action_id: Uuid,
        params: serde_json::Value,
        timeout_ms: Option<u64>,
    },

    /// Request descriptions of a specific plugin, or all plugins.
    Describe {
        /// If set, describe only this plugin. If None, describe all.
        plugin_name: Option<String>,
    },

    /// Respond to a ClaimCheck: does this app still want the entity?
    ClaimResponse { claim_id: Uuid, claimed: bool },
}

/// Messages sent from a plugin to the waft daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum PluginMessage {
    /// An entity was created or updated.
    EntityUpdated {
        urn: Urn,
        entity_type: String,
        data: serde_json::Value,
    },

    /// An entity was removed.
    EntityRemoved { urn: Urn, entity_type: String },

    /// An action completed successfully.
    ActionSuccess { action_id: Uuid },

    /// An action failed.
    ActionError { action_id: Uuid, error: String },

    /// Response to a CanStop command.
    StopResponse { can_stop: bool },

    /// Request a claim check from app subscribers: do they still want this entity?
    ClaimCheck { urn: Urn, claim_id: Uuid },
}

/// Notifications sent from the waft daemon to an app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum AppNotification {
    /// An entity was created or updated.
    EntityUpdated {
        urn: Urn,
        entity_type: String,
        data: serde_json::Value,
    },

    /// An entity was removed.
    EntityRemoved { urn: Urn, entity_type: String },

    /// An action completed successfully.
    ActionSuccess { action_id: Uuid },

    /// An action failed.
    ActionError { action_id: Uuid, error: String },

    /// An entity's data may be stale (plugin unresponsive).
    EntityStale { urn: Urn, entity_type: String },

    /// An entity's data is outdated (plugin reconnecting).
    EntityOutdated { urn: Urn, entity_type: String },

    /// Response to a Describe request.
    DescribeResponse {
        plugins: Vec<PluginDescription>,
    },

    /// Ask whether the app still wants a specific entity (from daemon, originated by plugin).
    ClaimCheck { urn: Urn, claim_id: Uuid },
}

/// Commands sent from the waft daemon to a plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum PluginCommand {
    /// Ask the plugin if it can stop gracefully.
    CanStop,

    /// Trigger an action on a specific entity.
    TriggerAction {
        urn: Urn,
        action: String,
        action_id: Uuid,
        params: serde_json::Value,
    },

    /// Aggregated result of a claim check: whether any subscriber claimed the entity.
    ClaimResult { urn: Urn, claim_id: Uuid, claimed: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_json<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        msg: &T,
    ) {
        let json = serde_json::to_string(msg).expect("serialize");
        let decoded: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(msg, &decoded);
    }

    #[test]
    fn app_message_subscribe() {
        roundtrip_json(&AppMessage::Subscribe {
            entity_type: "audio-device".to_string(),
        });
    }

    #[test]
    fn app_message_unsubscribe() {
        roundtrip_json(&AppMessage::Unsubscribe {
            entity_type: "battery".to_string(),
        });
    }

    #[test]
    fn app_message_status() {
        roundtrip_json(&AppMessage::Status {
            entity_type: "clock".to_string(),
        });
    }

    #[test]
    fn app_message_trigger_action() {
        roundtrip_json(&AppMessage::TriggerAction {
            urn: Urn::new("audio", "audio-device", "speakers"),
            action: "set-volume".to_string(),
            action_id: Uuid::new_v4(),
            params: serde_json::json!({"volume": 0.5}),
            timeout_ms: Some(5000),
        });
    }

    #[test]
    fn app_message_trigger_action_no_timeout() {
        roundtrip_json(&AppMessage::TriggerAction {
            urn: Urn::new("darkman", "dark-mode", "default"),
            action: "toggle".to_string(),
            action_id: Uuid::new_v4(),
            params: serde_json::Value::Null,
            timeout_ms: None,
        });
    }

    #[test]
    fn plugin_message_entity_updated() {
        roundtrip_json(&PluginMessage::EntityUpdated {
            urn: Urn::new("battery", "battery", "BAT0"),
            entity_type: "battery".to_string(),
            data: serde_json::json!({"percentage": 85.0, "state": "Discharging"}),
        });
    }

    #[test]
    fn plugin_message_entity_removed() {
        roundtrip_json(&PluginMessage::EntityRemoved {
            urn: Urn::new("blueman", "bluetooth-adapter", "hci0"),
            entity_type: "bluetooth-adapter".to_string(),
        });
    }

    #[test]
    fn plugin_message_action_success() {
        roundtrip_json(&PluginMessage::ActionSuccess {
            action_id: Uuid::new_v4(),
        });
    }

    #[test]
    fn plugin_message_action_error() {
        roundtrip_json(&PluginMessage::ActionError {
            action_id: Uuid::new_v4(),
            error: "device not found".to_string(),
        });
    }

    #[test]
    fn plugin_message_stop_response() {
        roundtrip_json(&PluginMessage::StopResponse { can_stop: true });
        roundtrip_json(&PluginMessage::StopResponse { can_stop: false });
    }

    #[test]
    fn app_notification_entity_updated() {
        roundtrip_json(&AppNotification::EntityUpdated {
            urn: Urn::new("clock", "clock", "default"),
            entity_type: "clock".to_string(),
            data: serde_json::json!({"time": "14:30", "date": "Thursday, 12 Feb 2026"}),
        });
    }

    #[test]
    fn app_notification_entity_removed() {
        roundtrip_json(&AppNotification::EntityRemoved {
            urn: Urn::new("audio", "audio-device", "headphones"),
            entity_type: "audio-device".to_string(),
        });
    }

    #[test]
    fn app_notification_action_success() {
        roundtrip_json(&AppNotification::ActionSuccess {
            action_id: Uuid::new_v4(),
        });
    }

    #[test]
    fn app_notification_action_error() {
        roundtrip_json(&AppNotification::ActionError {
            action_id: Uuid::new_v4(),
            error: "permission denied".to_string(),
        });
    }

    #[test]
    fn app_notification_entity_stale() {
        roundtrip_json(&AppNotification::EntityStale {
            urn: Urn::new("weather", "weather", "default"),
            entity_type: "weather".to_string(),
        });
    }

    #[test]
    fn app_notification_entity_outdated() {
        roundtrip_json(&AppNotification::EntityOutdated {
            urn: Urn::new("networkmanager", "network-adapter", "wlan0"),
            entity_type: "network-adapter".to_string(),
        });
    }

    #[test]
    fn plugin_command_can_stop() {
        roundtrip_json(&PluginCommand::CanStop);
    }

    #[test]
    fn plugin_command_trigger_action() {
        roundtrip_json(&PluginCommand::TriggerAction {
            urn: Urn::new("caffeine", "sleep-inhibitor", "default"),
            action: "toggle".to_string(),
            action_id: Uuid::new_v4(),
            params: serde_json::Value::Null,
        });
    }

    #[test]
    fn app_message_describe_all() {
        roundtrip_json(&AppMessage::Describe {
            plugin_name: None,
        });
    }

    #[test]
    fn app_message_describe_specific() {
        roundtrip_json(&AppMessage::Describe {
            plugin_name: Some("audio".to_string()),
        });
    }

    #[test]
    fn app_notification_describe_response() {
        use crate::description::*;

        roundtrip_json(&AppNotification::DescribeResponse {
            plugins: vec![PluginDescription {
                name: "clock".to_string(),
                display_name: "Clock".to_string(),
                description: "Current time and date display".to_string(),
                entity_types: vec![EntityTypeDescription {
                    entity_type: "clock".to_string(),
                    display_name: "Clock".to_string(),
                    description: "Current time and date".to_string(),
                    properties: vec![PropertyDescription {
                        name: "time".to_string(),
                        label: "Time".to_string(),
                        description: "Formatted time string".to_string(),
                        value_type: PropertyValueType::String,
                    }],
                    actions: vec![],
                }],
            }],
        });
    }

    #[test]
    fn app_notification_describe_response_empty() {
        roundtrip_json(&AppNotification::DescribeResponse {
            plugins: vec![],
        });
    }

    #[test]
    fn plugin_message_claim_check() {
        roundtrip_json(&PluginMessage::ClaimCheck {
            urn: Urn::new("notifications", "notification", "42"),
            claim_id: Uuid::nil(),
        });
    }

    #[test]
    fn app_notification_claim_check() {
        roundtrip_json(&AppNotification::ClaimCheck {
            urn: Urn::new("notifications", "notification", "42"),
            claim_id: Uuid::nil(),
        });
    }

    #[test]
    fn app_message_claim_response_claimed() {
        roundtrip_json(&AppMessage::ClaimResponse {
            claim_id: Uuid::nil(),
            claimed: true,
        });
    }

    #[test]
    fn app_message_claim_response_not_claimed() {
        roundtrip_json(&AppMessage::ClaimResponse {
            claim_id: Uuid::nil(),
            claimed: false,
        });
    }

    #[test]
    fn plugin_command_claim_result() {
        roundtrip_json(&PluginCommand::ClaimResult {
            urn: Urn::new("notifications", "notification", "42"),
            claim_id: Uuid::nil(),
            claimed: false,
        });
    }
}
