//! Tier 1 integration tests: entity routing through the daemon.
//!
//! Tests verify that EntityUpdated/EntityRemoved messages from plugins
//! are correctly routed to subscribed apps via the daemon.

use std::time::Duration;

use serial_test::serial;
use waft_protocol::urn::Urn;
use waft_protocol::AppNotification;
use waft_test_harness::{TestApp, TestDaemon, TestPlugin};

const TIMEOUT: Duration = Duration::from_secs(2);

/// A small yield to let the daemon process messages between steps.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// Plugin sends EntityUpdated, subscribed app receives it.
#[tokio::test]
#[serial]
async fn plugin_entity_routed_to_subscriber() {
    let daemon = TestDaemon::start().await;

    // Connect plugin and send entity (identifies connection as plugin)
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "item-1");
    let data = serde_json::json!({"value": 42});
    plugin
        .send_entity(urn.clone(), "test-entity", data.clone())
        .await;

    settle().await;

    // Connect app and subscribe
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;

    settle().await;

    // Plugin sends another entity update
    let data2 = serde_json::json!({"value": 100});
    plugin
        .send_entity(urn.clone(), "test-entity", data2.clone())
        .await;

    // App should receive the update
    let notification = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive EntityUpdated");

    match notification {
        AppNotification::EntityUpdated {
            urn: recv_urn,
            entity_type,
            data: recv_data,
        } => {
            assert_eq!(recv_urn, urn);
            assert_eq!(entity_type, "test-entity");
            assert_eq!(recv_data, data2);
        }
        other => panic!("expected EntityUpdated, got: {other:?}"),
    }

    daemon.shutdown().await;
}

/// App subscribes first, then plugin connects and sends entity.
#[tokio::test]
#[serial]
async fn app_subscribes_before_plugin_connects() {
    let daemon = TestDaemon::start().await;

    // App subscribes before any plugin exists
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;

    settle().await;

    // Plugin connects and sends entity
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "item-1");
    let data = serde_json::json!({"name": "hello"});
    plugin
        .send_entity(urn.clone(), "test-entity", data.clone())
        .await;

    // App should receive the update
    let notification = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive EntityUpdated");

    match notification {
        AppNotification::EntityUpdated {
            urn: recv_urn,
            entity_type,
            data: recv_data,
        } => {
            assert_eq!(recv_urn, urn);
            assert_eq!(entity_type, "test-entity");
            assert_eq!(recv_data, data);
        }
        other => panic!("expected EntityUpdated, got: {other:?}"),
    }

    daemon.shutdown().await;
}

/// Entity is cached by daemon; new app receives cached entity via Status request.
#[tokio::test]
#[serial]
async fn cached_entity_returned_on_status() {
    let daemon = TestDaemon::start().await;

    // Plugin sends entity
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "cached-1");
    let data = serde_json::json!({"cached": true});
    plugin
        .send_entity(urn.clone(), "test-entity", data.clone())
        .await;

    settle().await;

    // New app connects and requests status (not subscribe)
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.send(&waft_protocol::AppMessage::Status {
        entity_type: "test-entity".to_string(),
    })
    .await;

    // App should receive the cached entity
    let notification = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive cached entity via Status");

    match notification {
        AppNotification::EntityUpdated {
            urn: recv_urn,
            entity_type,
            data: recv_data,
        } => {
            assert_eq!(recv_urn, urn);
            assert_eq!(entity_type, "test-entity");
            assert_eq!(recv_data, data);
        }
        other => panic!("expected EntityUpdated from cache, got: {other:?}"),
    }

    daemon.shutdown().await;
}

/// Plugin sends EntityRemoved, subscribed app receives it.
#[tokio::test]
#[serial]
async fn entity_removed_forwarded_to_subscriber() {
    let daemon = TestDaemon::start().await;

    // Connect app and subscribe
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;

    settle().await;

    // Plugin sends entity first (identifies as plugin)
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "item-to-remove");
    plugin
        .send_entity(urn.clone(), "test-entity", serde_json::json!({"temp": true}))
        .await;

    // App receives the EntityUpdated
    let _ = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive initial EntityUpdated");

    // Plugin sends EntityRemoved
    plugin
        .send_entity_removed(urn.clone(), "test-entity")
        .await;

    // App should receive the EntityRemoved
    let notification = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive EntityRemoved");

    match notification {
        AppNotification::EntityRemoved {
            urn: recv_urn,
            entity_type,
        } => {
            assert_eq!(recv_urn, urn);
            assert_eq!(entity_type, "test-entity");
        }
        other => panic!("expected EntityRemoved, got: {other:?}"),
    }

    daemon.shutdown().await;
}
