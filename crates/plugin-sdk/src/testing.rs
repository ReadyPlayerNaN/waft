//! Test utilities for plugin daemon integration testing.
//!
//! This module provides helpers for writing integration tests that involve
//! real plugin daemon processes communicating via Unix sockets.

use crate::daemon::PluginDaemon;
use crate::server::PluginServer;
use std::path::PathBuf;
use std::time::Duration;
use waft_ipc::{Action, NamedWidget};

/// A simple mock plugin daemon for testing.
///
/// This daemon has configurable state and can be used to test
/// basic plugin functionality without complex business logic.
pub struct MockPluginDaemon {
    /// Plugin name
    pub name: String,
    /// Current widgets
    pub widgets: Vec<NamedWidget>,
    /// Action handler callback
    pub on_action: Option<Box<dyn Fn(String, Action) + Send + Sync>>,
}

impl MockPluginDaemon {
    /// Create a new mock daemon with the given name and initial widgets.
    pub fn new(name: impl Into<String>, widgets: Vec<NamedWidget>) -> Self {
        Self {
            name: name.into(),
            widgets,
            on_action: None,
        }
    }

    /// Set a custom action handler.
    pub fn with_action_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String, Action) + Send + Sync + 'static,
    {
        self.on_action = Some(Box::new(handler));
        self
    }

    /// Update the widget list (for testing dynamic updates).
    pub fn set_widgets(&mut self, widgets: Vec<NamedWidget>) {
        self.widgets = widgets;
    }
}

#[async_trait::async_trait]
impl PluginDaemon for MockPluginDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        self.widgets.clone()
    }

    async fn handle_action(
        &self,
        widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(handler) = &self.on_action {
            handler(widget_id, action);
        }
        Ok(())
    }
}

/// A simple test plugin with a single toggle widget.
pub struct TestPlugin {
    enabled: std::sync::Mutex<bool>,
}

impl TestPlugin {
    pub fn new() -> Self {
        Self {
            enabled: std::sync::Mutex::new(false),
        }
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.lock().unwrap()
    }
}

impl Default for TestPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PluginDaemon for TestPlugin {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        use waft_ipc::{ActionParams, Widget};

        let enabled = self.is_enabled();
        vec![NamedWidget {
            id: "test:toggle".into(),
            weight: 100,
            widget: Widget::FeatureToggle {
                title: "Test Plugin".into(),
                icon: "emblem-system-symbolic".into(),
                details: Some(if enabled {
                    "Enabled".into()
                } else {
                    "Disabled".into()
                }),
                active: enabled,
                busy: false,
                expandable: false,
                expanded_content: None,
                on_toggle: waft_ipc::Action {
                    id: "toggle".into(),
                    params: ActionParams::None,
                },
            },
        }]
    }

    async fn handle_action(
        &self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action.id == "toggle" {
            let mut enabled = self.enabled.lock().unwrap();
            *enabled = !*enabled;
        }
        Ok(())
    }
}

/// Generate a unique socket path for testing.
///
/// Uses thread ID and timestamp to ensure uniqueness across parallel tests.
/// Socket is placed in /tmp for easy cleanup.
pub fn unique_test_socket_path(prefix: &str) -> PathBuf {
    let thread_id = std::thread::current().id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    PathBuf::from(format!("/tmp/waft-test-{}-{:?}-{}.sock", prefix, thread_id, timestamp))
}

/// Get the socket path for a test plugin.
///
/// Returns the path where the plugin socket should be created:
/// `{XDG_RUNTIME_DIR}/waft/plugins/{name}.sock`
///
/// Can be overridden with `WAFT_PLUGIN_SOCKET_PATH` environment variable.
///
/// This is a convenience re-export of [`crate::plugin_socket_path`] for use in tests.
pub fn test_socket_path(plugin_name: &str) -> PathBuf {
    crate::plugin_socket_path(plugin_name)
}

/// Wait for a socket file to exist.
///
/// Polls the filesystem up to `max_wait` duration, checking every 10ms.
/// Returns `Ok(())` if the socket appears, `Err(())` if timeout.
pub async fn wait_for_socket(socket_path: &PathBuf, max_wait: Duration) -> Result<(), ()> {
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(10);

    while start.elapsed() < max_wait {
        if socket_path.exists() {
            return Ok(());
        }
        tokio::time::sleep(poll_interval).await;
    }

    Err(())
}

/// Clean up test socket files.
///
/// Removes socket files for test plugins to ensure clean test state.
pub fn cleanup_test_sockets(plugin_names: &[&str]) {
    for name in plugin_names {
        let socket_path = test_socket_path(name);
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
    }
}

