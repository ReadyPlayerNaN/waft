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
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};

use waft_ipc::{Action, ActionParams, PluginMessage, Widget};
use waft_overview::plugin_manager::{
    InternalMessage, PluginClient, PluginManager, PluginManagerConfig, PluginUpdate,
};

/// Initialize test environment with appropriate log filtering
fn init_test_logging() {
    unsafe {
        std::env::set_var(
            "RUST_LOG",
            "waft_plugin_sdk::server=warn,waft_overview=info,simple_plugin=info",
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
        let thread_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let unique_name = format!("test-plugin-{:?}-{}", thread_id, timestamp);

        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));

        let mut socket_dir = PathBuf::from(runtime_dir);
        socket_dir.push("waft");
        socket_dir.push("plugins");

        std::fs::create_dir_all(&socket_dir)
            .map_err(|e| format!("Failed to create plugin directory: {}", e))?;

        let socket_path = socket_dir.join(format!("{}.sock", unique_name));

        let mut cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_target.pop();
        cargo_target.pop();
        cargo_target.push("target");
        cargo_target.push("debug");
        cargo_target.push("examples");
        cargo_target.push("simple_plugin");

        let child = Command::new(cargo_target)
            .env("WAFT_PLUGIN_SOCKET_PATH", &socket_path)
            .env(
                "RUST_LOG",
                "waft_plugin_sdk::server=warn,simple_plugin=info",
            )
            .spawn()
            .map_err(|e| format!("Failed to start simple_plugin: {}", e))?;

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
        Err("Plugin socket did not appear within 5 seconds".into())
    }
}

impl Drop for PluginDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();

        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[tokio::test]
async fn test_plugin_discovery_and_connection() {
    init_test_logging();

    let daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    sleep(Duration::from_millis(100)).await;

    let (mut manager, mut update_rx, _action_tx) = PluginManager::new(PluginManagerConfig::default());

    tokio::spawn(async move {
        manager.run().await;
    });

    let mut connected_count = 0;

    loop {
        let update = timeout(Duration::from_secs(5), update_rx.recv())
            .await
            .expect("Timeout waiting for update")
            .expect("Channel closed");

        match update {
            PluginUpdate::PluginConnected { plugin_id } => {
                if plugin_id.starts_with("test-plugin-") {
                    connected_count += 1;
                    println!("Test plugin connected: {}", plugin_id);
                } else {
                    println!("Non-test plugin connected (ignored): {}", plugin_id);
                }
            }
            PluginUpdate::FullUpdate { widgets } => {
                // Wait for a FullUpdate that actually contains our test widget
                // (other daemons like clock/darkman may send FullUpdates first)
                if let Some(toggle) = widgets.iter().find(|w| w.id == "simple:toggle") {
                    println!(
                        "Found simple:toggle in FullUpdate ({} total widgets)",
                        widgets.len()
                    );

                    assert_eq!(toggle.weight, 100);

                    match &toggle.widget {
                        Widget::FeatureToggle {
                            title,
                            icon,
                            ..
                        } => {
                            assert_eq!(title, "Simple Plugin");
                            assert_eq!(icon, "emblem-system-symbolic");
                        }
                        _ => panic!("Expected FeatureToggle widget"),
                    }
                    break;
                }
            }
            PluginUpdate::Error { .. } => {
                // Connection errors to other parallel test sockets — expected
            }
            other => {
                println!("Received update: {:?}", other);
            }
        }
    }

    println!("Plugin discovery and connection successful");
    println!("Widget retrieval successful");
}

