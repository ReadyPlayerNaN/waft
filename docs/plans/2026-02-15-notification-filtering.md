# Notification Pattern Matching and Filtering Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a two-layer notification filtering system with pattern-based groups and configurable profiles, fully integrated with entity-based architecture.

**Architecture:** Configuration lives as entities (`notification-group`, `notification-profile`, `active-profile`) with bidirectional TOML sync. Pattern matching engine evaluates notifications against groups using nested AND/OR combinators. Settings UI provides full CRUD for groups and profiles.

**Tech Stack:** Rust, waft-protocol entities, serde/toml, regex crate, GTK4/libadwaita UI, entity-based daemon architecture.

---

## Task 1: Define Entity Types in Protocol

**Files:**

- Create: `crates/protocol/src/entity/notification_filter.rs`
- Modify: `crates/protocol/src/entity/mod.rs`
- Modify: `crates/protocol/src/entity/notification.rs`

**Step 1: Write entity type definitions**

Create `crates/protocol/src/entity/notification_filter.rs`:

```rust
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleValue {
    On,
    Off,
    Default,
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
}
```

**Step 2: Add suppress_toast field to Notification entity**

Modify `crates/protocol/src/entity/notification.rs`:

```rust
// Add to Notification struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    // ... existing fields ...

    /// If true, suppress toast popup (still show in panel)
    #[serde(default)]
    pub suppress_toast: bool,
}
```

**Step 3: Export new module**

Modify `crates/protocol/src/entity/mod.rs`:

```rust
pub mod notification_filter;
```

**Step 4: Run tests**

```bash
cargo test -p waft-protocol
```

Expected: All tests pass including new serialization tests

**Step 5: Commit**

```bash
git add crates/protocol/src/entity/notification_filter.rs crates/protocol/src/entity/mod.rs crates/protocol/src/entity/notification.rs
git commit -m "feat(protocol): add notification filtering entity types

- Add NotificationGroup, NotificationProfile, ActiveProfile entities
- Add RuleCombinator with nested AND/OR support
- Add Pattern with MatchField and MatchOperator
- Add suppress_toast field to Notification entity

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Implement TOML Config Schema

**Files:**

- Modify: `plugins/notifications/src/config.rs`
- Modify: `plugins/notifications/Cargo.toml`

**Step 1: Add toml parsing types to config.rs**

Add to `plugins/notifications/src/config.rs` (after existing SoundConfig):

```rust
use std::collections::HashMap;
use waft_protocol::entity::notification_filter::{
    CombinatorOperator, GroupRule, MatchField, MatchOperator, NotificationGroup,
    NotificationProfile, Pattern, RuleCombinator, RuleNode, RuleValue,
};

/// TOML representation of notification groups config.
#[derive(Debug, Clone, Deserialize)]
pub struct GroupsConfig {
    #[serde(default)]
    pub groups: Vec<TomlGroup>,
    #[serde(default)]
    pub profiles: Vec<TomlProfile>,
}

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

