//! Configuration types for notification sound and filter settings.
//!
//! Loaded from `~/.config/waft/config.toml` under the `plugin::notifications` entry.

use std::collections::HashMap;

use serde::Deserialize;
use waft_protocol::entity::notification_filter::{
    CombinatorOperator, GroupRule, MatchField, MatchOperator, NotificationGroup,
    NotificationProfile, Pattern, RuleCombinator, RuleNode,
};

/// Per-urgency sound theme names.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UrgencySounds {
    pub low: String,
    pub normal: String,
    pub critical: String,
}

impl Default for UrgencySounds {
    fn default() -> Self {
        Self {
            low: "message-new-instant".to_string(),
            normal: "message-new-email".to_string(),
            critical: "dialog-warning".to_string(),
        }
    }
}

/// A per-app sound rule.
///
/// Rules are evaluated top-to-bottom; first match wins.
/// An empty `sound` string means silent (no sound for this app).
#[derive(Debug, Clone, Deserialize)]
pub struct SoundRule {
    pub app_name: String,
    pub sound: String,
    /// Optional category filter (e.g. "im.received").
    /// If set, the rule only matches when the notification's category matches.
    #[serde(default)]
    pub category: Option<String>,
}

/// Top-level sound configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    /// Master toggle. When false, no sounds play.
    pub enabled: bool,
    /// Default sounds by urgency level.
    pub urgency: UrgencySounds,
    /// Per-app rules evaluated top-to-bottom.
    #[serde(default)]
    pub rules: Vec<SoundRule>,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            urgency: UrgencySounds::default(),
            rules: Vec::new(),
        }
    }
}

/// Load sound configuration from waft config.
///
/// Looks for the `sounds` table within the `plugin::notifications` plugin
/// settings. Returns default config if not found or if parsing fails.
pub fn load_sound_config() -> SoundConfig {
    let config = waft_config::Config::load();
    let Some(settings) = config.get_plugin_settings("plugin::notifications") else {
        log::debug!("[notifications/config] no plugin config found, using defaults");
        return SoundConfig::default();
    };

    let Some(sounds_value) = settings.get("sounds") else {
        log::debug!("[notifications/config] no sounds config found, using defaults");
        return SoundConfig::default();
    };

    match sounds_value.clone().try_into::<SoundConfig>() {
        Ok(sound_config) => {
            log::info!(
                "[notifications/config] loaded sound config: enabled={}, {} rules",
                sound_config.enabled,
                sound_config.rules.len()
            );
            sound_config
        }
        Err(e) => {
            log::warn!(
                "[notifications/config] failed to parse sounds config, using defaults: {e}"
            );
            SoundConfig::default()
        }
    }
}

// --- Filter configuration types ---

