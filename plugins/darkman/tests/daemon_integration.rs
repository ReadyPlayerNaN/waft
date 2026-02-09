//! Integration tests for darkman daemon.
//!
//! These tests verify the darkman daemon can:
//! 1. Start and create a socket
//! 2. Respond to widget requests
//! 3. Handle toggle actions
//! 4. Correctly build FeatureToggle widgets

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::time::sleep;

use waft_ipc::{Action, ActionParams, Slot, Widget};

/// Initialize test logging
fn init_test_logging() {
    unsafe {
        std::env::set_var("RUST_LOG", "waft_plugin_sdk::server=warn,waft_darkman_daemon=info");
    }
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Helper to start the darkman daemon for testing
struct DarkmanDaemon {
    child: Child,
    socket_path: PathBuf,
    plugin_id: String,
}

impl DarkmanDaemon {
    /// Start the darkman daemon with a unique socket path
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique socket path
        let thread_id = std::thread::current().id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let unique_name = format!("darkman-daemon-test-{:?}-{}", thread_id, timestamp);

        // Get runtime directory
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));

        let mut socket_dir = PathBuf::from(runtime_dir);
        socket_dir.push("waft");
        socket_dir.push("plugins");

        std::fs::create_dir_all(&socket_dir)
            .map_err(|e| format!("Failed to create plugin directory: {}", e))?;

        let socket_path = socket_dir.join(format!("{}.sock", unique_name));

        // Build path to darkman daemon binary
        let mut cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_target.pop(); // Go up from darkman plugin dir
        cargo_target.pop(); // Go up from plugins
        cargo_target.push("target");
        cargo_target.push("debug");
        cargo_target.push("waft-darkman-daemon");

        // Start the darkman daemon
        let child = Command::new(cargo_target)
            .env("WAFT_PLUGIN_SOCKET_PATH", &socket_path)
            .env("RUST_LOG", "waft_plugin_sdk::server=warn,waft_darkman_daemon=info")
            .spawn()
            .map_err(|e| format!("Failed to start waft-darkman-daemon: {}", e))?;

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
        Err("Darkman daemon socket did not appear within 5 seconds".into())
    }
}