/// Convert TOML types to protocol entity types.
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

    let groups = settings
        .get("groups")
        .and_then(|v| v.clone().try_into::<Vec<TomlGroup>>().ok())
        .map(|groups| groups.into_iter().map(Into::into).collect())
        .unwrap_or_default();

    let profiles = settings
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
```

**Step 2: Add tests**

Add to `plugins/notifications/src/config.rs` tests module:

```rust
#[test]
fn load_filter_config_parses_groups() {
    // This will use actual config file if present, or return empty
    let (groups, _) = load_filter_config();
    // Just verify it doesn't panic
    assert!(groups.len() >= 0);
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
    assert!(matches!(proto_group.matcher.children[1], RuleNode::Combinator(_)));
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
```

**Step 3: Run tests**

```bash
cargo test -p waft-plugin-notifications config
```

Expected: All config tests pass

**Step 4: Commit**

```bash
git add plugins/notifications/src/config.rs
git commit -m "feat(notifications): add TOML config parsing for groups and profiles

- Add TomlGroup, TomlProfile types with serde
- Add conversion to protocol entity types
- Add load_filter_config() function
- Add comprehensive parsing tests

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Implement Pattern Matching Engine

**Files:**

- Create: `plugins/notifications/src/filter/mod.rs`
- Create: `plugins/notifications/src/filter/matcher.rs`
- Create: `plugins/notifications/src/filter/compiler.rs`
- Modify: `plugins/notifications/src/lib.rs`
- Modify: `plugins/notifications/Cargo.toml`

**Step 1: Add regex dependency**

Modify `plugins/notifications/Cargo.toml`:

```toml
[dependencies]
# ... existing deps ...
regex = "1"
```

**Step 2: Write pattern matching tests**

Create `plugins/notifications/src/filter/matcher.rs`:

```rust
//! Pattern matching against notifications.

use crate::dbus::ingress::IngressedNotification;
use waft_protocol::entity::notification_filter::{
    CombinatorOperator, MatchField, MatchOperator, Pattern, RuleCombinator, RuleNode,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::hints::Hints;
    use crate::types::NotificationUrgency;
    use std::sync::Arc;
    use std::time::SystemTime;

    fn make_notification(app_name: &str, title: &str, urgency: NotificationUrgency) -> IngressedNotification {
        IngressedNotification {
            app_name: Some(Arc::from(app_name)),
            actions: vec![],
            created_at: SystemTime::now(),
            description: Arc::from("test body"),
            icon: None,
            id: 1,
            hints: Hints {
                urgency,
                ..Default::default()
            },
            replaces_id: None,
            title: Arc::from(title),
            ttl: None,
        }
    }

    #[test]
    fn test_match_app_name_contains() {
        let pattern = Pattern {
            field: MatchField::AppName,
            operator: MatchOperator::Contains,
            value: "slack".to_string(),
        };

        let notif = make_notification("Slack Desktop", "Test", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, &None));

        let notif2 = make_notification("Firefox", "Test", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, &None));
    }

    #[test]
    fn test_match_title_equals() {
        let pattern = Pattern {
            field: MatchField::Title,
            operator: MatchOperator::Equals,
            value: "Meeting".to_string(),
        };

        let notif = make_notification("App", "Meeting", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, &None));

        let notif2 = make_notification("App", "meeting", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif2, &None)); // Case-insensitive

        let notif3 = make_notification("App", "Meeting Time", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif3, &None));
    }

    #[test]
    fn test_match_urgency() {
        let pattern = Pattern {
            field: MatchField::Urgency,
            operator: MatchOperator::Equals,
            value: "critical".to_string(),
        };

        let notif = make_notification("App", "Test", NotificationUrgency::Critical);
        assert!(matches_pattern(&pattern, &notif, &None));

        let notif2 = make_notification("App", "Test", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, &None));
    }

    #[test]
    fn test_combinator_and() {
        let combinator = RuleCombinator {
            operator: CombinatorOperator::And,
            children: vec![
                RuleNode::Pattern(Pattern {
                    field: MatchField::AppName,
                    operator: MatchOperator::Contains,
                    value: "slack".to_string(),
                }),
                RuleNode::Pattern(Pattern {
                    field: MatchField::Urgency,
                    operator: MatchOperator::Equals,
                    value: "critical".to_string(),
                }),
            ],
        };

        let notif1 = make_notification("Slack", "Test", NotificationUrgency::Critical);
        assert!(matches_combinator(&combinator, &notif1, &None));

        let notif2 = make_notification("Slack", "Test", NotificationUrgency::Normal);
        assert!(!matches_combinator(&combinator, &notif2, &None)); // Fails urgency

        let notif3 = make_notification("Firefox", "Test", NotificationUrgency::Critical);
        assert!(!matches_combinator(&combinator, &notif3, &None)); // Fails app_name
    }

    #[test]
    fn test_combinator_or() {
        let combinator = RuleCombinator {
            operator: CombinatorOperator::Or,
            children: vec![
                RuleNode::Pattern(Pattern {
                    field: MatchField::Title,
                    operator: MatchOperator::Contains,
                    value: "meeting".to_string(),
                }),
                RuleNode::Pattern(Pattern {
                    field: MatchField::Urgency,
                    operator: MatchOperator::Equals,
                    value: "critical".to_string(),
                }),
            ],
        };

        let notif1 = make_notification("App", "Meeting reminder", NotificationUrgency::Normal);
        assert!(matches_combinator(&combinator, &notif1, &None)); // Matches title

        let notif2 = make_notification("App", "Test", NotificationUrgency::Critical);
        assert!(matches_combinator(&combinator, &notif2, &None)); // Matches urgency

        let notif3 = make_notification("App", "Test", NotificationUrgency::Normal);
        assert!(!matches_combinator(&combinator, &notif3, &None)); // Matches neither
    }

    #[test]
    fn test_nested_combinator() {
        let combinator = RuleCombinator {
            operator: CombinatorOperator::And,
            children: vec![
                RuleNode::Pattern(Pattern {
                    field: MatchField::AppName,
                    operator: MatchOperator::Equals,
                    value: "slack".to_string(),
                }),
                RuleNode::Combinator(RuleCombinator {
                    operator: CombinatorOperator::Or,
                    children: vec![
                        RuleNode::Pattern(Pattern {
                            field: MatchField::Title,
                            operator: MatchOperator::Contains,
                            value: "meeting".to_string(),
                        }),
                        RuleNode::Pattern(Pattern {
                            field: MatchField::Urgency,
                            operator: MatchOperator::Equals,
                            value: "critical".to_string(),
                        }),
                    ],
                }),
            ],
        };

        // app_name=slack AND (title contains meeting OR urgency=critical)
        let notif1 = make_notification("slack", "Meeting", NotificationUrgency::Normal);
        assert!(matches_combinator(&combinator, &notif1, &None));

        let notif2 = make_notification("slack", "Test", NotificationUrgency::Critical);
        assert!(matches_combinator(&combinator, &notif2, &None));

        let notif3 = make_notification("slack", "Test", NotificationUrgency::Normal);
        assert!(!matches_combinator(&combinator, &notif3, &None)); // Fails OR clause

        let notif4 = make_notification("firefox", "Meeting", NotificationUrgency::Normal);
        assert!(!matches_combinator(&combinator, &notif4, &None)); // Fails app_name
    }
}

/// Check if pattern matches notification.
pub fn matches_pattern(
    pattern: &Pattern,
    notification: &IngressedNotification,
    _compiled_regex: &Option<regex::Regex>,
) -> bool {
    // Implementation stub for testing
    false
}

/// Check if combinator matches notification.
pub fn matches_combinator(
    combinator: &RuleCombinator,
    notification: &IngressedNotification,
    _compiled_regex_cache: &Option<()>,
) -> bool {
    // Implementation stub for testing
    false
}
```

**Step 3: Run tests to verify they fail**

```bash
cargo test -p waft-plugin-notifications filter::matcher
```

Expected: All tests fail (stubs return false)

**Step 4: Implement pattern matching**

Replace stubs in `plugins/notifications/src/filter/matcher.rs`:

```rust
use regex::Regex;
use std::collections::HashMap;

/// Check if pattern matches notification.
pub fn matches_pattern(
    pattern: &Pattern,
    notification: &IngressedNotification,
    compiled_regex: &Option<Regex>,
) -> bool {
    let field_value = extract_field(pattern.field, notification);

    match pattern.operator {
        MatchOperator::Equals => {
            field_value.eq_ignore_ascii_case(&pattern.value)
        }
        MatchOperator::NotEquals => {
            !field_value.eq_ignore_ascii_case(&pattern.value)
        }
        MatchOperator::Contains => {
            field_value.to_lowercase().contains(&pattern.value.to_lowercase())
        }
        MatchOperator::NotContains => {
            !field_value.to_lowercase().contains(&pattern.value.to_lowercase())
        }
        MatchOperator::StartsWith => {
            field_value.to_lowercase().starts_with(&pattern.value.to_lowercase())
        }
        MatchOperator::NotStartsWith => {
            !field_value.to_lowercase().starts_with(&pattern.value.to_lowercase())
        }
        MatchOperator::EndsWith => {
            field_value.to_lowercase().ends_with(&pattern.value.to_lowercase())
        }
        MatchOperator::NotEndsWith => {
            !field_value.to_lowercase().ends_with(&pattern.value.to_lowercase())
        }
        MatchOperator::MatchesRegex => {
            if let Some(regex) = compiled_regex {
                regex.is_match(&field_value)
            } else {
                // Fallback: compile on the fly (shouldn't happen in production)
                Regex::new(&pattern.value)
                    .ok()
                    .map(|r| r.is_match(&field_value))
                    .unwrap_or(false)
            }
        }
        MatchOperator::NotMatchesRegex => {
            if let Some(regex) = compiled_regex {
                !regex.is_match(&field_value)
            } else {
                Regex::new(&pattern.value)
                    .ok()
                    .map(|r| !r.is_match(&field_value))
                    .unwrap_or(true)
            }
        }
    }
}

fn extract_field(field: MatchField, notification: &IngressedNotification) -> String {
    match field {
        MatchField::AppName => notification
            .app_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        MatchField::AppId => notification
            .hints
            .desktop_entry
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        MatchField::Title => notification.title.to_string(),
        MatchField::Body => notification.description.to_string(),
        MatchField::Category => notification
            .hints
            .category_raw
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default(),
        MatchField::Urgency => match notification.hints.urgency {
            crate::types::NotificationUrgency::Low => "low".to_string(),
            crate::types::NotificationUrgency::Normal => "normal".to_string(),
            crate::types::NotificationUrgency::Critical => "critical".to_string(),
        },
        MatchField::Workspace => {
            // Workspace is extracted later in the pipeline, not available here
            String::new()
        }
    }
}

/// Check if combinator matches notification.
pub fn matches_combinator(
    combinator: &RuleCombinator,
    notification: &IngressedNotification,
    compiled_cache: &HashMap<usize, Regex>,
) -> bool {
    match combinator.operator {
        CombinatorOperator::And => {
            // All children must match
            combinator.children.iter().enumerate().all(|(idx, child)| {
                matches_node(child, notification, compiled_cache, idx)
            })
        }
        CombinatorOperator::Or => {
            // At least one child must match
            combinator.children.iter().enumerate().any(|(idx, child)| {
                matches_node(child, notification, compiled_cache, idx)
            })
        }
    }
}

fn matches_node(
    node: &RuleNode,
    notification: &IngressedNotification,
    compiled_cache: &HashMap<usize, Regex>,
    node_idx: usize,
) -> bool {
    match node {
        RuleNode::Pattern(p) => {
            let regex = compiled_cache.get(&node_idx);
            matches_pattern(p, notification, &regex.cloned())
        }
        RuleNode::Combinator(c) => matches_combinator(c, notification, compiled_cache),
    }
}
```

**Step 5: Run tests to verify they pass**

```bash
cargo test -p waft-plugin-notifications filter::matcher
```

Expected: All matcher tests pass

**Step 6: Commit**

```bash
git add plugins/notifications/src/filter/matcher.rs plugins/notifications/Cargo.toml
git commit -m "feat(notifications): implement pattern matching engine

- Add matches_pattern with all operators (equals, contains, regex, etc.)
- Add matches_combinator with recursive AND/OR evaluation
- Add extract_field for notification field access
- Case-insensitive text matching
- Comprehensive test coverage

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Implement Compiled Matcher Cache

**Files:**

- Create: `plugins/notifications/src/filter/compiler.rs`
- Modify: `plugins/notifications/src/filter/mod.rs`

**Step 1: Write compiler tests**

Create `plugins/notifications/src/filter/compiler.rs`:

```rust
//! Compiled pattern matcher with regex caching.

use regex::Regex;
use std::collections::HashMap;
use waft_protocol::entity::notification_filter::{
    MatchOperator, NotificationGroup, Pattern, RuleCombinator, RuleNode,
};

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity::notification_filter::{
        CombinatorOperator, MatchField,
    };

    #[test]
    fn test_compile_group_simple() {
        let group = NotificationGroup {
            id: "test".to_string(),
            name: "Test".to_string(),
            order: 1,
            matcher: RuleCombinator {
                operator: CombinatorOperator::And,
                children: vec![RuleNode::Pattern(Pattern {
                    field: MatchField::AppName,
                    operator: MatchOperator::Contains,
                    value: "slack".to_string(),
                })],
            },
        };

        let compiled = CompiledGroup::compile(&group);
        assert_eq!(compiled.id, "test");
        assert_eq!(compiled.order, 1);
        assert_eq!(compiled.regex_cache.len(), 0); // No regex patterns
    }

    #[test]
    fn test_compile_group_with_regex() {
        let group = NotificationGroup {
            id: "test".to_string(),
            name: "Test".to_string(),
            order: 1,
            matcher: RuleCombinator {
                operator: CombinatorOperator::And,
                children: vec![RuleNode::Pattern(Pattern {
                    field: MatchField::Title,
                    operator: MatchOperator::MatchesRegex,
                    value: r"meeting\s+\d+".to_string(),
                })],
            },
        };

        let compiled = CompiledGroup::compile(&group);
        assert_eq!(compiled.regex_cache.len(), 1); // One regex pattern cached
    }

    #[test]
    fn test_compile_group_with_invalid_regex() {
        let group = NotificationGroup {
            id: "test".to_string(),
            name: "Test".to_string(),
            order: 1,
            matcher: RuleCombinator {
                operator: CombinatorOperator::And,
                children: vec![RuleNode::Pattern(Pattern {
                    field: MatchField::Title,
                    operator: MatchOperator::MatchesRegex,
                    value: "[invalid(".to_string(), // Invalid regex
                })],
            },
        };

        let compiled = CompiledGroup::compile(&group);
        assert_eq!(compiled.regex_cache.len(), 0); // Failed to compile, no cache entry
    }

    #[test]
    fn test_compile_groups_sorted_by_order() {
        let groups = vec![
            NotificationGroup {
                id: "third".to_string(),
                name: "Third".to_string(),
                order: 3,
                matcher: RuleCombinator {
                    operator: CombinatorOperator::And,
                    children: vec![],
                },
            },
            NotificationGroup {
                id: "first".to_string(),
                name: "First".to_string(),
                order: 1,
                matcher: RuleCombinator {
                    operator: CombinatorOperator::And,
                    children: vec![],
                },
            },
            NotificationGroup {
                id: "second".to_string(),
                name: "Second".to_string(),
                order: 2,
                matcher: RuleCombinator {
                    operator: CombinatorOperator::And,
                    children: vec![],
                },
            },
        ];

        let compiled = compile_groups(&groups);
        assert_eq!(compiled.len(), 3);
        assert_eq!(compiled[0].id, "first");
        assert_eq!(compiled[1].id, "second");
        assert_eq!(compiled[2].id, "third");
    }
}

