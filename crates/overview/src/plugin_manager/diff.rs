//! Widget diffing algorithm for minimizing GTK widget churn
//!
//! This module implements efficient comparison of widget trees to identify
//! exactly what changed between plugin updates. This allows the renderer to
//! only update what's necessary, avoiding expensive GTK widget recreation.

use waft_ipc::widget::{NamedWidget, Widget};
use std::collections::HashMap;

/// Represents a change in the widget tree
#[derive(Debug, Clone)]
pub enum WidgetDiff {
    /// A new widget was added
    Added(NamedWidget),
    /// An existing widget was modified
    Updated {
        id: String,
        old: Widget,
        new: Widget,
    },
    /// A widget was removed
    Removed(String),
}

/// Compares two widget sets and returns the minimal set of changes
///
/// This function identifies widgets that were added, updated, or removed
/// between the old and new sets. Widgets are compared by ID, and deep
/// equality checks are performed on the widget trees to detect changes.
///
/// # Algorithm
///
/// 1. Build a HashMap of old widgets indexed by ID for O(1) lookup
/// 2. For each new widget:
///    - If ID doesn't exist in old set -> Added
///    - If ID exists but widget tree differs -> Updated
///    - Remove processed ID from HashMap
/// 3. Any remaining IDs in HashMap -> Removed
///
/// # Example
///
/// ```
/// use waft_overview::plugin_manager::{diff_widgets, WidgetDiff};
/// use waft_ipc::widget::{NamedWidget, Widget, Slot};
///
/// let old = vec![
///     NamedWidget {
///         id: "widget1".to_string(),
///         slot: Slot::Controls,
///         weight: 10,
///         widget: Widget::Label {
///             text: "Old".to_string(),
///             css_classes: vec![],
///         },
///     },
/// ];
///
/// let new = vec![
///     NamedWidget {
///         id: "widget1".to_string(),
///         slot: Slot::Controls,
///         weight: 10,
///         widget: Widget::Label {
///             text: "New".to_string(),
///             css_classes: vec![],
///         },
///     },
/// ];
///
/// let diffs = diff_widgets(&old, &new);
/// assert_eq!(diffs.len(), 1);
/// ```
pub fn diff_widgets(
    old_widgets: &[NamedWidget],
    new_widgets: &[NamedWidget],
) -> Vec<WidgetDiff> {
    let mut diffs = Vec::new();

    // Build lookup map of old widgets by ID
    let mut old_map: HashMap<String, &NamedWidget> = old_widgets
        .iter()
        .map(|w| (w.id.clone(), w))
        .collect();

    // Process each new widget
    for new_widget in new_widgets {
        match old_map.remove(&new_widget.id) {
            None => {
                // Widget ID doesn't exist in old set -> Added
                diffs.push(WidgetDiff::Added(new_widget.clone()));
            }
            Some(old_widget) => {
                // Widget exists, check if it changed
                if !widgets_equal(&old_widget.widget, &new_widget.widget)
                    || old_widget.slot != new_widget.slot
                    || old_widget.weight != new_widget.weight
                {
                    diffs.push(WidgetDiff::Updated {
                        id: new_widget.id.clone(),
                        old: old_widget.widget.clone(),
                        new: new_widget.widget.clone(),
                    });
                }
                // If widgets are equal, no diff is recorded
            }
        }
    }

    // Any remaining widgets in old_map were removed
    for (id, _) in old_map {
        diffs.push(WidgetDiff::Removed(id));
    }

    diffs
}

