//! Integration test for end-to-end plugin IPC communication.
//!
//! This test verifies the complete flow:
//! 1. Plugin daemon starts and creates socket
//! 2. PluginManager discovers the plugin
//! 3. PluginManager connects and retrieves widgets
//! 4. Actions can be sent to the plugin
//! 5. Plugin state updates are reflected in widgets
//!
//! Note: Tests run in parallel and may discover each other's plugin sockets.
//! Connection failures to other test plugins are expected and logged at WARN level.

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::time::{sleep, timeout};

use waft_ipc::{Action, ActionParams, Slot, Widget};
use waft_overview::plugin_manager::{PluginManager, PluginManagerConfig, PluginUpdate};

/// Initialize test environment with appropriate log filtering
fn init_test_logging() {
    // Set log level to suppress expected "early eof" errors during parallel tests
    // These occur when tests discover each other's plugin sockets
    unsafe {
        std::env::set_var(
            "RUST_LOG",
            "waft_plugin_sdk::server=warn,waft_overview=info,simple_plugin=info"
        );
    }
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Helper to start the simple_plugin daemon as a child process
struct PluginDaemon {
    child: Child,
    socket_path: PathBuf,
    plugin_id: String,
}

impl PluginDaemon {
    /// Start a plugin daemon with a unique socket path for testing
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique socket path using thread ID and timestamp
        let thread_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let unique_name = format!("test-plugin-{:?}-{}", thread_id, timestamp);

        // Get runtime directory (same as production)
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));

        // Build socket path in the standard waft plugins directory
        let mut socket_dir = PathBuf::from(runtime_dir);
        socket_dir.push("waft");
        socket_dir.push("plugins");

        // Ensure the directory exists
        std::fs::create_dir_all(&socket_dir)
            .map_err(|e| format!("Failed to create plugin directory: {}", e))?;

        let socket_path = socket_dir.join(format!("{}.sock", unique_name));

        // Build path to simple_plugin example
        let mut cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_target.pop(); // Go up from overview
        cargo_target.pop(); // Go up from crates
        cargo_target.push("target");
        cargo_target.push("debug");
        cargo_target.push("examples");
        cargo_target.push("simple_plugin");

        // Start the plugin daemon with custom socket path
        // Suppress error logs about connection failures (expected in parallel tests)
        let child = Command::new(cargo_target)
            .env("WAFT_PLUGIN_SOCKET_PATH", &socket_path)
            .env("RUST_LOG", "waft_plugin_sdk::server=warn,simple_plugin=info")
            .spawn()
            .map_err(|e| format!("Failed to start simple_plugin: {}", e))?;

        // Extract plugin_id from socket filename (matches discovery logic)
        let plugin_id = socket_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid socket path")?
            .to_string();

        Ok(Self {
            child,
            socket_path,
            plugin_id,
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
    init_test_logging();

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

    // Wait for plugin connected event(s) - may receive multiple if running in parallel
    let mut connected_count = 0;

    // Consume updates until we get a FullUpdate
    loop {
        let update = timeout(Duration::from_secs(5), update_rx.recv())
            .await
            .expect("Timeout waiting for update")
            .expect("Channel closed");

        match update {
            PluginUpdate::PluginConnected { plugin_id } => {
                // In parallel tests, the manager may discover any of the test plugins
                assert!(plugin_id.starts_with("test-plugin-"), "Should connect to a test plugin");
                connected_count += 1;
                println!("✅ Plugin connected: {}", plugin_id);
            }
            PluginUpdate::FullUpdate { widgets } => {
                assert!(connected_count > 0, "Should have seen at least one PluginConnected before FullUpdate");
                println!("✅ Received FullUpdate after {} connections", connected_count);
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
                break; // Exit loop after processing FullUpdate
            }
            other => panic!("Unexpected update: {:?}", other),
        }
    }

    println!("✅ Plugin discovery and connection successful");
    println!("✅ Widget retrieval successful");
}

#[tokio::test]
async fn test_plugin_action_routing() {
    init_test_logging();

    // Start plugin daemon
    let daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Create PluginClient directly (simpler than going through PluginManager)
    use waft_overview::PluginClient;
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
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
    init_test_logging();

    // Start plugin daemon
    let mut daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect to plugin
    use waft_overview::PluginClient;
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
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
    let socket_path = daemon.socket_path.clone();
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

    // Reconnect with the new daemon's plugin_id and socket
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
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

/// Helper to start the clock daemon for testing
struct ClockDaemon {
    child: Child,
    socket_path: PathBuf,
    plugin_id: String,
}

impl ClockDaemon {
    /// Start the clock daemon with a unique socket path
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique socket path
        let thread_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let unique_name = format!("clock-daemon-test-{:?}-{}", thread_id, timestamp);

        // Get runtime directory
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));

        let mut socket_dir = PathBuf::from(runtime_dir);
        socket_dir.push("waft");
        socket_dir.push("plugins");

        std::fs::create_dir_all(&socket_dir)
            .map_err(|e| format!("Failed to create plugin directory: {}", e))?;

        let socket_path = socket_dir.join(format!("{}.sock", unique_name));

        // Build path to clock daemon binary
        let mut cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_target.pop(); // Go up from overview
        cargo_target.pop(); // Go up from crates
        cargo_target.push("target");
        cargo_target.push("debug");
        cargo_target.push("waft-clock-daemon");

        // Start the clock daemon
        let child = Command::new(cargo_target)
            .env("WAFT_PLUGIN_SOCKET_PATH", &socket_path)
            .env("RUST_LOG", "waft_plugin_sdk::server=warn,waft_plugin_clock=info")
            .spawn()
            .map_err(|e| format!("Failed to start waft-clock-daemon: {}", e))?;

        let plugin_id = socket_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid socket path")?
            .to_string();

        Ok(Self {
            child,
            socket_path,
            plugin_id,
        })
    }

    async fn wait_for_socket(&self) -> Result<(), Box<dyn std::error::Error>> {
        for _ in 0..50 {
            if self.socket_path.exists() {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
        Err("Clock daemon socket did not appear within 5 seconds".into())
    }
}

impl Drop for ClockDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[tokio::test]
#[ignore] // Only run if clock daemon binary is built
async fn test_clock_daemon_discovery() {
    init_test_logging();

    // Start clock daemon
    let daemon = ClockDaemon::start().expect("Failed to start clock daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Clock daemon socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect to clock daemon
    use waft_overview::PluginClient;
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
        .await
        .expect("Failed to connect to clock daemon");

    // Request widgets
    let widgets = client
        .request_widgets()
        .await
        .expect("Failed to get clock widgets");

    assert!(!widgets.is_empty(), "Clock should have widgets");
    println!("✅ Clock daemon discovery successful");

    // Find the clock widget
    let clock_widget = widgets
        .iter()
        .find(|w| w.id.starts_with("clock:"))
        .expect("Should have clock widget");

    assert_eq!(clock_widget.slot, Slot::Header);
    println!("✅ Clock widget in correct slot (Header)");

    // Verify widget structure
    match &clock_widget.widget {
        Widget::Container {
            orientation,
            children,
            ..
        } => {
            assert_eq!(*orientation, waft_ipc::Orientation::Vertical);
            assert_eq!(children.len(), 2, "Clock should have date and time labels");

            // Verify children are labels
            assert!(matches!(children[0], Widget::Label { .. }), "First child should be date label");
            assert!(matches!(children[1], Widget::Label { .. }), "Second child should be time label");

            println!("✅ Clock widget structure verified");
        }
        _ => panic!("Clock widget should be a Container"),
    }
}

#[tokio::test]
#[ignore] // Only run if clock daemon binary is built
async fn test_clock_daemon_click_action() {
    init_test_logging();

    // Start clock daemon
    let daemon = ClockDaemon::start().expect("Failed to start clock daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Clock daemon socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect to clock daemon
    use waft_overview::PluginClient;
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
        .await
        .expect("Failed to connect to clock daemon");

    // Get widgets
    let widgets = client
        .request_widgets()
        .await
        .expect("Failed to get widgets");

    let clock_widget = widgets
        .iter()
        .find(|w| w.id.starts_with("clock:"))
        .expect("Should have clock widget");

    // Trigger click action
    let action = Action {
        id: "click".to_string(),
        params: ActionParams::None,
    };

    let result = client
        .trigger_action(clock_widget.id.clone(), action)
        .await;

    // Action should succeed (even if on_click is not configured)
    assert!(result.is_ok(), "Click action should succeed: {:?}", result);
    println!("✅ Clock click action successful");
}