/// Compiled group with pre-compiled regexes.
pub struct CompiledGroup {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub matcher: RuleCombinator,
    pub regex_cache: HashMap<usize, Regex>,
}

impl CompiledGroup {
    /// Compile a group, pre-compiling all regex patterns.
    pub fn compile(group: &NotificationGroup) -> Self {
        let mut regex_cache = HashMap::new();
        collect_regexes(&group.matcher.children, &mut regex_cache, 0);

        Self {
            id: group.id.clone(),
            name: group.name.clone(),
            order: group.order,
            matcher: group.matcher.clone(),
            regex_cache,
        }
    }
}

/// Compile all groups and sort by order.
pub fn compile_groups(groups: &[NotificationGroup]) -> Vec<CompiledGroup> {
    let mut compiled: Vec<_> = groups.iter().map(CompiledGroup::compile).collect();
    compiled.sort_by_key(|g| g.order);
    compiled
}

fn collect_regexes(
    children: &[RuleNode],
    cache: &mut HashMap<usize, Regex>,
    mut next_idx: usize,
) -> usize {
    for child in children {
        match child {
            RuleNode::Pattern(p) => {
                if matches!(
                    p.operator,
                    MatchOperator::MatchesRegex | MatchOperator::NotMatchesRegex
                ) {
                    match Regex::new(&p.value) {
                        Ok(regex) => {
                            cache.insert(next_idx, regex);
                        }
                        Err(e) => {
                            log::warn!(
                                "[notifications/filter] Invalid regex pattern '{}': {}",
                                p.value,
                                e
                            );
                        }
                    }
                }
                next_idx += 1;
            }
            RuleNode::Combinator(c) => {
                next_idx = collect_regexes(&c.children, cache, next_idx);
            }
        }
    }
    next_idx
}
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p waft-plugin-notifications filter::compiler
```

Expected: Tests fail (no implementation yet)

**Step 3: Create filter module**

Create `plugins/notifications/src/filter/mod.rs`:

```rust
//! Notification filtering system.