/// Deep equality check for widgets, including nested children
///
/// This function performs structural comparison of widget trees,
/// checking all fields and recursively comparing nested widgets
/// in containers and expandable content.
fn widgets_equal(a: &Widget, b: &Widget) -> bool {
    match (a, b) {
        (
            Widget::FeatureToggle {
                title: t1,
                icon: i1,
                details: d1,
                active: a1,
                busy: b1,
                expandable: e1,
                expanded_content: ec1,
                on_toggle: ot1,
            },
            Widget::FeatureToggle {
                title: t2,
                icon: i2,
                details: d2,
                active: a2,
                busy: b2,
                expandable: e2,
                expanded_content: ec2,
                on_toggle: ot2,
            },
        ) => {
            t1 == t2
                && i1 == i2
                && d1 == d2
                && a1 == a2
                && b1 == b2
                && e1 == e2
                && actions_equal(ot1, ot2)
                && match (ec1, ec2) {
                    (None, None) => true,
                    (Some(w1), Some(w2)) => widgets_equal(w1, w2),
                    _ => false,
                }
        }

        (
            Widget::Slider {
                icon: i1,
                value: v1,
                muted: m1,
                expandable: e1,
                expanded_content: ec1,
                on_value_change: ovc1,
                on_icon_click: oic1,
            },
            Widget::Slider {
                icon: i2,
                value: v2,
                muted: m2,
                expandable: e2,
                expanded_content: ec2,
                on_value_change: ovc2,
                on_icon_click: oic2,
            },
        ) => {
            i1 == i2
                && float_eq(*v1, *v2)
                && m1 == m2
                && e1 == e2
                && actions_equal(ovc1, ovc2)
                && actions_equal(oic1, oic2)
                && match (ec1, ec2) {
                    (None, None) => true,
                    (Some(w1), Some(w2)) => widgets_equal(w1, w2),
                    _ => false,
                }
        }

        (
            Widget::Container {
                orientation: o1,
                spacing: s1,
                css_classes: c1,
                children: ch1,
            },
            Widget::Container {
                orientation: o2,
                spacing: s2,
                css_classes: c2,
                children: ch2,
            },
        ) => {
            o1 == o2
                && s1 == s2
                && c1 == c2
                && ch1.len() == ch2.len()
                && ch1
                    .iter()
                    .zip(ch2.iter())
                    .all(|(w1, w2)| widgets_equal(w1, w2))
        }

        (
            Widget::MenuRow {
                icon: i1,
                label: l1,
                sublabel: sl1,
                trailing: t1,
                sensitive: s1,
                on_click: oc1,
            },
            Widget::MenuRow {
                icon: i2,
                label: l2,
                sublabel: sl2,
                trailing: t2,
                sensitive: s2,
                on_click: oc2,
            },
        ) => {
            i1 == i2
                && l1 == l2
                && sl1 == sl2
                && s1 == s2
                && match (oc1, oc2) {
                    (None, None) => true,
                    (Some(a1), Some(a2)) => actions_equal(a1, a2),
                    _ => false,
                }
                && match (t1, t2) {
                    (None, None) => true,
                    (Some(w1), Some(w2)) => widgets_equal(w1, w2),
                    _ => false,
                }
        }

        (
            Widget::Switch {
                active: a1,
                sensitive: s1,
                on_toggle: ot1,
            },
            Widget::Switch {
                active: a2,
                sensitive: s2,
                on_toggle: ot2,
            },
        ) => a1 == a2 && s1 == s2 && actions_equal(ot1, ot2),

        (Widget::Spinner { spinning: s1 }, Widget::Spinner { spinning: s2 }) => s1 == s2,

        (Widget::Checkmark { visible: v1 }, Widget::Checkmark { visible: v2 }) => v1 == v2,

        (
            Widget::Button {
                label: l1,
                icon: i1,
                on_click: oc1,
            },
            Widget::Button {
                label: l2,
                icon: i2,
                on_click: oc2,
            },
        ) => l1 == l2 && i1 == i2 && actions_equal(oc1, oc2),

        (
            Widget::Label {
                text: t1,
                css_classes: c1,
            },
            Widget::Label {
                text: t2,
                css_classes: c2,
            },
        ) => t1 == t2 && c1 == c2,

        // Different widget types are never equal
        _ => false,
    }
}

/// Compare actions for equality
fn actions_equal(a: &waft_ipc::widget::Action, b: &waft_ipc::widget::Action) -> bool {
    use waft_ipc::widget::ActionParams;

    if a.id != b.id {
        return false;
    }

    match (&a.params, &b.params) {
        (ActionParams::None, ActionParams::None) => true,
        (ActionParams::Value(v1), ActionParams::Value(v2)) => float_eq(*v1, *v2),
        (ActionParams::String(s1), ActionParams::String(s2)) => s1 == s2,
        (ActionParams::Map(m1), ActionParams::Map(m2)) => m1 == m2,
        _ => false,
    }
}

