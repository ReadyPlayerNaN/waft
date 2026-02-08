//! Integration test for end-to-end plugin IPC communication.
//!
//! This test verifies the complete flow:
//! 1. Plugin daemon starts and creates socket
//! 2. PluginManager discovers the plugin
//! 3. PluginManager connects and retrieves widgets
//! 4. Actions can be sent to the plugin
//! 5. Plugin state updates are reflected in widgets

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::time::{sleep, timeout};

use waft_ipc::{Action, ActionParams, Slot, Widget};
use waft_overview::plugin_manager::{PluginManager, PluginManagerConfig, PluginUpdate};

/// Helper to start the simple_plugin daemon as a child process
struct PluginDaemon {
    child: Child,
    socket_path: PathBuf,
}

impl PluginDaemon {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // Build path to simple_plugin example
        let mut cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_target.pop(); // Go up from overview
        cargo_target.pop(); // Go up from crates
        cargo_target.push("target");
        cargo_target.push("debug");
        cargo_target.push("examples");
        cargo_target.push("simple_plugin");

        // Start the plugin daemon
        let child = Command::new(cargo_target)
            .spawn()
            .map_err(|e| format!("Failed to start simple_plugin: {}", e))?;

        // Get socket path
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
        let socket_path = PathBuf::from(runtime_dir)
            .join("waft")
            .join("plugins")
            .join("simple.sock");

        Ok(Self {
            child,
            socket_path,
        })
    }

    async fn wait_for_socket(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Wait up to 5 seconds for socket to appear
        for _ in 0..50 {
            if self.socket_path.exists() {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
        Err("Plugin socket did not appear within 5 seconds".into())
    }
}

impl Drop for PluginDaemon {
    fn drop(&mut self) {
        // Kill the daemon when test finishes
        let _ = self.child.kill();
        let _ = self.child.wait();

        // Clean up socket
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[tokio::test]
async fn test_plugin_discovery_and_connection() {
    // Start plugin daemon
    let daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    // Give plugin a moment to fully initialize
    sleep(Duration::from_millis(100)).await;

    // Create PluginManager
    let config = PluginManagerConfig {
        poll_interval: Duration::from_secs(10), // Don't poll during test
        reconnect_interval: Duration::from_secs(10),
        auto_reconnect: false,
    };
    let (mut manager, mut update_rx) = PluginManager::new(config);

    // Spawn manager in background
    tokio::spawn(async move {
        manager.run().await;
    });

    // Wait for plugin connected event
    let update = timeout(Duration::from_secs(5), update_rx.recv())
        .await
        .expect("Timeout waiting for update")
        .expect("Channel closed");

    match update {
        PluginUpdate::PluginConnected { plugin_id } => {
            assert_eq!(plugin_id, "simple", "Should connect to simple plugin");
            println!("✅ Plugin connected: {}", plugin_id);
        }
        _ => panic!("Expected PluginConnected, got {:?}", update),
    }

    // Wait for full widget update
    let update = timeout(Duration::from_secs(5), update_rx.recv())
        .await
        .expect("Timeout waiting for full update")
        .expect("Channel closed");

    match update {
        PluginUpdate::FullUpdate { widgets } => {
            assert!(!widgets.is_empty(), "Should have received widgets");

            // Verify the simple plugin's toggle widget
            let toggle = widgets
                .iter()
                .find(|w| w.id == "simple:toggle")
                .expect("Should have simple:toggle widget");

            assert_eq!(toggle.slot, Slot::FeatureToggles);
            assert_eq!(toggle.weight, 100);

            match &toggle.widget {
                Widget::FeatureToggle {
                    title,
                    icon,
                    active,
                    ..
                } => {
                    assert_eq!(title, "Simple Plugin");
                    assert_eq!(icon, "emblem-system-symbolic");
                    assert!(!active, "Initial state should be disabled");
                }
                _ => panic!("Expected FeatureToggle widget"),
            }
        }
        _ => panic!("Expected FullUpdate, got {:?}", update),
    }

    println!("✅ Plugin discovery and connection successful");
    println!("✅ Widget retrieval successful");
}

#[tokio::test]
async fn test_plugin_action_routing() {
    // Start plugin daemon
    let daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Create PluginClient directly (simpler than going through PluginManager)
    use waft_overview::PluginClient;
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    let socket_path = PathBuf::from(runtime_dir)
        .join("waft")
        .join("plugins")
        .join("simple.sock");

    let mut client = PluginClient::connect("simple".to_string(), socket_path)
        .await
        .expect("Failed to connect to plugin");

    // Request initial widgets
    let widgets = client
        .request_widgets()
        .await
        .expect("Failed to get widgets");

    assert!(!widgets.is_empty(), "Should have widgets");
    println!("✅ Got {} widgets from plugin", widgets.len());

    // Trigger toggle action
    let action = Action {
        id: "toggle".to_string(),
        params: ActionParams::None,
    };

    let result = client
        .trigger_action("simple:toggle".to_string(), action)
        .await;

    assert!(result.is_ok(), "Action should succeed: {:?}", result);

    // Request widgets again to verify state changed
    let widgets_after = client
        .request_widgets()
        .await
        .expect("Failed to get widgets after toggle");

    // Find the toggle widget
    let toggle = widgets_after
        .iter()
        .find(|w| w.id == "simple:toggle")
        .expect("Should have simple:toggle widget");

    // Verify it's now active
    match &toggle.widget {
        Widget::FeatureToggle { active, .. } => {
            assert!(*active, "Widget should be active after toggle");
        }
        _ => panic!("Expected FeatureToggle widget"),
    }

    println!("✅ Action routing successful");
    println!("✅ State update verified");
}

#[tokio::test]
async fn test_plugin_reconnection() {
    // Start plugin daemon
    let mut daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect to plugin
    use waft_overview::PluginClient;
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    let socket_path = PathBuf::from(runtime_dir)
        .join("waft")
        .join("plugins")
        .join("simple.sock");

    let mut client = PluginClient::connect("simple".to_string(), socket_path.clone())
        .await
        .expect("Failed to connect to plugin");

    // Get widgets
    let widgets = client
        .request_widgets()
        .await
        .expect("Failed to get widgets");
    assert!(!widgets.is_empty(), "Should have widgets");
    println!("✅ Initial connection successful");

    // Kill the plugin daemon
    daemon.child.kill().expect("Failed to kill daemon");
    daemon.child.wait().expect("Failed to wait for daemon");
    std::fs::remove_file(&socket_path).ok(); // Clean up socket

    println!("✅ Plugin stopped");

    // Restart daemon
    daemon = PluginDaemon::start().expect("Failed to restart daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear after restart");

    sleep(Duration::from_millis(200)).await;

    // Reconnect
    let mut client = PluginClient::connect("simple".to_string(), socket_path)
        .await
        .expect("Failed to reconnect to plugin");

    // Verify we can still get widgets
    let widgets_after = client
        .request_widgets()
        .await
        .expect("Failed to get widgets after reconnect");
    assert!(!widgets_after.is_empty(), "Should have widgets after reconnect");

    println!("✅ Plugin reconnection successful");
}