pub mod compiler;
pub mod matcher;

pub use compiler::{CompiledGroup, compile_groups};
pub use matcher::{matches_combinator, matches_pattern};
```

**Step 4: Export filter module**

Modify `plugins/notifications/src/lib.rs`:

```rust
pub mod filter;
```

**Step 5: Run tests to verify they pass**

```bash
cargo test -p waft-plugin-notifications filter
```

Expected: All filter tests pass

**Step 6: Commit**

```bash
git add plugins/notifications/src/filter/
git commit -m "feat(notifications): add compiled matcher cache with regex precompilation

- Add CompiledGroup with pre-compiled regex patterns
- Add compile_groups with order-based sorting
- Add regex error handling and logging
- Add comprehensive compiler tests

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Add Active Profile State Persistence

**Files:**

- Create: `plugins/notifications/src/filter/profile_state.rs`
- Modify: `plugins/notifications/src/filter/mod.rs`

**Step 1: Write profile state tests**

Create `plugins/notifications/src/filter/profile_state.rs`:

```rust
//! Active profile state persistence.

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_active_profile_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent-waft-test-profile");
        let profile = load_active_profile_from_path(&path);
        assert_eq!(profile, None);
    }

    #[test]
    fn test_save_and_load_active_profile() {
        let path = PathBuf::from("/tmp/waft-test-profile-roundtrip");
        save_active_profile_to_path(&path, "work").unwrap();

        let loaded = load_active_profile_from_path(&path);
        assert_eq!(loaded, Some("work".to_string()));

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_active_profile_whitespace() {
        let path = PathBuf::from("/tmp/waft-test-profile-whitespace");
        std::fs::write(&path, "  work  \n").unwrap();

        let loaded = load_active_profile_from_path(&path);
        assert_eq!(loaded, Some("work".to_string()));

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }
}

/// Get the active profile state file path.
pub fn get_active_profile_path() -> PathBuf {
    let state_dir = dirs::state_dir()
        .or_else(|| dirs::data_local_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"));

    state_dir.join("waft").join("notification-profile")
}

/// Load active profile ID from state file.
pub fn load_active_profile() -> Option<String> {
    load_active_profile_from_path(&get_active_profile_path())
}

fn load_active_profile_from_path(path: &PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Save active profile ID to state file.
pub fn save_active_profile(profile_id: &str) -> std::io::Result<()> {
    let path = get_active_profile_path();
    save_active_profile_to_path(&path, profile_id)
}

fn save_active_profile_to_path(path: &PathBuf, profile_id: &str) -> std::io::Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Atomic write via temp file
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, profile_id)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}
```

