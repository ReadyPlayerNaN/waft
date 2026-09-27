//! Tier 2 integration tests: action routing through the daemon.
//!
//! Tests verify that TriggerAction from apps is routed to plugins,
//! and ActionSuccess/ActionError from plugins is routed back to apps.

use std::time::Duration;

use serial_test::serial;
use uuid::Uuid;
use waft_protocol::urn::Urn;
use waft_protocol::{AppMessage, AppNotification, PluginCommand, PluginMessage};
use waft_test_harness::{TestApp, TestDaemon, TestPlugin};

const TIMEOUT: Duration = Duration::from_secs(2);

/// A small yield to let the daemon process messages between steps.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// App sends TriggerAction, daemon routes it to the plugin as PluginCommand::TriggerAction.
#[tokio::test]
#[serial]
async fn trigger_action_routed_to_plugin() {
    let daemon = TestDaemon::start().await;

    // Plugin connects and sends entity to register
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "item-1");
    plugin
        .send_entity(
            urn.clone(),
            "test-entity",
            serde_json::json!({"ready": true}),
        )
        .await;

    settle().await;

    // App connects, subscribes, and triggers an action
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;

    settle().await;

    let action_id = Uuid::new_v4();
    let params = serde_json::json!({"level": 75});
    app.send(&AppMessage::TriggerAction {
        urn: urn.clone(),
        action: "set-level".to_string(),
        action_id,
        params: params.clone(),
        timeout_ms: Some(5000),
    })
    .await;

    // Plugin should receive the TriggerAction command
    let cmd = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("plugin should receive TriggerAction");

    match cmd {
        PluginCommand::TriggerAction {
            urn: recv_urn,
            action,
            action_id: recv_action_id,
            params: recv_params,
        } => {
            assert_eq!(recv_urn, urn);
            assert_eq!(action, "set-level");
            assert_eq!(recv_action_id, action_id);
            assert_eq!(recv_params, params);
        }
        other => panic!("expected TriggerAction, got: {other:?}"),
    }

    daemon.shutdown().await;
}

/// Plugin sends ActionSuccess, daemon routes it back to the app.
#[tokio::test]
#[serial]
async fn action_success_routed_to_app() {
    let daemon = TestDaemon::start().await;

    // Plugin registers
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "item-1");
    plugin
        .send_entity(
            urn.clone(),
            "test-entity",
            serde_json::json!({"ready": true}),
        )
        .await;

    settle().await;

    // App subscribes and triggers action
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;

    settle().await;

    let action_id = Uuid::new_v4();
    app.send(&AppMessage::TriggerAction {
        urn: urn.clone(),
        action: "toggle".to_string(),
        action_id,
        params: serde_json::Value::Null,
        timeout_ms: Some(5000),
    })
    .await;

    // Plugin receives TriggerAction
    let _ = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("plugin should receive TriggerAction");

    // Plugin responds with ActionSuccess
    plugin
        .send(&PluginMessage::ActionSuccess {
            action_id,
            data: None,
        })
        .await;

    // App should receive ActionSuccess
    let notification = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive ActionSuccess");

    match notification {
        AppNotification::ActionSuccess {
            action_id: recv_id, ..
        } => {
            assert_eq!(recv_id, action_id);
        }
        other => panic!("expected ActionSuccess, got: {other:?}"),
    }

    daemon.shutdown().await;
}

/// Plugin sends ActionError, daemon routes it back to the app.
#[tokio::test]
#[serial]
async fn action_error_routed_to_app() {
    let daemon = TestDaemon::start().await;

    // Plugin registers
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "item-1");
    plugin
        .send_entity(
            urn.clone(),
            "test-entity",
            serde_json::json!({"ready": true}),
        )
        .await;

    settle().await;

    // App subscribes and triggers action
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;

    settle().await;

    let action_id = Uuid::new_v4();
    app.send(&AppMessage::TriggerAction {
        urn: urn.clone(),
        action: "fail-action".to_string(),
        action_id,
        params: serde_json::Value::Null,
        timeout_ms: Some(5000),
    })
    .await;

    // Plugin receives TriggerAction
    let _ = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("plugin should receive TriggerAction");

    // Plugin responds with ActionError
    plugin
        .send(&PluginMessage::ActionError {
            action_id,
            error: "device not found".to_string(),
            error_details: Some(waft_protocol::ProtocolError::not_found("device not found")),
        })
        .await;

    // App should receive ActionError
    let notification = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive ActionError");

    match notification {
        AppNotification::ActionError {
            action_id: recv_id,
            error,
            error_details,
        } => {
            assert_eq!(recv_id, action_id);
            assert_eq!(error, "device not found");
            assert_eq!(
                error_details
                    .expect("expected structured error details")
                    .code,
                "entity.not-found"
            );
        }
        other => panic!("expected ActionError, got: {other:?}"),
    }

    daemon.shutdown().await;
}

/// Repeated network actions are rejected before they fan out to the plugin.
#[tokio::test]
#[serial]
async fn repeated_connect_actions_are_not_fanned_out() {
    let daemon = TestDaemon::start().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "vpn-1");
    plugin
        .send_entity(
            urn.clone(),
            "test-entity",
            serde_json::json!({"state": "disconnected"}),
        )
        .await;
    settle().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;
    settle().await;

    for _ in 0..3 {
        app.send(&AppMessage::TriggerAction {
            urn: urn.clone(),
            action: "connect".to_string(),
            action_id: Uuid::new_v4(),
            params: serde_json::Value::Null,
            timeout_ms: Some(5000),
        })
        .await;
    }

    let first = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("first connect action should reach the plugin");
    assert!(matches!(first, PluginCommand::TriggerAction { .. }));

    assert!(
        plugin
            .recv_timeout(Duration::from_millis(100))
            .await
            .is_none(),
        "rapid duplicate connect actions must not reach the plugin"
    );

    daemon.shutdown().await;
}

/// A daemon timeout must actively cancel the plugin operation.
#[tokio::test]
#[serial]
async fn timed_out_action_sends_cancellation_to_plugin() {
    let daemon = TestDaemon::start().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let handshake = plugin.handshake("test-plugin", "test-plugin-impl").await;
    assert!(matches!(
        handshake,
        waft_protocol::HandshakeMessage::HelloAck(_)
    ));
    let urn = Urn::new("test-plugin", "test-entity", "item-timeout");
    plugin
        .send_entity(
            urn.clone(),
            "test-entity",
            serde_json::json!({"ready": true}),
        )
        .await;
    settle().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;
    settle().await;

    let action_id = Uuid::new_v4();
    app.send(&AppMessage::TriggerAction {
        urn,
        action: "connect".to_string(),
        action_id,
        params: serde_json::Value::Null,
        timeout_ms: Some(50),
    })
    .await;

    let command = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("plugin should receive the timed action");
    assert!(matches!(command, PluginCommand::TriggerAction { .. }));

    let timeout_error = app
        .recv_timeout(Duration::from_secs(1))
        .await
        .expect("app should receive timeout error");
    assert!(matches!(
        timeout_error,
        AppNotification::ActionError { action_id: id, .. } if id == action_id
    ));

    let cancel = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("plugin should receive cancellation");
    assert_eq!(cancel, PluginCommand::CancelAction { action_id });

    daemon.shutdown().await;
}