/// Spawn a plugin daemon in a background task.
///
/// This starts the plugin server in a tokio task and returns a handle
/// that can be used to stop it. The socket path is also returned.
///
/// # Example
///
/// ```no_run
/// use waft_plugin_sdk::testing::*;
///
/// # async fn test() {
/// let daemon = TestPlugin::new();
/// let (handle, socket_path) = spawn_test_plugin("test", daemon).await;
///
/// // Wait for socket
/// wait_for_socket(&socket_path, std::time::Duration::from_secs(1)).await.unwrap();
///
/// // Do testing...
///
/// // Stop plugin
/// handle.abort();
/// # }
/// ```
pub async fn spawn_test_plugin<D>(
    plugin_name: &str,
    daemon: D,
) -> (tokio::task::JoinHandle<()>, PathBuf)
where
    D: PluginDaemon + 'static,
{
    let socket_path = test_socket_path(plugin_name);
    let plugin_name = plugin_name.to_string();

    let handle = tokio::spawn(async move {
        let (server, _notifier) = PluginServer::new(plugin_name, daemon);
        if let Err(e) = server.run().await {
            log::error!("Test plugin server error: {}", e);
        }
    });

    (handle, socket_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_ipc::{ActionParams, Widget};

    #[test]
    fn test_mock_daemon_creation() {
        let widgets = vec![NamedWidget {
            id: "test".into(),
            weight: 10,
            widget: Widget::Label {
                text: "Test".into(),
                css_classes: vec![],
            },
        }];

        let daemon = MockPluginDaemon::new("test", widgets.clone());
        assert_eq!(daemon.name, "test");
        assert_eq!(daemon.get_widgets().len(), 1);
    }

    #[tokio::test]
    async fn test_mock_daemon_action_handler() {
        use std::sync::{Arc, Mutex};

        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();

        let daemon = MockPluginDaemon::new("test", vec![]).with_action_handler(move |_, _| {
            *called_clone.lock().unwrap() = true;
        });

        let action = Action {
            id: "test".into(),
            params: ActionParams::None,
        };

        daemon.handle_action("widget".into(), action).await.unwrap();
        assert!(*called.lock().unwrap());
    }

    #[test]
    fn test_plugin_creation() {
        let plugin = TestPlugin::new();
        assert!(!plugin.is_enabled());

        let widgets = plugin.get_widgets();
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].id, "test:toggle");
    }

    #[tokio::test]
    async fn test_plugin_toggle_action() {
        let plugin = TestPlugin::new();
        assert!(!plugin.is_enabled());

        let action = Action {
            id: "toggle".into(),
            params: ActionParams::None,
        };

        plugin.handle_action("test:toggle".into(), action.clone()).await.unwrap();
        assert!(plugin.is_enabled());

        plugin.handle_action("test:toggle".into(), action).await.unwrap();
        assert!(!plugin.is_enabled());
    }

    #[test]
    fn test_socket_path_generation() {
        // Ensure env var is not set
        unsafe {
            std::env::remove_var("WAFT_PLUGIN_SOCKET_PATH");
        }

        let path = test_socket_path("test-plugin");
        assert!(path.to_string_lossy().contains("waft/plugins/test-plugin.sock"));
    }

    #[test]
    fn test_socket_path_env_override() {
        // Set custom socket path via env var
        unsafe {
            std::env::set_var("WAFT_PLUGIN_SOCKET_PATH", "/tmp/custom-override.sock");
        }

        let path = test_socket_path("test-plugin");
        assert_eq!(path, PathBuf::from("/tmp/custom-override.sock"));

        // Clean up
        unsafe {
            std::env::remove_var("WAFT_PLUGIN_SOCKET_PATH");
        }
    }

    #[test]
    fn test_unique_test_socket_path() {
        let path1 = unique_test_socket_path("plugin1");
        let path2 = unique_test_socket_path("plugin2");

        // Paths should be different
        assert_ne!(path1, path2);

        // Both should be in /tmp
        assert!(path1.starts_with("/tmp"));
        assert!(path2.starts_with("/tmp"));

        // Should contain the prefix
        assert!(path1.to_string_lossy().contains("plugin1"));
        assert!(path2.to_string_lossy().contains("plugin2"));
    }

    #[tokio::test]
    async fn test_wait_for_socket_timeout() {
        let socket_path = PathBuf::from("/tmp/nonexistent-socket-12345.sock");
        let result = wait_for_socket(&socket_path, Duration::from_millis(50)).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_cleanup_test_sockets() {
        // This test just verifies the function doesn't panic
        cleanup_test_sockets(&["test1", "test2"]);
    }

    #[test]
    fn test_mock_daemon_set_widgets() {
        let mut daemon = MockPluginDaemon::new("test", vec![]);
        assert_eq!(daemon.get_widgets().len(), 0);

        let widgets = vec![NamedWidget {
            id: "widget1".into(),
            weight: 20,
            widget: Widget::Label {
                text: "New".into(),
                css_classes: vec![],
            },
        }];

        daemon.set_widgets(widgets);
        assert_eq!(daemon.get_widgets().len(), 1);
        assert_eq!(daemon.get_widgets()[0].id, "widget1");
    }

    #[test]
    fn test_default_impl() {
        let plugin = TestPlugin::default();
        assert!(!plugin.is_enabled());
    }
}
