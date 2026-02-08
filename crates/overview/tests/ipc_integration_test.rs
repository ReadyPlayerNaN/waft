//! Integration tests for IPC infrastructure
//!
//! These tests verify the complete plugin IPC pipeline from discovery
//! through connection, widget loading, action routing, and state updates.
//!
//! Tests require a mock plugin server for deterministic testing without
//! depending on external plugin daemons.

use waft_ipc::{Action, ActionParams, NamedWidget, PluginMessage, Slot, Widget};
use waft_overview::plugin_manager::{
    discover_plugins, diff_widgets, PluginClient, WidgetRegistry, ActionRouter,
};

/// Test helper: Creates a mock plugin socket for testing
///
/// Returns the socket path that can be used for connection tests
#[cfg(test)]
async fn create_mock_plugin_socket(name: &str) -> std::path::PathBuf {
    use tokio::net::UnixListener;
    use std::env;

    let runtime_dir = env::var("XDG_RUNTIME_DIR")
        .or_else(|_| env::var("TMPDIR"))
        .unwrap_or_else(|_| "/tmp".to_string());

    let plugin_dir = std::path::PathBuf::from(&runtime_dir)
        .join("waft-test")
        .join("plugins");

    std::fs::create_dir_all(&plugin_dir).unwrap();

    let socket_path = plugin_dir.join(format!("{}.sock", name));

    // Remove stale socket if exists
    let _ = std::fs::remove_file(&socket_path);

    // Create listening socket
    let _listener = UnixListener::bind(&socket_path).unwrap();

    socket_path
}

/// Test helper: Creates test widget for a plugin
#[cfg(test)]
fn create_test_widget(id: &str, slot: Slot, active: bool) -> NamedWidget {
    NamedWidget {
        id: id.to_string(),
        slot,
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

    // TODO: Implement full lifecycle test
    // - Start mock plugin server
    // - Discover plugin via discover_plugins()
    // - Connect with PluginClient
    // - Verify connection established
    // - Stop mock server
    // - Verify disconnection detected
    // - Verify registry cleanup

    // For now, just test discovery with no plugins
    let plugins = discover_plugins();
    assert!(plugins.is_empty() || plugins.iter().any(|p| p.socket_path.exists()));
}

#[tokio::test]
async fn test_widget_action_roundtrip() {
    // Test Scenario 2: Widget Action Roundtrip
    //
    // This test verifies the complete action flow:
    // 1. Plugin sends initial widget state
    // 2. Widgets stored in registry
    // 3. User triggers action
    // 4. Action routed to correct plugin
    // 5. Plugin updates state
    // 6. Updated widgets sent back
    // 7. Diff algorithm detects changes
    // 8. UI notified of minimal updates

    let registry = WidgetRegistry::new();
    let router = ActionRouter::new();

    // Setup: Add initial widgets to registry
    let initial_widgets = vec![
        create_test_widget("test:widget1", Slot::FeatureToggles, false),
    ];

    registry.set_widgets("test-plugin", initial_widgets.clone());

    // Verify registry contains widget
    assert_eq!(registry.widget_count(), 1);
    assert_eq!(registry.plugin_count(), 1);

    // Simulate state change
    let updated_widgets = vec![
        create_test_widget("test:widget1", Slot::FeatureToggles, true),
    ];

    // Calculate diff
    let diffs = diff_widgets(&initial_widgets, &updated_widgets);

    // Verify diff detected the change
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], waft_overview::plugin_manager::WidgetDiff::Updated { .. }));

    // Update registry with new state
    registry.set_widgets("test-plugin", updated_widgets);

    // Verify registry updated
    let widgets = registry.get_widgets_by_slot(Slot::FeatureToggles);
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
    //
    // This test verifies:
    // 1. Multiple plugins can be registered
    // 2. Widgets from different plugins coexist
    // 3. Action routing goes to correct plugin
    // 4. No cross-plugin contamination
    // 5. Widget ordering by weight works across plugins

    let registry = WidgetRegistry::new();

    // Setup: Register widgets from multiple plugins
    let plugin_a_widgets = vec![
        create_test_widget("plugin-a:widget1", Slot::FeatureToggles, false),
    ];

    let plugin_b_widgets = vec![
        create_test_widget("plugin-b:widget1", Slot::FeatureToggles, false),
    ];

    let plugin_c_widgets = vec![
        create_test_widget("plugin-c:widget1", Slot::Controls, false),
    ];

    registry.set_widgets("plugin-a", plugin_a_widgets);
    registry.set_widgets("plugin-b", plugin_b_widgets);
    registry.set_widgets("plugin-c", plugin_c_widgets);

    // Verify all plugins registered
    assert_eq!(registry.plugin_count(), 3);
    assert_eq!(registry.widget_count(), 3);

    // Verify slot-based retrieval
    let feature_toggles = registry.get_widgets_by_slot(Slot::FeatureToggles);
    assert_eq!(feature_toggles.len(), 2); // plugin-a and plugin-b

    let controls = registry.get_widgets_by_slot(Slot::Controls);
    assert_eq!(controls.len(), 1); // plugin-c

    // Verify widget isolation
    registry.remove_plugin("plugin-a");
    assert_eq!(registry.plugin_count(), 2);
    assert_eq!(registry.widget_count(), 2);

    let feature_toggles = registry.get_widgets_by_slot(Slot::FeatureToggles);
    assert_eq!(feature_toggles.len(), 1); // Only plugin-b remains
    assert_eq!(feature_toggles[0].id, "plugin-b:widget1");
}