/// TOML representation of notification groups config.
#[derive(Debug, Clone, Deserialize)]
pub struct TomlGroup {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub matcher: TomlCombinator,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlCombinator {
    pub operator: CombinatorOperator,
    pub children: Vec<TomlNode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TomlNode {
    Pattern {
        field: MatchField,
        operator: MatchOperator,
        value: String,
    },
    Combinator {
        operator: CombinatorOperator,
        children: Vec<TomlNode>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub rules: HashMap<String, GroupRule>,
}

impl From<TomlGroup> for NotificationGroup {
    fn from(toml: TomlGroup) -> Self {
        Self {
            id: toml.id,
            name: toml.name,
            order: toml.order,
            matcher: toml.matcher.into(),
        }
    }
}

impl From<TomlCombinator> for RuleCombinator {
    fn from(toml: TomlCombinator) -> Self {
        Self {
            operator: toml.operator,
            children: toml.children.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<TomlNode> for RuleNode {
    fn from(toml: TomlNode) -> Self {
        match toml {
            TomlNode::Pattern {
                field,
                operator,
                value,
            } => RuleNode::Pattern(Pattern {
                field,
                operator,
                value,
            }),
            TomlNode::Combinator { operator, children } => {
                RuleNode::Combinator(RuleCombinator {
                    operator,
                    children: children.into_iter().map(Into::into).collect(),
                })
            }
        }
    }
}

impl From<TomlProfile> for NotificationProfile {
    fn from(toml: TomlProfile) -> Self {
        Self {
            id: toml.id,
            name: toml.name,
            rules: toml.rules,
        }
    }
}

/// Load notification groups and profiles from waft config.
pub fn load_filter_config() -> (Vec<NotificationGroup>, Vec<NotificationProfile>) {
    let config = waft_config::Config::load();
    let Some(settings) = config.get_plugin_settings("plugin::notifications") else {
        log::debug!("[notifications/config] no plugin config found, using empty groups/profiles");
        return (Vec::new(), Vec::new());
    };

    let groups: Vec<NotificationGroup> = settings
        .get("groups")
        .and_then(|v| v.clone().try_into::<Vec<TomlGroup>>().ok())
        .map(|groups| groups.into_iter().map(Into::into).collect())
        .unwrap_or_default();

    let profiles: Vec<NotificationProfile> = settings
        .get("profiles")
        .and_then(|v| v.clone().try_into::<Vec<TomlProfile>>().ok())
        .map(|profiles| profiles.into_iter().map(Into::into).collect())
        .unwrap_or_default();

    log::info!(
        "[notifications/config] loaded {} groups, {} profiles",
        groups.len(),
        profiles.len()
    );

    (groups, profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity::notification_filter::RuleValue;

    #[test]
    fn default_config_has_sounds_enabled() {
        let config = SoundConfig::default();
        assert!(config.enabled);
    }

    #[test]
    fn default_urgency_sounds() {
        let config = SoundConfig::default();
        assert_eq!(config.urgency.low, "message-new-instant");
        assert_eq!(config.urgency.normal, "message-new-email");
        assert_eq!(config.urgency.critical, "dialog-warning");
    }

    #[test]
    fn default_config_has_no_rules() {
        let config = SoundConfig::default();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
            enabled = true

            [urgency]
            low = "custom-low"
            normal = "custom-normal"
            critical = "custom-critical"

            [[rules]]
            app_name = "Spotify"
            sound = ""

            [[rules]]
            app_name = "Firefox"
            category = "im.received"
            sound = "message-new-instant"

            [[rules]]
            app_name = "Signal"
            sound = "phone-incoming-call"
        "#;

        let config: SoundConfig = toml::from_str(toml_str).expect("failed to parse");

        assert!(config.enabled);
        assert_eq!(config.urgency.low, "custom-low");
        assert_eq!(config.urgency.normal, "custom-normal");
        assert_eq!(config.urgency.critical, "custom-critical");
        assert_eq!(config.rules.len(), 3);

        assert_eq!(config.rules[0].app_name, "Spotify");
        assert_eq!(config.rules[0].sound, "");
        assert!(config.rules[0].category.is_none());

        assert_eq!(config.rules[1].app_name, "Firefox");
        assert_eq!(config.rules[1].sound, "message-new-instant");
        assert_eq!(
            config.rules[1].category.as_deref(),
            Some("im.received")
        );

        assert_eq!(config.rules[2].app_name, "Signal");
        assert_eq!(config.rules[2].sound, "phone-incoming-call");
        assert!(config.rules[2].category.is_none());
    }

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
            enabled = false
        "#;

        let config: SoundConfig = toml::from_str(toml_str).expect("failed to parse");
        assert!(!config.enabled);
        // Defaults for urgency
        assert_eq!(config.urgency.low, "message-new-instant");
        assert_eq!(config.urgency.normal, "message-new-email");
        assert_eq!(config.urgency.critical, "dialog-warning");
        assert!(config.rules.is_empty());
    }

    #[test]
    fn parse_empty_config_uses_defaults() {
        let toml_str = "";
        let config: SoundConfig = toml::from_str(toml_str).expect("failed to parse");
        assert!(config.enabled);
        assert_eq!(config.urgency.normal, "message-new-email");
    }

    #[test]
    fn parse_config_with_only_rules() {
        let toml_str = r#"
            [[rules]]
            app_name = "Slack"
            sound = "message-new-instant"
        "#;

        let config: SoundConfig = toml::from_str(toml_str).expect("failed to parse");
        assert!(config.enabled);
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].app_name, "Slack");
    }

    #[test]
    fn parse_config_with_only_urgency() {
        let toml_str = r#"
            [urgency]
            low = ""
            normal = "custom-normal"
            critical = "dialog-error"
        "#;

        let config: SoundConfig = toml::from_str(toml_str).expect("failed to parse");
        assert!(config.enabled);
        assert_eq!(config.urgency.low, "");
        assert_eq!(config.urgency.normal, "custom-normal");
        assert_eq!(config.urgency.critical, "dialog-error");
    }

    #[test]
    fn parse_config_partial_urgency_uses_defaults() {
        let toml_str = r#"
            [urgency]
            critical = "bell"
        "#;

        let config: SoundConfig = toml::from_str(toml_str).expect("failed to parse");
        assert_eq!(config.urgency.low, "message-new-instant");
        assert_eq!(config.urgency.normal, "message-new-email");
        assert_eq!(config.urgency.critical, "bell");
    }

    #[test]
    fn load_sound_config_returns_defaults_when_no_config_file() {
        // This tests the fallback path when no config file exists.
        // In a test environment, ~/.config/waft/config.toml likely doesn't exist.
        let config = load_sound_config();
        assert!(config.enabled);
        assert_eq!(config.urgency.normal, "message-new-email");
    }

    // --- Filter config tests ---

    #[test]
    fn load_filter_config_returns_empty_when_no_config_file() {
        let (groups, profiles) = load_filter_config();
        assert!(groups.is_empty());
        assert!(profiles.is_empty());
    }

    #[test]
    fn parse_toml_group_with_pattern() {
        let toml_str = r#"
            id = "test"
            name = "Test Group"
            order = 1

            [matcher]
            operator = "and"

            [[matcher.children]]
            type = "pattern"
            field = "app_name"
            operator = "contains"
            value = "slack"
        "#;

        let group: TomlGroup = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(group.id, "test");
        assert_eq!(group.name, "Test Group");
        assert_eq!(group.order, 1);

        let proto_group: NotificationGroup = group.into();
        assert_eq!(proto_group.matcher.children.len(), 1);
    }

    #[test]
    fn parse_toml_group_with_nested_combinator() {
        let toml_str = r#"
            id = "complex"
            name = "Complex Group"
            order = 2

            [matcher]
            operator = "and"

            [[matcher.children]]
            type = "pattern"
            field = "app_name"
            operator = "equals"
            value = "test"

            [[matcher.children]]
            type = "combinator"
            operator = "or"

            [[matcher.children.children]]
            type = "pattern"
            field = "urgency"
            operator = "equals"
            value = "critical"
        "#;

        let group: TomlGroup = toml::from_str(toml_str).expect("parse failed");
        let proto_group: NotificationGroup = group.into();

        assert_eq!(proto_group.matcher.children.len(), 2);
        assert!(matches!(
            proto_group.matcher.children[1],
            RuleNode::Combinator(_)
        ));
    }

    #[test]
    fn parse_toml_profile() {
        let toml_str = r#"
            id = "work"
            name = "Work"

            [rules.team-chats]
            hide = "off"
            no_toast = "on"
            no_sound = "default"
        "#;

        let profile: TomlProfile = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(profile.id, "work");
        assert_eq!(profile.name, "Work");
        assert_eq!(profile.rules.len(), 1);

        let rule = profile.rules.get("team-chats").unwrap();
        assert_eq!(rule.hide, RuleValue::Off);
        assert_eq!(rule.no_toast, RuleValue::On);
        assert_eq!(rule.no_sound, RuleValue::Default);
    }
}