**Step 2: Export module**

Modify `plugins/notifications/src/filter/mod.rs`:

```rust
pub mod profile_state;

pub use profile_state::{load_active_profile, save_active_profile};
```

**Step 3: Run tests**

```bash
cargo test -p waft-plugin-notifications filter::profile_state
```

Expected: All profile state tests pass

**Step 4: Commit**

```bash
git add plugins/notifications/src/filter/profile_state.rs plugins/notifications/src/filter/mod.rs
git commit -m "feat(notifications): add active profile state persistence

- Add load_active_profile/save_active_profile functions
- Store in XDG_STATE_HOME/waft/notification-profile
- Atomic write via temp file
- Comprehensive tests

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 6: Integrate Filtering into Plugin

**Files:**

- Modify: `plugins/notifications/src/lib.rs`
- Modify: `plugins/notifications/bin/waft-notifications.rs`

**Step 1: Add filter state to plugin**

Modify `plugins/notifications/src/lib.rs`:

```rust
use self::filter::{CompiledGroup, compile_groups};
use waft_protocol::entity::notification_filter::{
    NotificationGroup, NotificationProfile, ActiveProfile,
    NOTIFICATION_GROUP_ENTITY_TYPE, NOTIFICATION_PROFILE_ENTITY_TYPE,
    ACTIVE_PROFILE_ENTITY_TYPE,
};

pub struct NotificationsPlugin {
    state: Arc<StdMutex<State>>,
    outbound_tx: flume::Sender<OutboundEvent>,

    // Filter configuration
    groups: Arc<StdMutex<Vec<NotificationGroup>>>,
    profiles: Arc<StdMutex<Vec<NotificationProfile>>>,
    active_profile_id: Arc<StdMutex<String>>,
    compiled_matchers: Arc<StdMutex<Vec<CompiledGroup>>>,
}

impl NotificationsPlugin {
    pub fn new(
        state: Arc<StdMutex<State>>,
        outbound_tx: flume::Sender<OutboundEvent>,
        groups: Vec<NotificationGroup>,
        profiles: Vec<NotificationProfile>,
        active_profile_id: String,
    ) -> Self {
        let compiled_matchers = compile_groups(&groups);

        Self {
            state,
            outbound_tx,
            groups: Arc::new(StdMutex::new(groups)),
            profiles: Arc::new(StdMutex::new(profiles)),
            active_profile_id: Arc::new(StdMutex::new(active_profile_id)),
            compiled_matchers: Arc::new(StdMutex::new(compiled_matchers)),
        }
    }

    /// Match notification to first matching group.
    pub fn match_notification(&self, notification: &IngressedNotification) -> Option<String> {
        let compiled = match self.compiled_matchers.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[notifications] mutex poisoned in match_notification: {e}");
                e.into_inner()
            }
        };

        for group in compiled.iter() {
            if filter::matcher::matches_combinator(
                &group.matcher,
                notification,
                &group.regex_cache,
            ) {
                return Some(group.id.clone());
            }
        }

        None
    }

    /// Get filter actions for a notification based on matched group and active profile.
    pub fn get_filter_actions(&self, group_id: Option<&str>) -> FilterActions {
        let Some(group_id) = group_id else {
            return FilterActions::default();
        };

        let active_profile_id = match self.active_profile_id.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                warn!("[notifications] mutex poisoned in get_filter_actions: {e}");
                e.into_inner().clone()
            }
        };

        let profiles = match self.profiles.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[notifications] mutex poisoned in get_filter_actions: {e}");
                e.into_inner()
            }
        };

        let profile = profiles.iter().find(|p| p.id == active_profile_id);
        let Some(profile) = profile else {
            return FilterActions::default();
        };

        let rule = profile.rules.get(group_id);
        let Some(rule) = rule else {
            return FilterActions::default();
        };

        FilterActions {
            hide: rule.hide == waft_protocol::entity::notification_filter::RuleValue::On,
            no_toast: rule.no_toast == waft_protocol::entity::notification_filter::RuleValue::On,
            no_sound: rule.no_sound == waft_protocol::entity::notification_filter::RuleValue::On,
        }
    }
}