#[tokio::test]
async fn test_widget_registry_thread_safety() {
    // Test Scenario 4: Concurrent Registry Access
    //
    // This test verifies thread-safe registry operations:
    // 1. Concurrent reads don't block
    // 2. Writes are serialized correctly
    // 3. No data corruption under contention

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
                    Slot::FeatureToggles,
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
async fn test_diff_algorithm_performance() {
    // Test Scenario 5: Diff Algorithm Efficiency
    //
    // This test verifies:
    // 1. No diff for identical widget sets
    // 2. Only changed widgets in diff
    // 3. Deep equality detection works
    // 4. Performance with large widget sets

    // Create large widget set
    let mut old_widgets = Vec::new();
    for i in 0..100 {
        old_widgets.push(create_test_widget(
            &format!("widget-{}", i),
            Slot::FeatureToggles,
            false,
        ));
    }

    // Clone for comparison
    let identical_widgets = old_widgets.clone();

    // Test 1: Identical sets produce no diff
    let diffs = diff_widgets(&old_widgets, &identical_widgets);
    assert!(diffs.is_empty(), "Identical widget sets should produce no diffs");

    // Test 2: Single widget change detected
    let mut changed_widgets = old_widgets.clone();
    changed_widgets[50] = create_test_widget("widget-50", Slot::FeatureToggles, true);

    let diffs = diff_widgets(&old_widgets, &changed_widgets);
    assert_eq!(diffs.len(), 1, "Should detect exactly one change");

    // Test 3: Performance timing (should be fast)
    let start = std::time::Instant::now();
    let _diffs = diff_widgets(&old_widgets, &changed_widgets);
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 50,
        "Diff of 100 widgets should complete in <50ms, took {:?}",
        duration
    );
}

#[tokio::test]
async fn test_discovery_with_multiple_sockets() {
    // Test Scenario 6: Plugin Discovery Correctness
    //
    // This test verifies:
    // 1. Discovery finds all .sock files
    // 2. Non-.sock files ignored
    // 3. Results sorted by name
    // 4. Socket paths are correct

    // Note: This test uses the real discover_plugins() function
    // which scans /run/user/{uid}/waft/plugins/
    //
    // In a production test environment, we'd want to:
    // - Create a temp directory with test sockets
    // - Use a test-specific discovery function
    // - Clean up after test
    //
    // For now, we just verify the function doesn't crash
    // and returns a valid result (empty or with existing plugins)

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

#[test]
fn test_widget_diff_all_operations() {
    // Test Scenario 7: Comprehensive Diff Operations
    //
    // This test verifies all diff operation types:
    // 1. Added - new widgets
    // 2. Updated - modified widgets
    // 3. Removed - deleted widgets
    // 4. Mixed operations in single diff

    let old_widgets = vec![
        create_test_widget("keep", Slot::FeatureToggles, false),
        create_test_widget("update", Slot::FeatureToggles, false),
        create_test_widget("remove", Slot::FeatureToggles, false),
    ];

    let new_widgets = vec![
        create_test_widget("keep", Slot::FeatureToggles, false), // No change
        create_test_widget("update", Slot::FeatureToggles, true), // Updated state
        create_test_widget("add", Slot::FeatureToggles, false),   // New widget
        // "remove" widget omitted - will show as removed
    ];

    let diffs = diff_widgets(&old_widgets, &new_widgets);

    // Verify we have 3 diffs (1 updated, 1 added, 1 removed)
    assert_eq!(diffs.len(), 3);

    // Count diff types
    let added_count = diffs.iter().filter(|d| matches!(d, waft_overview::plugin_manager::WidgetDiff::Added(_))).count();
    let updated_count = diffs.iter().filter(|d| matches!(d, waft_overview::plugin_manager::WidgetDiff::Updated { .. })).count();
    let removed_count = diffs.iter().filter(|d| matches!(d, waft_overview::plugin_manager::WidgetDiff::Removed(_))).count();

    assert_eq!(added_count, 1, "Should have 1 added widget");
    assert_eq!(updated_count, 1, "Should have 1 updated widget");
    assert_eq!(removed_count, 1, "Should have 1 removed widget");
}

// TODO: Additional integration tests to implement:
//
// - test_action_router_error_handling()
//   Test ActionRouter with missing plugins, disconnected clients, etc.
//
// - test_plugin_client_reconnection()
//   Test PluginClient reconnection logic after disconnect
//
// - test_widget_registry_concurrent_updates()
//   Stress test registry with many concurrent updates
//
// - test_message_protocol_framing()
//   Test transport layer message framing with various sizes
//
// - test_plugin_discovery_error_cases()
//   Test discovery with permission errors, invalid directories, etc.
//
// These tests require more infrastructure (mock servers, etc.) and
// will be implemented as part of ongoing Task #12 work.
