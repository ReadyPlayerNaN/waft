//! Notification filtering entity types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const NOTIFICATION_GROUP_ENTITY_TYPE: &str = "notification-group";
pub const NOTIFICATION_PROFILE_ENTITY_TYPE: &str = "notification-profile";
pub const ACTIVE_PROFILE_ENTITY_TYPE: &str = "active-profile";

/// A pattern-based notification group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationGroup {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub matcher: RuleCombinator,
}

/// Nested combinator for boolean logic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleCombinator {
    pub operator: CombinatorOperator,
    pub children: Vec<RuleNode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CombinatorOperator {
    And,
    Or,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RuleNode {
    Pattern(Pattern),
    Combinator(RuleCombinator),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pattern {
    pub field: MatchField,
    pub operator: MatchOperator,
    pub value: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchField {
    AppName,
    AppId,
    Title,
    Body,
    Category,
    Urgency,
    Workspace,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchOperator {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    StartsWith,
    NotStartsWith,
    EndsWith,
    NotEndsWith,
    MatchesRegex,
    NotMatchesRegex,
}

/// A profile with rules for groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub rules: HashMap<String, GroupRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupRule {
    pub hide: RuleValue,
    pub no_toast: RuleValue,
    pub no_sound: RuleValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleValue {
    On,
    Off,
    Default,
}

pub const SOUND_CONFIG_ENTITY_TYPE: &str = "sound-config";

/// Sound configuration exposed as an entity for the settings UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundConfigEntity {
    pub enabled: bool,
    pub default_low: String,
    pub default_normal: String,
    pub default_critical: String,
}

/// Active profile tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveProfile {
    pub profile_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_notification_group() {
        let group = NotificationGroup {
            id: "test".to_string(),
            name: "Test Group".to_string(),
            order: 1,
            matcher: RuleCombinator {
                operator: CombinatorOperator::And,
                children: vec![
                    RuleNode::Pattern(Pattern {
                        field: MatchField::AppName,
                        operator: MatchOperator::Contains,
                        value: "slack".to_string(),
                    }),
                ],
            },
        };

        let json = serde_json::to_string(&group).unwrap();
        let deserialized: NotificationGroup = serde_json::from_str(&json).unwrap();
        assert_eq!(group, deserialized);
    }

    #[test]
    fn serialize_nested_combinator() {
        let combinator = RuleCombinator {
            operator: CombinatorOperator::And,
            children: vec![
                RuleNode::Pattern(Pattern {
                    field: MatchField::AppName,
                    operator: MatchOperator::Equals,
                    value: "test".to_string(),
                }),
                RuleNode::Combinator(RuleCombinator {
                    operator: CombinatorOperator::Or,
                    children: vec![
                        RuleNode::Pattern(Pattern {
                            field: MatchField::Urgency,
                            operator: MatchOperator::Equals,
                            value: "critical".to_string(),
                        }),
                    ],
                }),
            ],
        };

        let json = serde_json::to_string(&combinator).unwrap();
        let deserialized: RuleCombinator = serde_json::from_str(&json).unwrap();
        assert_eq!(combinator, deserialized);
    }

    #[test]
    fn serialize_notification_profile() {
        let mut rules = HashMap::new();
        rules.insert(
            "group1".to_string(),
            GroupRule {
                hide: RuleValue::Off,
                no_toast: RuleValue::On,
                no_sound: RuleValue::Default,
                sound: Some("bell".to_string()),
            },
        );

        let profile = NotificationProfile {
            id: "work".to_string(),
            name: "Work".to_string(),
            rules,
        };

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: NotificationProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, deserialized);
    }

    #[test]
    fn group_rule_without_sound_deserializes_to_none() {
        let json = serde_json::json!({
            "hide": "off",
            "no_toast": "on",
            "no_sound": "default"
        });
        let rule: GroupRule = serde_json::from_value(json).unwrap();
        assert_eq!(rule.sound, None);
    }

    #[test]
    fn sound_config_entity_serde_roundtrip() {
        let entity = SoundConfigEntity {
            enabled: true,
            default_low: "message-new-instant".to_string(),
            default_normal: "message-new-email".to_string(),
            default_critical: "dialog-warning".to_string(),
        };
        let json = serde_json::to_value(&entity).unwrap();
        let decoded: SoundConfigEntity = serde_json::from_value(json).unwrap();
        assert_eq!(entity, decoded);
    }
}
