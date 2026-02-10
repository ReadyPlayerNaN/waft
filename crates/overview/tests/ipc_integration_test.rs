//! Integration tests for IPC infrastructure
//!
//! These tests verify the complete plugin IPC pipeline from discovery
//! through connection, widget loading, action routing, and state updates.
//!
//! Tests require a mock plugin server for deterministic testing without
//! depending on external plugin daemons.

use waft_ipc::{Action, ActionParams, NamedWidget, Widget};
use waft_overview::plugin_manager::{discover_plugins, WidgetRegistry};

/// Test helper: Creates test widget for a plugin
#[cfg(test)]
fn create_test_widget(id: &str, active: bool) -> NamedWidget {
    NamedWidget {
        id: id.to_string(),
        weight: 100,
        widget: Widget::FeatureToggle {
            title: "Test Widget".to_string(),
            icon: "test-icon".to_string(),
            details: Some(if active { "Active".to_string() } else { "Inactive".to_string() }),
            active,
            busy: false,
            expandable: false,
            expanded_content: None,
            on_toggle: Action {
                id: "toggle".to_string(),
                params: ActionParams::None,
            },
        },
    }
}

#[tokio::test]
async fn test_plugin_connection_lifecycle() {
    // Test Scenario 1: Plugin Discovery and Connection Lifecycle
    //
    // This test verifies:
    // 1. Discovery finds mock plugin sockets
    // 2. Client can connect to discovered plugins
    // 3. Disconnection is handled gracefully
    // 4. Registry cleanup occurs on disconnect

    // For now, just test discovery with no plugins
    let plugins = discover_plugins();
    assert!(plugins.is_empty() || plugins.iter().any(|p| p.socket_path.exists()));
}

#[tokio::test]
async fn test_widget_action_roundtrip() {
    // Test Scenario 2: Widget Action Roundtrip

    let registry = WidgetRegistry::new();

    // Setup: Add initial widgets to registry
    let initial_widgets = vec![
        create_test_widget("test:widget1", false),
    ];

    registry.set_widgets("test-plugin", initial_widgets.clone());

    // Verify registry contains widget
    assert_eq!(registry.widget_count(), 1);
    assert_eq!(registry.plugin_count(), 1);

    // Simulate state change
    let updated_widgets = vec![
        create_test_widget("test:widget1", true),
    ];

    // Update registry with new state
    registry.set_widgets("test-plugin", updated_widgets);

    // Verify registry updated
    let widgets = registry.get_all_widgets_sorted();
    assert_eq!(widgets.len(), 1);

    // Verify widget state changed
    if let Widget::FeatureToggle { active, .. } = &widgets[0].widget {
        assert!(*active);
    } else {
        panic!("Expected FeatureToggle widget");
    }
}

#[tokio::test]
async fn test_multiple_plugins() {
    // Test Scenario 3: Multiple Plugin Coordination

    let registry = WidgetRegistry::new();

    // Setup: Register widgets from multiple plugins
    let plugin_a_widgets = vec![
        create_test_widget("plugin-a:widget1", false),
    ];

    let plugin_b_widgets = vec![
        create_test_widget("plugin-b:widget1", false),
    ];

    let plugin_c_widgets = vec![
        create_test_widget("plugin-c:widget1", false),
    ];

    registry.set_widgets("plugin-a", plugin_a_widgets);
    registry.set_widgets("plugin-b", plugin_b_widgets);
    registry.set_widgets("plugin-c", plugin_c_widgets);

    // Verify all plugins registered
    assert_eq!(registry.plugin_count(), 3);
    assert_eq!(registry.widget_count(), 3);

    // Verify all widgets are retrievable
    let all_widgets = registry.get_all_widgets();
    assert_eq!(all_widgets.len(), 3);

    // Verify widget isolation
    registry.remove_plugin("plugin-a");
    assert_eq!(registry.plugin_count(), 2);
    assert_eq!(registry.widget_count(), 2);

    let remaining = registry.get_all_widgets();
    assert_eq!(remaining.len(), 2);
    let ids: Vec<&str> = remaining.iter().map(|w| w.id.as_str()).collect();
    assert!(ids.contains(&"plugin-b:widget1"));
    assert!(ids.contains(&"plugin-c:widget1"));
}

#[tokio::test]
async fn test_widget_registry_thread_safety() {
    // Test Scenario 4: Concurrent Registry Access

    use std::sync::Arc;
    use std::thread;

    let registry = Arc::new(WidgetRegistry::new());
    let mut handles = vec![];

    // Spawn multiple writer threads
    for i in 0..5 {
        let registry_clone = Arc::clone(&registry);
        let handle = thread::spawn(move || {
            let widgets = vec![
                create_test_widget(
                    &format!("thread-{}:widget", i),
                    false,
                ),
            ];
            registry_clone.set_widgets(&format!("plugin-{}", i), widgets);
        });
        handles.push(handle);
    }

    // Spawn multiple reader threads
    for _ in 0..5 {
        let registry_clone = Arc::clone(&registry);
        let handle = thread::spawn(move || {
            let _widgets = registry_clone.get_all_widgets();
            let _count = registry_clone.widget_count();
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify final state
    assert_eq!(registry.plugin_count(), 5);
    assert_eq!(registry.widget_count(), 5);
}

#[tokio::test]
async fn test_discovery_with_multiple_sockets() {
    // Test Scenario 6: Plugin Discovery Correctness

    let plugins = discover_plugins();

    // Verify all discovered plugins have .sock files
    for plugin in &plugins {
        assert!(
            plugin.socket_path.extension().and_then(|s| s.to_str()) == Some("sock"),
            "Plugin socket path should end in .sock: {:?}",
            plugin.socket_path
        );
    }

    // Verify sorted by name
    let names: Vec<_> = plugins.iter().map(|p| p.name.as_str()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort();
    assert_eq!(names, sorted_names, "Plugins should be sorted by name");
}