#[derive(Debug, Default)]
pub struct FilterActions {
    pub hide: bool,
    pub no_toast: bool,
    pub no_sound: bool,
}
```

**Step 2: Add filter config entities to get_entities()**

Modify `NotificationsPlugin::get_entities()` in `plugins/notifications/src/lib.rs`:

```rust
fn get_entities(&self) -> Vec<Entity> {
    let guard = match self.state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[notifications] mutex poisoned in get_entities: {e}");
            e.into_inner()
        }
    };

    let mut entities = Vec::new();

    // DND entity (existing)
    let dnd = proto::Dnd { active: guard.dnd };
    entities.push(Entity::new(
        Urn::new("notifications", proto::DND_ENTITY_TYPE, "default"),
        proto::DND_ENTITY_TYPE,
        &dnd,
    ));

    // Notification entities (existing)
    for (id, _lifecycle) in &guard.panel_notifications {
        // ... existing code ...
    }

    // Notification groups
    let groups = match self.groups.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[notifications] mutex poisoned: {e}");
            e.into_inner()
        }
    };

    for group in groups.iter() {
        entities.push(Entity::new(
            Urn::new("notifications", NOTIFICATION_GROUP_ENTITY_TYPE, &group.id),
            NOTIFICATION_GROUP_ENTITY_TYPE,
            group,
        ));
    }

    // Notification profiles
    let profiles = match self.profiles.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[notifications] mutex poisoned: {e}");
            e.into_inner()
        }
    };

    for profile in profiles.iter() {
        entities.push(Entity::new(
            Urn::new("notifications", NOTIFICATION_PROFILE_ENTITY_TYPE, &profile.id),
            NOTIFICATION_PROFILE_ENTITY_TYPE,
            profile,
        ));
    }

    // Active profile
    let active_profile_id = match self.active_profile_id.lock() {
        Ok(g) => g.clone(),
        Err(e) => {
            warn!("[notifications] mutex poisoned: {e}");
            e.into_inner().clone()
        }
    };

    entities.push(Entity::new(
        Urn::new("notifications", ACTIVE_PROFILE_ENTITY_TYPE, "current"),
        ACTIVE_PROFILE_ENTITY_TYPE,
        &ActiveProfile { profile_id: active_profile_id },
    ));

    entities
}
```

**Step 3: Update main.rs to load config**

Modify `plugins/notifications/bin/waft-notifications.rs`:

```rust
use waft_plugin_notifications::config::{load_filter_config, load_sound_config};
use waft_plugin_notifications::filter::{load_active_profile, save_active_profile};

// In main():
let (groups, profiles) = load_filter_config();

// Determine active profile
let active_profile_id = load_active_profile()
    .or_else(|| profiles.first().map(|p| p.id.clone()))
    .unwrap_or_else(|| "default".to_string());

log::info!(
    "[notifications] loaded {} groups, {} profiles, active: {}",
    groups.len(),
    profiles.len(),
    active_profile_id
);

// Create the plugin with filter config
let plugin = NotificationsPlugin::new(
    state.clone(),
    outbound_tx.clone(),
    groups,
    profiles,
    active_profile_id,
);
```

**Step 4: Integrate filtering into ingress monitor**

Modify ingress monitor in `plugins/notifications/bin/waft-notifications.rs`:

```rust
IngressEvent::Notify { notification } => {
    // 1. Match notification against groups
    let matched_group = plugin.match_notification(&notification);

    // 2. Get filter actions
    let actions = plugin.get_filter_actions(matched_group.as_deref());

    // 3. Apply hide filter (drop notification entirely)
    if actions.hide {
        log::debug!(
            "[notifications] Hiding notification from {:?} (group: {:?})",
            notification.app_name,
            matched_group
        );
        continue; // Skip this notification
    }

    // 4. Evaluate sound policy (check no_sound action)
    let sound_decision = if actions.no_sound {
        SoundDecision::Suppress
    } else {
        // Existing sound policy evaluation
        let guard = match ingress_state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[notifications/ingress] mutex poisoned: {e}");
                e.into_inner()
            }
        };
        let ctx = NotificationContext {
            app_name: notification.app_name.as_ref().map(|s| s.as_ref()),
            urgency: notification.hints.urgency,
            suppress_sound: notification.hints.suppress_sound,
            sound_file: notification.hints.sound_file.as_ref().map(|s| s.as_ref()),
            sound_name: notification.hints.sound_name.as_ref().map(|s| s.as_ref()),
            category: notification.hints.category_raw.as_ref().map(|s| s.as_ref()),
            dnd_active: guard.dnd,
        };
        ingress_sound_policy.evaluate(&ctx)
    };

    // 5. Mutate state (existing logic, but mark suppress_toast)
    {
        let mut guard = match ingress_state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[notifications/ingress] mutex poisoned: {e}");
                e.into_inner()
            }
        };

        // TODO: Add suppress_toast to notification metadata
        process_op(&mut guard, NotificationOp::Ingress(notification));
    }

    // 6. Play sound if not suppressed (existing)
    if let SoundDecision::Play(sound_id) = sound_decision {
        let player = ingress_sound_player.clone();
        tokio::spawn(async move {
            player.play(&sound_id).await;
        });
    }

    // 7. Notify daemon (existing)
    ingress_notifier.notify();
    ttl_wake_for_ingress.notify_one();
}
```

**Step 5: Build and test**

```bash
cargo build -p waft-plugin-notifications
cargo test -p waft-plugin-notifications
```

Expected: All tests pass, plugin compiles

**Step 6: Commit**

```bash
git add plugins/notifications/src/lib.rs plugins/notifications/bin/waft-notifications.rs
git commit -m "feat(notifications): integrate filtering into plugin

