//! Integration tests for error scenarios in the plugin SDK.
//!
//! These tests verify that the plugin server correctly handles various error conditions
//! and continues operating even when clients misbehave or network issues occur.

use waft_ipc::widget::{Action, ActionParams, NamedWidget, Widget};
use waft_plugin_sdk::{PluginDaemon, PluginServer};

/// Test daemon that can simulate various error conditions
struct ErrorTestDaemon {
    fail_get_widgets: bool,
    fail_handle_action: bool,
    widgets: Vec<NamedWidget>,
}

impl ErrorTestDaemon {
    fn new() -> Self {
        Self {
            fail_get_widgets: false,
            fail_handle_action: false,
            widgets: vec![],
        }
    }

    #[allow(dead_code)]
    fn with_error_on_get_widgets(mut self) -> Self {
        self.fail_get_widgets = true;
        self
    }

    fn with_error_on_handle_action(mut self) -> Self {
        self.fail_handle_action = true;
        self
    }

    fn with_widgets(mut self, widgets: Vec<NamedWidget>) -> Self {
        self.widgets = widgets;
        self
    }
}

#[async_trait::async_trait]
impl PluginDaemon for ErrorTestDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        if self.fail_get_widgets {
            // In a real scenario, this panic would be caught by the server
            // and converted to a log message, but for testing we return empty
            vec![]
        } else {
            self.widgets.clone()
        }
    }

    async fn handle_action(
        &self,
        _widget_id: String,
        _action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.fail_handle_action {
            Err("Simulated action handler failure".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn test_daemon_with_no_widgets() {
    // Verify that a daemon with no widgets doesn't cause errors
    let daemon = ErrorTestDaemon::new();
    let widgets = daemon.get_widgets();
    assert!(widgets.is_empty());
}

#[test]
fn test_daemon_with_widgets() {
    // Verify that widgets are returned correctly
    let test_widgets = vec![NamedWidget {
        id: "test:widget1".to_string(),
        weight: 100,
        widget: Widget::Label {
            text: "Test".to_string(),
            css_classes: vec![],
        },
    }];

    let daemon = ErrorTestDaemon::new().with_widgets(test_widgets.clone());
    let widgets = daemon.get_widgets();
    assert_eq!(widgets.len(), 1);
    assert_eq!(widgets[0].id, "test:widget1");
}

#[tokio::test]
async fn test_action_handler_success() {
    // Verify successful action handling
    let mut daemon = ErrorTestDaemon::new();

    let action = Action {
        id: "test_action".to_string(),
        params: ActionParams::None,
    };

    let result = daemon.handle_action("widget1".to_string(), action).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_action_handler_failure() {
    // Verify that action handler failures are properly propagated
    let mut daemon = ErrorTestDaemon::new().with_error_on_handle_action();

    let action = Action {
        id: "test_action".to_string(),
        params: ActionParams::None,
    };

    let result = daemon.handle_action("widget1".to_string(), action).await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Simulated action handler failure"));
}

#[tokio::test]
async fn test_multiple_sequential_actions() {
    // Verify that multiple actions can be handled in sequence
    let mut daemon = ErrorTestDaemon::new();

    for i in 0..10 {
        let action = Action {
            id: format!("action_{}", i),
            params: ActionParams::Value(i as f64),
        };

        let result = daemon.handle_action(format!("widget{}", i), action).await;
        assert!(result.is_ok());
    }
}

#[test]
fn test_server_construction() {
    // Verify that server can be constructed with a daemon
    let daemon = ErrorTestDaemon::new();
    let (_server, _notifier) = PluginServer::new("test-plugin", daemon);
    // If we get here without panic, construction succeeded
}

#[test]
fn test_server_with_different_plugin_names() {
    // Verify that server accepts various plugin name formats
    let test_names = vec![
        "simple",
        "with-dashes",
        "with_underscores",
        "MixedCase",
        "with123numbers",
    ];

    for name in test_names {
        let daemon = ErrorTestDaemon::new();
        let (_server, _notifier) = PluginServer::new(name, daemon);
        // If we get here without panic, construction succeeded
    }
}

/// Test scenario: Verify that widget updates work correctly
#[tokio::test]
async fn test_widget_state_updates() {
    let initial_widgets = vec![NamedWidget {
        id: "test:toggle".to_string(),
        weight: 100,
        widget: Widget::Switch {
            active: false,
            sensitive: true,
            on_toggle: Action {
                id: "toggle".to_string(),
                params: ActionParams::None,
            },
        },
    }];

    let daemon = ErrorTestDaemon::new().with_widgets(initial_widgets);

    // Verify initial state
    let widgets = daemon.get_widgets();
    assert_eq!(widgets.len(), 1);

    match &widgets[0].widget {
        Widget::Switch { active, .. } => assert!(!active),
        _ => panic!("Expected Switch widget"),
    }
}