#[tokio::test]
async fn test_plugin_action_routing() {
    init_test_logging();

    let daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect with the new event-driven client
    let (merged_tx, mut merged_rx) = mpsc::unbounded_channel();
    let client =
        PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone(), merged_tx)
            .await
            .expect("Failed to connect to plugin");

    // Request widgets
    client.send_get_widgets().expect("Failed to send GetWidgets");

    // Wait for response via merged channel
    let msg = timeout(Duration::from_secs(5), merged_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match msg {
        InternalMessage::Plugin { msg: PluginMessage::SetWidgets { widgets }, .. } => {
            assert!(!widgets.is_empty(), "Should have widgets");
            println!("Got {} widgets from plugin", widgets.len());
        }
        other => panic!("Expected SetWidgets, got: {:?}", other),
    }

    // Trigger toggle action
    let action = Action {
        id: "toggle".to_string(),
        params: ActionParams::None,
    };

    client
        .send_action("simple:toggle".to_string(), action)
        .expect("Failed to send action");

    // Wait for updated widgets (server pushes SetWidgets after action)
    let msg = timeout(Duration::from_secs(5), merged_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match msg {
        InternalMessage::Plugin { msg: PluginMessage::SetWidgets { widgets }, .. } => {
            let toggle = widgets
                .iter()
                .find(|w| w.id == "simple:toggle")
                .expect("Should have simple:toggle widget");

            match &toggle.widget {
                Widget::FeatureToggle { active, .. } => {
                    assert!(*active, "Widget should be active after toggle");
                }
                _ => panic!("Expected FeatureToggle widget"),
            }
        }
        other => panic!("Expected SetWidgets after action, got: {:?}", other),
    }

    println!("Action routing successful");
    println!("State update verified");
}

#[tokio::test]
async fn test_plugin_reconnection() {
    init_test_logging();

    let mut daemon = PluginDaemon::start().expect("Failed to start plugin daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect with new API
    let (merged_tx, mut merged_rx) = mpsc::unbounded_channel();
    let client =
        PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone(), merged_tx)
            .await
            .expect("Failed to connect to plugin");

    // Get widgets
    client.send_get_widgets().expect("Failed to send GetWidgets");
    let msg = timeout(Duration::from_secs(5), merged_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match msg {
        InternalMessage::Plugin { msg: PluginMessage::SetWidgets { widgets }, .. } => {
            assert!(!widgets.is_empty(), "Should have widgets");
        }
        other => panic!("Expected SetWidgets, got: {:?}", other),
    }
    println!("Initial connection successful");

    // Kill the plugin daemon
    let socket_path = daemon.socket_path.clone();
    daemon.child.kill().expect("Failed to kill daemon");
    daemon.child.wait().expect("Failed to wait for daemon");
    std::fs::remove_file(&socket_path).ok();

    println!("Plugin stopped");

    // Restart daemon
    daemon = PluginDaemon::start().expect("Failed to restart daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Plugin socket did not appear after restart");

    sleep(Duration::from_millis(200)).await;

    // Reconnect with new client
    let (merged_tx2, mut merged_rx2) = mpsc::unbounded_channel();
    let client2 =
        PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone(), merged_tx2)
            .await
            .expect("Failed to reconnect to plugin");

    client2.send_get_widgets().expect("Failed to send GetWidgets");
    let msg = timeout(Duration::from_secs(5), merged_rx2.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match msg {
        InternalMessage::Plugin { msg: PluginMessage::SetWidgets { widgets }, .. } => {
            assert!(!widgets.is_empty(), "Should have widgets after reconnect");
        }
        other => panic!("Expected SetWidgets after reconnect, got: {:?}", other),
    }

    println!("Plugin reconnection successful");
}

/// Helper to start the clock daemon for testing
struct ClockDaemon {
    child: Child,
    socket_path: PathBuf,
    plugin_id: String,
}

impl ClockDaemon {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let thread_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let unique_name = format!("clock-daemon-test-{:?}-{}", thread_id, timestamp);

        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));

        let mut socket_dir = PathBuf::from(runtime_dir);
        socket_dir.push("waft");
        socket_dir.push("plugins");

        std::fs::create_dir_all(&socket_dir)
            .map_err(|e| format!("Failed to create plugin directory: {}", e))?;

        let socket_path = socket_dir.join(format!("{}.sock", unique_name));

        let mut cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_target.pop();
        cargo_target.pop();
        cargo_target.push("target");
        cargo_target.push("debug");
        cargo_target.push("waft-clock-daemon");

        let child = Command::new(cargo_target)
            .env("WAFT_PLUGIN_SOCKET_PATH", &socket_path)
            .env(
                "RUST_LOG",
                "waft_plugin_sdk::server=warn,waft_plugin_clock=info",
            )
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

    let daemon = ClockDaemon::start().expect("Failed to start clock daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Clock daemon socket did not appear");

    sleep(Duration::from_millis(200)).await;

    let (merged_tx, mut merged_rx) = mpsc::unbounded_channel();
    let client =
        PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone(), merged_tx)
            .await
            .expect("Failed to connect to clock daemon");

    client.send_get_widgets().expect("Failed to send GetWidgets");

    let msg = timeout(Duration::from_secs(5), merged_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match msg {
        InternalMessage::Plugin { msg: PluginMessage::SetWidgets { widgets }, .. } => {
            assert!(!widgets.is_empty(), "Clock should have widgets");
            println!("Clock daemon discovery successful");

            let clock_widget = widgets
                .iter()
                .find(|w| w.id.starts_with("clock:"))
                .expect("Should have clock widget");

            match &clock_widget.widget {
                Widget::Container {
                    orientation,
                    children,
                    ..
                } => {
                    assert_eq!(*orientation, waft_ipc::Orientation::Vertical);
                    assert_eq!(children.len(), 2, "Clock should have date and time labels");

                    assert!(
                        matches!(children[0].widget, Widget::Label { .. }),
                        "First child should be date label"
                    );
                    assert!(
                        matches!(children[1].widget, Widget::Label { .. }),
                        "Second child should be time label"
                    );

                    println!("Clock widget structure verified");
                }
                _ => panic!("Clock widget should be a Container"),
            }
        }
        other => panic!("Expected SetWidgets, got: {:?}", other),
    }
}

#[tokio::test]
#[ignore] // Only run if clock daemon binary is built
async fn test_clock_daemon_click_action() {
    init_test_logging();

    let daemon = ClockDaemon::start().expect("Failed to start clock daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Clock daemon socket did not appear");

    sleep(Duration::from_millis(200)).await;

    let (merged_tx, mut merged_rx) = mpsc::unbounded_channel();
    let client =
        PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone(), merged_tx)
            .await
            .expect("Failed to connect to clock daemon");

    // Get widgets first
    client.send_get_widgets().expect("Failed to send GetWidgets");
    let msg = timeout(Duration::from_secs(5), merged_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    let clock_widget_id = match msg {
        InternalMessage::Plugin { msg: PluginMessage::SetWidgets { widgets }, .. } => {
            widgets
                .iter()
                .find(|w| w.id.starts_with("clock:"))
                .expect("Should have clock widget")
                .id
                .clone()
        }
        other => panic!("Expected SetWidgets, got: {:?}", other),
    };

    // Trigger click action
    let action = Action {
        id: "click".to_string(),
        params: ActionParams::None,
    };

    let result = client.send_action(clock_widget_id, action);
    assert!(result.is_ok(), "Click action should succeed: {:?}", result);
    println!("Clock click action successful");
}