- Add groups, profiles, active_profile state to plugin
- Add match_notification and get_filter_actions methods
- Export filter config entities in get_entities()
- Apply filtering in ingress monitor (hide, no_sound)
- Load filter config on startup

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Remove Deprioritization Code

**Files:**

- Delete: `plugins/notifications/src/store/deprioritize.rs`
- Modify: `plugins/notifications/src/store/mod.rs`
- Modify: `plugins/notifications/src/store/manager.rs`

**Step 1: Remove deprioritization module**

```bash
rm plugins/notifications/src/store/deprioritize.rs
```

**Step 2: Remove module export**

Modify `plugins/notifications/src/store/mod.rs`:

```rust
// Remove:
// pub mod deprioritize;
```

**Step 3: Remove deprioritization usage**

Modify `plugins/notifications/src/store/manager.rs`:

Remove any imports or usage of `deprioritize` module.

**Step 4: Run tests**

```bash
cargo test -p waft-plugin-notifications
```

Expected: All tests pass (deprioritization tests are gone)

**Step 5: Commit**

```bash
git add -A plugins/notifications/src/store/
git commit -m "refactor(notifications): remove hardcoded deprioritization logic

Replaced with user-configurable pattern-based filtering system.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 8: Add Entity Actions for Config Management

**Files:**

- Modify: `plugins/notifications/src/lib.rs`
- Create: `plugins/notifications/src/filter/toml_sync.rs`
- Modify: `plugins/notifications/src/filter/mod.rs`

**Step 1: Implement TOML serialization**

Create `plugins/notifications/src/filter/toml_sync.rs`:

```rust
//! TOML configuration serialization.

use std::collections::HashMap;
use waft_protocol::entity::notification_filter::{
    NotificationGroup, NotificationProfile,
};

/// Rebuild TOML config from groups and profiles.
pub fn rebuild_toml(
    groups: &[NotificationGroup],
    profiles: &[NotificationProfile],
) -> Result<String, Box<dyn std::error::Error>> {
    // Load existing config to preserve other plugin settings
    let mut config = waft_config::Config::load();

    // Find or create notifications plugin entry
    let plugin_entry = config
        .plugins
        .iter_mut()
        .find(|p| p.id == "plugin::notifications");

    let settings = if let Some(entry) = plugin_entry {
        &mut entry.settings
    } else {
        // Create new entry
        config.plugins.push(waft_config::PluginConfigEntry {
            id: "plugin::notifications".to_string(),
            use_daemon: None,
            settings: toml::Table::new(),
        });
        &mut config.plugins.last_mut().unwrap().settings
    };

    // Serialize groups
    let groups_value = toml::to_string(groups)
        .map_err(|e| format!("failed to serialize groups: {e}"))?;
    let groups_table: toml::Value = toml::from_str(&groups_value)
        .map_err(|e| format!("failed to parse groups: {e}"))?;
    settings.insert("groups".to_string(), groups_table);

    // Serialize profiles
    let profiles_value = toml::to_string(profiles)
        .map_err(|e| format!("failed to serialize profiles: {e}"))?;
    let profiles_table: toml::Value = toml::from_str(&profiles_value)
        .map_err(|e| format!("failed to parse profiles: {e}"))?;
    settings.insert("profiles".to_string(), profiles_table);

    // Serialize entire config
    toml::to_string(&config)
        .map_err(|e| format!("failed to serialize config: {e}").into())
}

