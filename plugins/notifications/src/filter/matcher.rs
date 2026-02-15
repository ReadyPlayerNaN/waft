//! Pattern matching against notifications.

use std::collections::HashMap;

use regex::Regex;

use crate::dbus::ingress::IngressedNotification;
use waft_protocol::entity::notification_filter::{
    CombinatorOperator, MatchField, MatchOperator, Pattern, RuleCombinator, RuleNode,
};

/// Extract the value of a field from a notification.
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
            // Workspace is not available at ingress time
            String::new()
        }
    }
}

/// Check if a single pattern matches a notification.
pub fn matches_pattern(
    pattern: &Pattern,
    notification: &IngressedNotification,
    compiled_regex: Option<&Regex>,
) -> bool {
    let field_value = extract_field(pattern.field, notification);

    match pattern.operator {
        MatchOperator::Equals => field_value.eq_ignore_ascii_case(&pattern.value),
        MatchOperator::NotEquals => !field_value.eq_ignore_ascii_case(&pattern.value),
        MatchOperator::Contains => field_value
            .to_lowercase()
            .contains(&pattern.value.to_lowercase()),
        MatchOperator::NotContains => !field_value
            .to_lowercase()
            .contains(&pattern.value.to_lowercase()),
        MatchOperator::StartsWith => field_value
            .to_lowercase()
            .starts_with(&pattern.value.to_lowercase()),
        MatchOperator::NotStartsWith => !field_value
            .to_lowercase()
            .starts_with(&pattern.value.to_lowercase()),
        MatchOperator::EndsWith => field_value
            .to_lowercase()
            .ends_with(&pattern.value.to_lowercase()),
        MatchOperator::NotEndsWith => !field_value
            .to_lowercase()
            .ends_with(&pattern.value.to_lowercase()),
        MatchOperator::MatchesRegex => {
            if let Some(regex) = compiled_regex {
                regex.is_match(&field_value)
            } else {
                // Fallback: compile on the fly
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

/// Check if a combinator tree matches a notification.
pub fn matches_combinator(
    combinator: &RuleCombinator,
    notification: &IngressedNotification,
    compiled_cache: &HashMap<usize, Regex>,
) -> bool {
    match combinator.operator {
        CombinatorOperator::And => combinator
            .children
            .iter()
            .enumerate()
            .all(|(idx, child)| matches_node(child, notification, compiled_cache, idx)),
        CombinatorOperator::Or => combinator
            .children
            .iter()
            .enumerate()
            .any(|(idx, child)| matches_node(child, notification, compiled_cache, idx)),
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
            matches_pattern(p, notification, regex)
        }
        RuleNode::Combinator(c) => matches_combinator(c, notification, compiled_cache),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::hints::Hints;
    use crate::types::NotificationUrgency;
    use std::sync::Arc;
    use std::time::SystemTime;

    fn make_notification(
        app_name: &str,
        title: &str,
        urgency: NotificationUrgency,
    ) -> IngressedNotification {
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
    fn match_app_name_contains() {
        let pattern = Pattern {
            field: MatchField::AppName,
            operator: MatchOperator::Contains,
            value: "slack".to_string(),
        };

        let notif = make_notification("Slack Desktop", "Test", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, None));

        let notif2 = make_notification("Firefox", "Test", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, None));
    }

    #[test]
    fn match_title_equals_case_insensitive() {
        let pattern = Pattern {
            field: MatchField::Title,
            operator: MatchOperator::Equals,
            value: "Meeting".to_string(),
        };

        let notif = make_notification("App", "Meeting", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, None));

        let notif2 = make_notification("App", "meeting", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif2, None));

        let notif3 = make_notification("App", "Meeting Time", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif3, None));
    }

    #[test]
    fn match_urgency() {
        let pattern = Pattern {
            field: MatchField::Urgency,
            operator: MatchOperator::Equals,
            value: "critical".to_string(),
        };

        let notif = make_notification("App", "Test", NotificationUrgency::Critical);
        assert!(matches_pattern(&pattern, &notif, None));

        let notif2 = make_notification("App", "Test", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, None));
    }

    #[test]
    fn match_not_equals() {
        let pattern = Pattern {
            field: MatchField::AppName,
            operator: MatchOperator::NotEquals,
            value: "firefox".to_string(),
        };

        let notif = make_notification("slack", "Test", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, None));

        let notif2 = make_notification("Firefox", "Test", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, None));
    }

    #[test]
    fn match_starts_with() {
        let pattern = Pattern {
            field: MatchField::Title,
            operator: MatchOperator::StartsWith,
            value: "meeting".to_string(),
        };

        let notif = make_notification("App", "Meeting reminder", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, None));

        let notif2 = make_notification("App", "Your meeting", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, None));
    }

    #[test]
    fn match_ends_with() {
        let pattern = Pattern {
            field: MatchField::Title,
            operator: MatchOperator::EndsWith,
            value: "reminder".to_string(),
        };

        let notif = make_notification("App", "Meeting Reminder", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, None));

        let notif2 = make_notification("App", "Reminder time", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, None));
    }

    #[test]
    fn match_regex() {
        let pattern = Pattern {
            field: MatchField::Title,
            operator: MatchOperator::MatchesRegex,
            value: r"meeting\s+\d+".to_string(),
        };

        let compiled = Regex::new(&pattern.value).unwrap();

        let notif = make_notification("App", "meeting 42", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, Some(&compiled)));

        let notif2 = make_notification("App", "meeting now", NotificationUrgency::Normal);
        assert!(!matches_pattern(&pattern, &notif2, Some(&compiled)));
    }

    #[test]
    fn combinator_and() {
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
        assert!(matches_combinator(&combinator, &notif1, &HashMap::new()));

        let notif2 = make_notification("Slack", "Test", NotificationUrgency::Normal);
        assert!(!matches_combinator(&combinator, &notif2, &HashMap::new()));

        let notif3 = make_notification("Firefox", "Test", NotificationUrgency::Critical);
        assert!(!matches_combinator(&combinator, &notif3, &HashMap::new()));
    }

    #[test]
    fn combinator_or() {
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
        assert!(matches_combinator(&combinator, &notif1, &HashMap::new()));

        let notif2 = make_notification("App", "Test", NotificationUrgency::Critical);
        assert!(matches_combinator(&combinator, &notif2, &HashMap::new()));

        let notif3 = make_notification("App", "Test", NotificationUrgency::Normal);
        assert!(!matches_combinator(&combinator, &notif3, &HashMap::new()));
    }

    #[test]
    fn nested_combinator() {
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
        assert!(matches_combinator(&combinator, &notif1, &HashMap::new()));

        let notif2 = make_notification("slack", "Test", NotificationUrgency::Critical);
        assert!(matches_combinator(&combinator, &notif2, &HashMap::new()));

        let notif3 = make_notification("slack", "Test", NotificationUrgency::Normal);
        assert!(!matches_combinator(
            &combinator,
            &notif3,
            &HashMap::new()
        ));

        let notif4 = make_notification("firefox", "Meeting", NotificationUrgency::Normal);
        assert!(!matches_combinator(
            &combinator,
            &notif4,
            &HashMap::new()
        ));
    }

    #[test]
    fn empty_and_combinator_matches_everything() {
        let combinator = RuleCombinator {
            operator: CombinatorOperator::And,
            children: vec![],
        };

        let notif = make_notification("App", "Test", NotificationUrgency::Normal);
        assert!(matches_combinator(&combinator, &notif, &HashMap::new()));
    }

    #[test]
    fn empty_or_combinator_matches_nothing() {
        let combinator = RuleCombinator {
            operator: CombinatorOperator::Or,
            children: vec![],
        };

        let notif = make_notification("App", "Test", NotificationUrgency::Normal);
        assert!(!matches_combinator(&combinator, &notif, &HashMap::new()));
    }

    #[test]
    fn body_field_matches_description() {
        let pattern = Pattern {
            field: MatchField::Body,
            operator: MatchOperator::Contains,
            value: "test".to_string(),
        };

        let notif = make_notification("App", "Title", NotificationUrgency::Normal);
        assert!(matches_pattern(&pattern, &notif, None)); // description is "test body"
    }
}