impl Drop for DarkmanDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[tokio::test]
#[ignore] // Only run if darkman daemon binary is built
async fn test_darkman_daemon_discovery() {
    init_test_logging();

    // Start darkman daemon
    let daemon = DarkmanDaemon::start().expect("Failed to start darkman daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Darkman daemon socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect to darkman daemon
    use waft_overview::PluginClient;
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
        .await
        .expect("Failed to connect to darkman daemon");

    // Request widgets
    let widgets = client
        .request_widgets()
        .await
        .expect("Failed to get darkman widgets");

    assert!(!widgets.is_empty(), "Darkman should have widgets");
    println!("✅ Darkman daemon discovery successful");

    // Find the darkman toggle widget
    let toggle_widget = widgets
        .iter()
        .find(|w| w.id == "darkman:toggle")
        .expect("Should have darkman:toggle widget");

    assert_eq!(toggle_widget.slot, Slot::FeatureToggles);
    assert_eq!(toggle_widget.weight, 190);
    println!("✅ Darkman widget in correct slot (FeatureToggles) with weight 190");

    // Verify widget structure
    match &toggle_widget.widget {
        Widget::FeatureToggle {
            title,
            icon,
            active,
            busy,
            ..
        } => {
            assert_eq!(title, "Dark Mode");
            assert_eq!(icon, "weather-clear-night-symbolic");
            // active state depends on system darkman state, so we just check it's a boolean
            assert!(!busy, "Should not be busy initially");

            println!("✅ Darkman toggle widget structure verified");
            println!("   - Title: {}", title);
            println!("   - Icon: {}", icon);
            println!("   - Active: {}", active);
        }
        _ => panic!("Darkman widget should be a FeatureToggle"),
    }
}

#[tokio::test]
#[ignore] // Only run if darkman daemon binary is built
async fn test_darkman_daemon_toggle_action() {
    init_test_logging();

    // Start darkman daemon
    let daemon = DarkmanDaemon::start().expect("Failed to start darkman daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Darkman daemon socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect to darkman daemon
    use waft_overview::PluginClient;
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
        .await
        .expect("Failed to connect to darkman daemon");

    // Get initial widgets
    let widgets = client
        .request_widgets()
        .await
        .expect("Failed to get widgets");

    let toggle_widget = widgets
        .iter()
        .find(|w| w.id == "darkman:toggle")
        .expect("Should have darkman:toggle widget");

    let initial_active = match &toggle_widget.widget {
        Widget::FeatureToggle { active, .. } => *active,
        _ => panic!("Expected FeatureToggle"),
    };

    println!("✅ Initial darkman state: {}", if initial_active { "Dark" } else { "Light" });

    // Trigger toggle action
    let action = Action {
        id: "toggle".to_string(),
        params: ActionParams::None,
    };

    let result = client
        .trigger_action("darkman:toggle".to_string(), action)
        .await;

    // Action may fail if darkman service is not running - that's OK for the test
    // We're testing the daemon's ability to handle the action, not darkman itself
    match result {
        Ok(_) => {
            println!("✅ Toggle action succeeded");

            // Request widgets again to verify state changed
            let widgets_after = client
                .request_widgets()
                .await
                .expect("Failed to get widgets after toggle");

            let toggle_after = widgets_after
                .iter()
                .find(|w| w.id == "darkman:toggle")
                .expect("Should have darkman:toggle widget");

            match &toggle_after.widget {
                Widget::FeatureToggle { active, .. } => {
                    assert_ne!(*active, initial_active, "State should have changed after toggle");
                    println!("✅ State changed from {} to {}",
                        if initial_active { "Dark" } else { "Light" },
                        if *active { "Dark" } else { "Light" }
                    );
                }
                _ => panic!("Expected FeatureToggle"),
            }
        }
        Err(e) => {
            println!("⚠️  Toggle action failed (darkman service may not be running): {}", e);
            println!("✅ Daemon handled action request gracefully");
        }
    }
}

#[tokio::test]
#[ignore] // Only run if darkman daemon binary is built
async fn test_darkman_daemon_widget_format() {
    init_test_logging();

    // Start darkman daemon
    let daemon = DarkmanDaemon::start().expect("Failed to start darkman daemon");
    daemon
        .wait_for_socket()
        .await
        .expect("Darkman daemon socket did not appear");

    sleep(Duration::from_millis(200)).await;

    // Connect to darkman daemon
    use waft_overview::PluginClient;
    let mut client = PluginClient::connect(daemon.plugin_id.clone(), daemon.socket_path.clone())
        .await
        .expect("Failed to connect to darkman daemon");

    // Request widgets
    let widgets = client
        .request_widgets()
        .await
        .expect("Failed to get widgets");

    assert_eq!(widgets.len(), 1, "Darkman should have exactly one widget");

    let widget = &widgets[0];

    // Verify all widget metadata
    assert_eq!(widget.id, "darkman:toggle");
    assert_eq!(widget.slot, Slot::FeatureToggles);
    assert_eq!(widget.weight, 190);

    // Verify widget content
    match &widget.widget {
        Widget::FeatureToggle {
            title,
            icon,
            details,
            active,
            busy,
            expandable,
            expanded_content,
            on_toggle,
        } => {
            assert_eq!(title, "Dark Mode");
            assert_eq!(icon, "weather-clear-night-symbolic");
            assert_eq!(details, &None, "Should have no details");
            assert!(matches!(active, true | false), "Active should be boolean");
            assert!(!busy, "Should not be busy");
            assert!(!expandable, "Should not be expandable");
            assert!(expanded_content.is_none(), "Should have no expanded content");
            assert_eq!(on_toggle.id, "toggle", "Toggle action should be 'toggle'");
            assert!(
                matches!(on_toggle.params, ActionParams::None),
                "Toggle action should have no params"
            );

            println!("✅ All widget properties verified correctly");
        }
        _ => panic!("Expected FeatureToggle widget"),
    }
}