/// Write config to file atomically.
pub fn write_config_atomic(content: &str) -> std::io::Result<()> {
    let path = waft_config::Config::config_path()
        .ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "config path not found"
        ))?;

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write to temp file
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, content)?;

    // Atomic rename
    std::fs::rename(&tmp_path, &path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity::notification_filter::{
        CombinatorOperator, GroupRule, MatchField, MatchOperator,
        Pattern, RuleCombinator, RuleNode, RuleValue,
    };

    #[test]
    fn test_rebuild_toml_simple() {
        let groups = vec![NotificationGroup {
            id: "test".to_string(),
            name: "Test".to_string(),
            order: 1,
            matcher: RuleCombinator {
                operator: CombinatorOperator::And,
                children: vec![RuleNode::Pattern(Pattern {
                    field: MatchField::AppName,
                    operator: MatchOperator::Contains,
                    value: "slack".to_string(),
                })],
            },
        }];

        let mut rules = HashMap::new();
        rules.insert(
            "test".to_string(),
            GroupRule {
                hide: RuleValue::Off,
                no_toast: RuleValue::On,
                no_sound: RuleValue::Default,
            },
        );

        let profiles = vec![NotificationProfile {
            id: "work".to_string(),
            name: "Work".to_string(),
            rules,
        }];

        let toml = rebuild_toml(&groups, &profiles).unwrap();
        assert!(toml.contains("plugin::notifications"));
        assert!(toml.contains("test"));
        assert!(toml.contains("work"));
    }
}
```

**Step 2: Export toml_sync module**

Modify `plugins/notifications/src/filter/mod.rs`:

```rust
pub mod toml_sync;
```

**Step 3: Add entity actions to handle_action**

Modify `plugins/notifications/src/lib.rs` `handle_action`:

```rust
async fn handle_action(
    &self,
    urn: Urn,
    action: String,
    params: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let parts: Vec<&str> = urn.as_str().split('/').collect();
    let entity_type = parts.get(1).copied().unwrap_or("");
    let entity_id = parts.get(2).copied().unwrap_or("");

    match (entity_type, action.as_str()) {
        // Existing actions (dnd, notification, etc.)
        // ...

        ("active-profile", "set-profile") => {
            let profile_id = params
                .get("profile_id")
                .and_then(|v| v.as_str())
                .ok_or("missing profile_id")?
                .to_string();

            // Update in-memory state
            {
                let mut guard = match self.active_profile_id.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };
                *guard = profile_id.clone();
            }

            // Persist to state file
            if let Err(e) = filter::save_active_profile(&profile_id) {
                warn!("[notifications] failed to save active profile: {e}");
            }

            info!("[notifications] active profile set to {profile_id}");
        }

        (_, "create-group") => {
            let group: NotificationGroup = serde_json::from_value(params)?;

            // Add to groups
            {
                let mut groups_guard = match self.groups.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };
                groups_guard.push(group.clone());
            }

            // Rebuild compiled matchers
            self.rebuild_matchers();

            // Write to TOML
            self.sync_config_to_toml()?;

            info!("[notifications] created group {}", group.id);
        }

        ("notification-group", "update-group") => {
            let group: NotificationGroup = serde_json::from_value(params)?;

            // Update group
            {
                let mut groups_guard = match self.groups.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };

                if let Some(existing) = groups_guard.iter_mut().find(|g| g.id == entity_id) {
                    *existing = group.clone();
                } else {
                    return Err("group not found".into());
                }
            }

            // Rebuild compiled matchers
            self.rebuild_matchers();

            // Write to TOML
            self.sync_config_to_toml()?;

            info!("[notifications] updated group {}", group.id);
        }

        ("notification-group", "delete-group") => {
            // Remove from groups
            {
                let mut groups_guard = match self.groups.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };
                groups_guard.retain(|g| g.id != entity_id);
            }

            // Remove from all profile rules
            {
                let mut profiles_guard = match self.profiles.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };

                for profile in profiles_guard.iter_mut() {
                    profile.rules.remove(entity_id);
                }
            }

            // Rebuild compiled matchers
            self.rebuild_matchers();

            // Write to TOML
            self.sync_config_to_toml()?;

            info!("[notifications] deleted group {entity_id}");
        }

        (_, "create-profile") => {
            let profile: NotificationProfile = serde_json::from_value(params)?;

            // Add to profiles
            {
                let mut profiles_guard = match self.profiles.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };
                profiles_guard.push(profile.clone());
            }

            // Write to TOML
            self.sync_config_to_toml()?;

            info!("[notifications] created profile {}", profile.id);
        }

        ("notification-profile", "update-profile") => {
            let profile: NotificationProfile = serde_json::from_value(params)?;

            // Update profile
            {
                let mut profiles_guard = match self.profiles.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };

                if let Some(existing) = profiles_guard.iter_mut().find(|p| p.id == entity_id) {
                    *existing = profile.clone();
                } else {
                    return Err("profile not found".into());
                }
            }

            // Write to TOML
            self.sync_config_to_toml()?;

            info!("[notifications] updated profile {}", profile.id);
        }

        ("notification-profile", "delete-profile") => {
            // Remove from profiles
            {
                let mut profiles_guard = match self.profiles.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned: {e}");
                        e.into_inner()
                    }
                };
                profiles_guard.retain(|p| p.id != entity_id);
            }

            // Write to TOML
            self.sync_config_to_toml()?;

            info!("[notifications] deleted profile {entity_id}");
        }

        _ => {
            debug!(
                "[notifications] Unknown action '{}' on entity type '{}'",
                action, entity_type
            );
        }
    }

    Ok(())
}

// Helper methods
impl NotificationsPlugin {
    fn rebuild_matchers(&self) {
        let groups = match self.groups.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[notifications] mutex poisoned: {e}");
                e.into_inner()
            }
        };

        let compiled = filter::compile_groups(&groups);

        let mut matchers_guard = match self.compiled_matchers.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[notifications] mutex poisoned: {e}");
                e.into_inner()
            }
        };
        *matchers_guard = compiled;
    }

    fn sync_config_to_toml(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let groups = match self.groups.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                warn!("[notifications] mutex poisoned: {e}");
                e.into_inner().clone()
            }
        };

        let profiles = match self.profiles.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                warn!("[notifications] mutex poisoned: {e}");
                e.into_inner().clone()
            }
        };

        let toml = filter::toml_sync::rebuild_toml(&groups, &profiles)?;
        filter::toml_sync::write_config_atomic(&toml)?;

        Ok(())
    }
}
```

**Step 4: Run tests**

```bash
cargo test -p waft-plugin-notifications
cargo build -p waft-plugin-notifications
```

Expected: All tests pass, plugin compiles

**Step 5: Commit**

```bash
git add plugins/notifications/src/filter/toml_sync.rs plugins/notifications/src/filter/mod.rs plugins/notifications/src/lib.rs
git commit -m "feat(notifications): add entity actions for config management

- Add create-group, update-group, delete-group actions
- Add create-profile, update-profile, delete-profile actions
- Add set-profile action for active profile switching
- Add TOML serialization with atomic write
- Rebuild matchers on config changes

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```
