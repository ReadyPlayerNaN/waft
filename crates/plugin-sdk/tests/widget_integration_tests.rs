//! Integration tests for widget builders and protocol compatibility.
//!
//! These tests verify that widgets built with the SDK builders correctly
//! serialize/deserialize through the IPC protocol and maintain their structure.

use waft_plugin_sdk::builder::*;
use waft_ipc::widget::{Action, ActionParams, Widget};

#[test]
fn test_details_builder_round_trip_serialization() {
    let summary = LabelBuilder::new("Meeting Details").build();
    let content = ColBuilder::new()
        .spacing(8)
        .child(LabelBuilder::new("Time: 10:00 AM").build())
        .child(LabelBuilder::new("Location: Room 5").build())
        .build();

    let widget = DetailsBuilder::new()
        .summary(summary)
        .content(content)
        .css_class("calendar-event")
        .on_toggle("expand_event")
        .build();

    let json = serde_json::to_string(&widget).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::Details {
            summary,
            content,
            css_classes,
            on_toggle,
        } => {
            assert!(matches!(*summary, Widget::Label { .. }));
            assert!(matches!(*content, Widget::Col { .. }));
            assert_eq!(css_classes, vec!["calendar-event"]);
            assert_eq!(on_toggle.id, "expand_event");
        }
        _ => panic!("Expected Widget::Details"),
    }
}

#[test]
fn test_toggle_button_builder_round_trip_serialization() {
    let widget = ToggleButtonBuilder::new("view-list-symbolic")
        .active(true)
        .on_toggle("toggle_view")
        .build();

    let json = serde_json::to_string(&widget).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::ToggleButton {
            icon,
            active,
            on_toggle,
        } => {
            assert_eq!(icon, "view-list-symbolic");
            assert!(active);
            assert_eq!(on_toggle.id, "toggle_view");
        }
        _ => panic!("Expected Widget::ToggleButton"),
    }
}

#[test]
fn test_complex_nested_widget_serialization() {
    // Build a complex widget hierarchy like an EDS-agenda event card
    let event_details = DetailsBuilder::new()
        .summary(
            RowBuilder::new()
                .spacing(8)
                .child(LabelBuilder::new("Team Meeting").css_class("title").build())
                .child(LabelBuilder::new("10:00 AM").css_class("dim-label").build())
                .build(),
        )
        .content(
            ColBuilder::new()
                .spacing(4)
                .child(Widget::Separator)
                .child(
                    IconListBuilder::new("map-marker-symbolic")
                        .icon_size(16)
                        .child(LabelBuilder::new("Conference Room A").build())
                        .build(),
                )
                .child(Widget::Separator)
                .child(
                    IconListBuilder::new("avatar-default-symbolic")
                        .icon_size(16)
                        .child(LabelBuilder::new("john@example.com").build())
                        .child(LabelBuilder::new("jane@example.com").build())
                        .build(),
                )
                .build(),
        )
        .css_class("event-card")
        .on_toggle("expand_event")
        .build();

    let json = serde_json::to_string(&event_details).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::Details {
            summary,
            content,
            css_classes,
            ..
        } => {
            assert!(matches!(*summary, Widget::Row { .. }));
            assert!(matches!(*content, Widget::Col { .. }));
            assert_eq!(css_classes, vec!["event-card"]);

            // Verify nested structure
            if let Widget::Col { children, .. } = *content {
                assert_eq!(children.len(), 4); // Sep, IconList, Sep, IconList
                assert!(matches!(children[0].widget, Widget::Separator));
                assert!(matches!(children[1].widget, Widget::IconList { .. }));
                assert!(matches!(children[2].widget, Widget::Separator));
                assert!(matches!(children[3].widget, Widget::IconList { .. }));
            } else {
                panic!("Expected Col in content");
            }
        }
        _ => panic!("Expected Widget::Details"),
    }
}

#[test]
fn test_keyed_nodes_preserve_keys() {
    let row = RowBuilder::new()
        .keyed_child("event-1", LabelBuilder::new("Event 1").build())
        .keyed_child("event-2", LabelBuilder::new("Event 2").build())
        .keyed_child("event-3", LabelBuilder::new("Event 3").build())
        .build();

    let json = serde_json::to_string(&row).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::Row { children, .. } => {
            assert_eq!(children.len(), 3);
            assert_eq!(children[0].key, Some("event-1".to_string()));
            assert_eq!(children[1].key, Some("event-2".to_string()));
            assert_eq!(children[2].key, Some("event-3".to_string()));
        }
        _ => panic!("Expected Widget::Row"),
    }
}

