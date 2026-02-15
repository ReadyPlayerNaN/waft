//! Compiled pattern matcher with regex caching.

use std::collections::HashMap;

use regex::Regex;
use waft_protocol::entity::notification_filter::{
    MatchOperator, NotificationGroup, RuleCombinator, RuleNode,
};

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
                                "[notifications/filter] invalid regex pattern '{}': {}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity::notification_filter::{
        CombinatorOperator, MatchField, Pattern,
    };

    #[test]
    fn compile_group_simple_no_regex() {
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
        assert_eq!(compiled.regex_cache.len(), 0);
    }

    #[test]
    fn compile_group_with_regex() {
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
        assert_eq!(compiled.regex_cache.len(), 1);
    }

    #[test]
    fn compile_group_with_invalid_regex() {
        let group = NotificationGroup {
            id: "test".to_string(),
            name: "Test".to_string(),
            order: 1,
            matcher: RuleCombinator {
                operator: CombinatorOperator::And,
                children: vec![RuleNode::Pattern(Pattern {
                    field: MatchField::Title,
                    operator: MatchOperator::MatchesRegex,
                    value: "[invalid(".to_string(),
                })],
            },
        };

        let compiled = CompiledGroup::compile(&group);
        assert_eq!(compiled.regex_cache.len(), 0);
    }

    #[test]
    fn compile_groups_sorted_by_order() {
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