/// Float equality with epsilon tolerance
fn float_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < f64::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_ipc::widget::{Action, ActionParams, Orientation, Slot};

    fn make_label(id: &str, text: &str) -> NamedWidget {
        NamedWidget {
            id: id.to_string(),
            slot: Slot::Controls,
            weight: 10,
            widget: Widget::Label {
                text: text.to_string(),
                css_classes: vec![],
            },
        }
    }

    #[test]
    fn test_diff_empty_sets() {
        let diffs = diff_widgets(&[], &[]);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_diff_all_added() {
        let old = vec![];
        let new = vec![make_label("w1", "Widget 1"), make_label("w2", "Widget 2")];

        let diffs = diff_widgets(&old, &new);
        assert_eq!(diffs.len(), 2);

        assert!(matches!(&diffs[0], WidgetDiff::Added(w) if w.id == "w1"));
        assert!(matches!(&diffs[1], WidgetDiff::Added(w) if w.id == "w2"));
    }

    #[test]
    fn test_diff_all_removed() {
        let old = vec![make_label("w1", "Widget 1"), make_label("w2", "Widget 2")];
        let new = vec![];

        let diffs = diff_widgets(&old, &new);
        assert_eq!(diffs.len(), 2);

        let removed_ids: Vec<String> = diffs
            .iter()
            .filter_map(|d| match d {
                WidgetDiff::Removed(id) => Some(id.clone()),
                _ => None,
            })
            .collect();

        assert!(removed_ids.contains(&"w1".to_string()));
        assert!(removed_ids.contains(&"w2".to_string()));
    }

    #[test]
    fn test_diff_no_changes() {
        let old = vec![make_label("w1", "Widget 1")];
        let new = vec![make_label("w1", "Widget 1")];

        let diffs = diff_widgets(&old, &new);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_diff_text_updated() {
        let old = vec![make_label("w1", "Old Text")];
        let new = vec![make_label("w1", "New Text")];

        let diffs = diff_widgets(&old, &new);
        assert_eq!(diffs.len(), 1);

        match &diffs[0] {
            WidgetDiff::Updated { id, old: _, new: _ } => {
                assert_eq!(id, "w1");
            }
            _ => panic!("Expected Updated diff"),
        }
    }

    #[test]
    fn test_diff_slot_changed() {
        let old = vec![NamedWidget {
            id: "w1".to_string(),
            slot: Slot::Controls,
            weight: 10,
            widget: Widget::Label {
                text: "Widget".to_string(),
                css_classes: vec![],
            },
        }];

        let new = vec![NamedWidget {
            id: "w1".to_string(),
            slot: Slot::FeatureToggles, // Slot changed
            weight: 10,
            widget: Widget::Label {
                text: "Widget".to_string(),
                css_classes: vec![],
            },
        }];

        let diffs = diff_widgets(&old, &new);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(&diffs[0], WidgetDiff::Updated { id, .. } if id == "w1"));
    }

    #[test]
    fn test_diff_weight_changed() {
        let old = vec![NamedWidget {
            id: "w1".to_string(),
            slot: Slot::Controls,
            weight: 10,
            widget: Widget::Label {
                text: "Widget".to_string(),
                css_classes: vec![],
            },
        }];

        let new = vec![NamedWidget {
            id: "w1".to_string(),
            slot: Slot::Controls,
            weight: 20, // Weight changed
            widget: Widget::Label {
                text: "Widget".to_string(),
                css_classes: vec![],
            },
        }];

        let diffs = diff_widgets(&old, &new);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(&diffs[0], WidgetDiff::Updated { id, .. } if id == "w1"));
    }

    #[test]
    fn test_diff_mixed_changes() {
        let old = vec![
            make_label("w1", "Keep"),
            make_label("w2", "Remove"),
            make_label("w3", "Old"),
        ];

        let new = vec![
            make_label("w1", "Keep"),
            make_label("w3", "New"), // Updated
            make_label("w4", "Add"), // Added
        ];

        let diffs = diff_widgets(&old, &new);
        assert_eq!(diffs.len(), 3);

        let added = diffs
            .iter()
            .filter(|d| matches!(d, WidgetDiff::Added(_)))
            .count();
        let updated = diffs
            .iter()
            .filter(|d| matches!(d, WidgetDiff::Updated { .. }))
            .count();
        let removed = diffs
            .iter()
            .filter(|d| matches!(d, WidgetDiff::Removed(_)))
            .count();

        assert_eq!(added, 1);
        assert_eq!(updated, 1);
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_widgets_equal_feature_toggle() {
        let action = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        let w1 = Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth".to_string(),
            details: Some("Connected".to_string()),
            active: true,
            busy: false,
            expandable: false,
            expanded_content: None,
            on_toggle: action.clone(),
        };

        let w2 = w1.clone();
        assert!(widgets_equal(&w1, &w2));

        // Change active state
        let w3 = Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth".to_string(),
            details: Some("Connected".to_string()),
            active: false, // Changed
            busy: false,
            expandable: false,
            expanded_content: None,
            on_toggle: action,
        };

        assert!(!widgets_equal(&w1, &w3));
    }

    #[test]
    fn test_widgets_equal_slider() {
        let action = Action {
            id: "set_volume".to_string(),
            params: ActionParams::Value(0.5),
        };

        let w1 = Widget::Slider {
            icon: "volume".to_string(),
            value: 0.65,
            muted: false,
            expandable: false,
            expanded_content: None,
            on_value_change: action.clone(),
            on_icon_click: action.clone(),
        };

        let w2 = w1.clone();
        assert!(widgets_equal(&w1, &w2));

        // Change value
        let w3 = Widget::Slider {
            icon: "volume".to_string(),
            value: 0.8, // Changed
            muted: false,
            expandable: false,
            expanded_content: None,
            on_value_change: action.clone(),
            on_icon_click: action,
        };

        assert!(!widgets_equal(&w1, &w3));
    }

    #[test]
    fn test_widgets_equal_container_children() {
        let w1 = Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 8,
            css_classes: vec![],
            children: vec![
                Widget::Label {
                    text: "Child 1".to_string(),
                    css_classes: vec![],
                },
                Widget::Label {
                    text: "Child 2".to_string(),
                    css_classes: vec![],
                },
            ],
        };

        let w2 = w1.clone();
        assert!(widgets_equal(&w1, &w2));

        // Change child text
        let w3 = Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 8,
            css_classes: vec![],
            children: vec![
                Widget::Label {
                    text: "Child 1".to_string(),
                    css_classes: vec![],
                },
                Widget::Label {
                    text: "Modified".to_string(), // Changed
                    css_classes: vec![],
                },
            ],
        };

        assert!(!widgets_equal(&w1, &w3));
    }

    #[test]
    fn test_widgets_equal_nested_expanded_content() {
        let action = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        let expanded = Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 4,
            css_classes: vec![],
            children: vec![Widget::Label {
                text: "Nested".to_string(),
                css_classes: vec![],
            }],
        });

        let w1 = Widget::FeatureToggle {
            title: "Feature".to_string(),
            icon: "icon".to_string(),
            details: None,
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(expanded.clone()),
            on_toggle: action.clone(),
        };

        let w2 = w1.clone();
        assert!(widgets_equal(&w1, &w2));

        // Change nested content
        let modified_expanded = Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 4,
            css_classes: vec![],
            children: vec![Widget::Label {
                text: "Modified".to_string(), // Changed
                css_classes: vec![],
            }],
        });

        let w3 = Widget::FeatureToggle {
            title: "Feature".to_string(),
            icon: "icon".to_string(),
            details: None,
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(modified_expanded),
            on_toggle: action,
        };

        assert!(!widgets_equal(&w1, &w3));
    }

    #[test]
    fn test_widgets_equal_menu_row_trailing() {
        let w1 = Widget::MenuRow {
            icon: Some("icon".to_string()),
            label: "Label".to_string(),
            sublabel: None,
            trailing: Some(Box::new(Widget::Switch {
                active: true,
                sensitive: true,
                on_toggle: Action {
                    id: "toggle".to_string(),
                    params: ActionParams::None,
                },
            })),
            sensitive: true,
            on_click: None,
        };

        let w2 = w1.clone();
        assert!(widgets_equal(&w1, &w2));

        // Change trailing widget
        let w3 = Widget::MenuRow {
            icon: Some("icon".to_string()),
            label: "Label".to_string(),
            sublabel: None,
            trailing: Some(Box::new(Widget::Spinner { spinning: true })),
            sensitive: true,
            on_click: None,
        };

        assert!(!widgets_equal(&w1, &w3));
    }

    #[test]
    fn test_actions_equal() {
        let a1 = Action {
            id: "action".to_string(),
            params: ActionParams::Value(0.5),
        };
        let a2 = a1.clone();
        assert!(actions_equal(&a1, &a2));

        let a3 = Action {
            id: "different".to_string(),
            params: ActionParams::Value(0.5),
        };
        assert!(!actions_equal(&a1, &a3));

        let a4 = Action {
            id: "action".to_string(),
            params: ActionParams::Value(0.8),
        };
        assert!(!actions_equal(&a1, &a4));
    }

    #[test]
    fn test_float_equality() {
        assert!(float_eq(0.5, 0.5));
        assert!(float_eq(0.1 + 0.2, 0.3));
        assert!(!float_eq(0.5, 0.501));
    }

    #[test]
    fn test_diff_preserves_order() {
        let old = vec![make_label("w1", "One"), make_label("w2", "Two")];

        let new = vec![
            make_label("w1", "One"),
            make_label("w3", "Three"),
            make_label("w2", "Two"),
        ];

        let diffs = diff_widgets(&old, &new);

        // Should have exactly one Added diff for w3
        let added: Vec<_> = diffs
            .iter()
            .filter(|d| matches!(d, WidgetDiff::Added(_)))
            .collect();

        assert_eq!(added.len(), 1);
    }

    #[test]
    fn test_complex_nested_structure() {
        let action = Action {
            id: "test".to_string(),
            params: ActionParams::None,
        };

        let old = vec![NamedWidget {
            id: "complex".to_string(),
            slot: Slot::FeatureToggles,
            weight: 50,
            widget: Widget::FeatureToggle {
                title: "Feature".to_string(),
                icon: "icon".to_string(),
                details: None,
                active: true,
                busy: false,
                expandable: true,
                expanded_content: Some(Box::new(Widget::Container {
                    orientation: Orientation::Vertical,
                    spacing: 4,
                    css_classes: vec!["menu".to_string()],
                    children: vec![
                        Widget::MenuRow {
                            icon: None,
                            label: "Row 1".to_string(),
                            sublabel: Some("Detail".to_string()),
                            trailing: Some(Box::new(Widget::Switch {
                                active: true,
                                sensitive: true,
                                on_toggle: action.clone(),
                            })),
                            sensitive: true,
                            on_click: None,
                        },
                        Widget::MenuRow {
                            icon: None,
                            label: "Row 2".to_string(),
                            sublabel: None,
                            trailing: None,
                            sensitive: true,
                            on_click: Some(action.clone()),
                        },
                    ],
                })),
                on_toggle: action.clone(),
            },
        }];

        let new = old.clone();
        let diffs = diff_widgets(&old, &new);
        assert!(diffs.is_empty(), "Identical complex structures should have no diffs");

        // Now modify a deeply nested field
        let mut modified = new.clone();
        if let Widget::FeatureToggle {
            expanded_content, ..
        } = &mut modified[0].widget
        {
            if let Some(container) = expanded_content {
                if let Widget::Container { children, .. } = container.as_mut() {
                    if let Widget::MenuRow { label, .. } = &mut children[0] {
                        *label = "Modified Row 1".to_string();
                    }
                }
            }
        }

        let diffs = diff_widgets(&old, &modified);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(&diffs[0], WidgetDiff::Updated { id, .. } if id == "complex"));
    }
}