#[test]
fn test_mixed_keyed_and_unkeyed_children() {
    let col = ColBuilder::new()
        .child(LabelBuilder::new("Header").build()) // Unkeyed
        .keyed_child("content-1", LabelBuilder::new("Item 1").build())
        .keyed_child("content-2", LabelBuilder::new("Item 2").build())
        .child(LabelBuilder::new("Footer").build()) // Unkeyed
        .build();

    let json = serde_json::to_string(&col).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::Col { children, .. } => {
            assert_eq!(children.len(), 4);
            assert_eq!(children[0].key, None);
            assert_eq!(children[1].key, Some("content-1".to_string()));
            assert_eq!(children[2].key, Some("content-2".to_string()));
            assert_eq!(children[3].key, None);
        }
        _ => panic!("Expected Widget::Col"),
    }
}

#[test]
fn test_separator_standalone_serialization() {
    let widget = Widget::Separator;
    let json = serde_json::to_string(&widget).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    assert!(matches!(deserialized, Widget::Separator));
}

#[test]
fn test_action_params_in_builders() {
    // Test that builders correctly handle different action param types
    let button_with_string_param = ButtonBuilder::new()
        .label("Open Event")
        .on_click_action(Action {
            id: "open_event".to_string(),
            params: ActionParams::String("event-123".to_string()),
        })
        .build();

    let json = serde_json::to_string(&button_with_string_param).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::Button { on_click, .. } => match on_click.params {
            ActionParams::String(s) => assert_eq!(s, "event-123"),
            _ => panic!("Expected ActionParams::String"),
        },
        _ => panic!("Expected Widget::Button"),
    }
}

#[test]
fn test_widget_css_classes_preserved() {
    let label = LabelBuilder::new("Test")
        .css_class("class1")
        .css_class("class2")
        .css_class("class3")
        .build();

    let json = serde_json::to_string(&label).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::Label { css_classes, .. } => {
            assert_eq!(css_classes, vec!["class1", "class2", "class3"]);
        }
        _ => panic!("Expected Widget::Label"),
    }
}

#[test]
fn test_empty_containers_serialization() {
    let empty_row = RowBuilder::new().build();
    let empty_col = ColBuilder::new().build();

    let row_json = serde_json::to_string(&empty_row).unwrap();
    let col_json = serde_json::to_string(&empty_col).unwrap();

    let row_de: Widget = serde_json::from_str(&row_json).unwrap();
    let col_de: Widget = serde_json::from_str(&col_json).unwrap();

    match row_de {
        Widget::Row { children, .. } => assert!(children.is_empty()),
        _ => panic!("Expected Widget::Row"),
    }

    match col_de {
        Widget::Col { children, .. } => assert!(children.is_empty()),
        _ => panic!("Expected Widget::Col"),
    }
}

#[test]
fn test_details_builder_validates_required_fields() {
    // DetailsBuilder should require both summary and content
    let result = std::panic::catch_unwind(|| {
        DetailsBuilder::new().build() // Missing both summary and content
    });
    assert!(result.is_err());

    let result = std::panic::catch_unwind(|| {
        DetailsBuilder::new()
            .summary(LabelBuilder::new("Summary").build())
            .build() // Missing content
    });
    assert!(result.is_err());

    let result = std::panic::catch_unwind(|| {
        DetailsBuilder::new()
            .content(LabelBuilder::new("Content").build())
            .build() // Missing summary
    });
    assert!(result.is_err());
}

#[test]
fn test_icon_list_with_separators() {
    let list = IconListBuilder::new("calendar-symbolic")
        .icon_size(24)
        .child(LabelBuilder::new("Event 1").build())
        .child(Widget::Separator)
        .child(LabelBuilder::new("Event 2").build())
        .child(Widget::Separator)
        .child(LabelBuilder::new("Event 3").build())
        .build();

    let json = serde_json::to_string(&list).unwrap();
    let deserialized: Widget = serde_json::from_str(&json).unwrap();

    match deserialized {
        Widget::IconList { children, .. } => {
            assert_eq!(children.len(), 5);
            assert!(matches!(children[0].widget, Widget::Label { .. }));
            assert!(matches!(children[1].widget, Widget::Separator));
            assert!(matches!(children[2].widget, Widget::Label { .. }));
            assert!(matches!(children[3].widget, Widget::Separator));
            assert!(matches!(children[4].widget, Widget::Label { .. }));
        }
        _ => panic!("Expected Widget::IconList"),
    }
}
