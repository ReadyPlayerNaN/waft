//! Tier 3 integration tests: protocol hardening features.

use std::time::Duration;

use serial_test::serial;
use waft_protocol::{
    AppMessage, AppNotification, CAP_DERIVED_ENTITY_TYPE, CAP_HANDSHAKE, CAP_STATUS_COMPLETE,
    HandshakeMessage, Hello, PeerRole, PluginMessage, PROTOCOL_VERSION,
};
use waft_protocol::urn::Urn;
use waft_test_harness::{TestApp, TestDaemon, TestPlugin};

const TIMEOUT: Duration = Duration::from_secs(4);

async fn settle() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
#[serial]
async fn app_handshake_negotiates_current_protocol_version() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    let response = app.handshake("test-app").await;

    match response {
        HandshakeMessage::HelloAck(ack) => {
            assert_eq!(ack.negotiated_version, PROTOCOL_VERSION);
            assert!(ack.capabilities.contains(&CAP_HANDSHAKE.to_string()));
            assert!(ack.capabilities.contains(&CAP_DERIVED_ENTITY_TYPE.to_string()));
        }
        other => panic!("expected HelloAck, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn older_peer_with_subset_capabilities_is_accepted() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.send_hello(Hello {
        role: PeerRole::App,
        implementation: "legacy-test-app".to_string(),
        plugin_name: None,
        min_version: 1,
        max_version: 1,
        capabilities: vec![CAP_STATUS_COMPLETE.to_string()],
    })
    .await;

    let response = app.recv_handshake().await;
    match response {
        HandshakeMessage::HelloAck(ack) => {
            assert_eq!(ack.negotiated_version, 1);
            assert_eq!(ack.capabilities, vec![CAP_STATUS_COMPLETE.to_string()]);
        }
        other => panic!("expected HelloAck, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn unsupported_handshake_version_is_rejected() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.send_hello(Hello {
        role: PeerRole::App,
        implementation: "future-app".to_string(),
        plugin_name: None,
        min_version: PROTOCOL_VERSION + 100,
        max_version: PROTOCOL_VERSION + 100,
        capabilities: vec![CAP_HANDSHAKE.to_string()],
    })
    .await;

    let response = app.recv_handshake().await;
    match response {
        HandshakeMessage::HelloError(err) => {
            assert_eq!(err.error.code, "handshake.incompatible-version");
            assert_eq!(err.error.scope, waft_protocol::ProtocolErrorScope::Handshake);
        }
        other => panic!("expected HelloError, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn derived_entity_type_routing_works_without_explicit_entity_type() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    let response = app.handshake("app-with-derived-entity-type").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));
    app.subscribe("test-entity").await;

    settle().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let response = plugin.handshake("test-plugin", "test-plugin-impl").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));

    let urn = Urn::new("test-plugin", "test-entity", "item-1");
    plugin
        .send(&PluginMessage::EntityUpdated {
            urn: urn.clone(),
            entity_type: None,
            data: serde_json::json!({"value": 42}),
        })
        .await;

    let notification = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive EntityUpdated");

    match notification {
        AppNotification::EntityUpdated {
            urn: recv_urn,
            entity_type,
            data,
        } => {
            assert_eq!(recv_urn, urn);
            assert!(entity_type.is_none(), "handshaked peer should receive derived entity type");
            assert_eq!(data, serde_json::json!({"value": 42}));
        }
        other => panic!("expected EntityUpdated, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn daemon_generated_action_error_includes_structured_details() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    let response = app.handshake("action-error-app").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));

    let action_id = uuid::Uuid::new_v4();
    app.send(&AppMessage::TriggerAction {
        urn: Urn::new("missing-plugin", "missing-entity", "item"),
        action: "do-thing".to_string(),
        action_id,
        params: serde_json::Value::Null,
        timeout_ms: None,
    })
    .await;

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
            assert!(error.contains("no plugin found"));
            let details = error_details.expect("structured error details should be present");
            assert_eq!(details.code, "entity.not-found");
        }
        other => panic!("expected ActionError, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn unknown_handshake_capabilities_are_dropped_from_ack() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.send_hello(Hello {
        role: PeerRole::App,
        implementation: "cap-probe-app".to_string(),
        plugin_name: None,
        min_version: PROTOCOL_VERSION,
        max_version: PROTOCOL_VERSION,
        capabilities: vec![
            CAP_HANDSHAKE.to_string(),
            "unknown-capability".to_string(),
            CAP_STATUS_COMPLETE.to_string(),
        ],
    })
    .await;

    match app.recv_handshake().await {
        HandshakeMessage::HelloAck(ack) => {
            assert_eq!(
                ack.capabilities,
                vec![CAP_HANDSHAKE.to_string(), CAP_STATUS_COMPLETE.to_string()]
            );
        }
        other => panic!("expected HelloAck, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn plugin_handshake_without_plugin_name_is_rejected() {
    let daemon = TestDaemon::start().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    plugin
        .send_hello(Hello {
            role: PeerRole::Plugin,
            implementation: "broken-plugin".to_string(),
            plugin_name: None,
            min_version: PROTOCOL_VERSION,
            max_version: PROTOCOL_VERSION,
            capabilities: vec![CAP_HANDSHAKE.to_string()],
        })
        .await;

    match plugin.recv_handshake().await {
        HandshakeMessage::HelloError(err) => {
            assert_eq!(err.error.code, "protocol.validation");
            assert!(err.error.message.contains("plugin_name"));
        }
        other => panic!("expected HelloError, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn legacy_app_receives_explicit_entity_type_from_negotiated_plugin() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;
    settle().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let response = plugin.handshake("test-plugin", "test-plugin-impl").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));

    let urn = Urn::new("test-plugin", "test-entity", "item-legacy");
    plugin
        .send(&PluginMessage::EntityUpdated {
            urn: urn.clone(),
            entity_type: None,
            data: serde_json::json!({"value": 7}),
        })
        .await;

    match app
        .recv_timeout(TIMEOUT)
        .await
        .expect("legacy app should receive EntityUpdated")
    {
        AppNotification::EntityUpdated { entity_type, .. } => {
            assert_eq!(entity_type.as_deref(), Some("test-entity"));
        }
        other => panic!("expected EntityUpdated, got: {other:?}"),
    }

    plugin
        .send(&PluginMessage::EntityRemoved {
            urn: urn.clone(),
            entity_type: None,
        })
        .await;

    match app
        .recv_timeout(TIMEOUT)
        .await
        .expect("legacy app should receive EntityRemoved")
    {
        AppNotification::EntityRemoved { entity_type, .. } => {
            assert_eq!(entity_type.as_deref(), Some("test-entity"));
        }
        other => panic!("expected EntityRemoved, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn negotiated_app_receives_stale_without_explicit_entity_type() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    let response = app.handshake("stale-app").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));
    app.subscribe("test-entity").await;
    settle().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let response = plugin.handshake("test-plugin", "test-plugin-impl").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));
    let urn = Urn::new("test-plugin", "test-entity", "item-stale");
    plugin
        .send(&PluginMessage::EntityUpdated {
            urn: urn.clone(),
            entity_type: None,
            data: serde_json::json!({"value": 1}),
        })
        .await;

    let _ = app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive initial update");

    drop(plugin);

    match app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive EntityStale")
    {
        AppNotification::EntityStale {
            urn: recv_urn,
            entity_type,
        } => {
            assert_eq!(recv_urn, urn);
            assert!(entity_type.is_none());
        }
        other => panic!("expected EntityStale, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn negotiated_app_receives_outdated_without_explicit_entity_type() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    let response = app.handshake("outdated-app").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));
    app.subscribe("test-entity").await;
    settle().await;

    for i in 0..5 {
        let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
        let response = plugin.handshake("flaky-plugin", "flaky-plugin-impl").await;
        assert!(matches!(response, HandshakeMessage::HelloAck(_)));
        let urn = Urn::new("flaky-plugin", "test-entity", "item-outdated");
        plugin
            .send(&PluginMessage::EntityUpdated {
                urn: urn.clone(),
                entity_type: None,
                data: serde_json::json!({"crash": i}),
            })
            .await;

        let _ = app
            .recv_timeout(TIMEOUT)
            .await
            .expect("app should receive update before crash");

        drop(plugin);

        let notification = app
            .recv_timeout(TIMEOUT)
            .await
            .expect("app should receive stale/outdated after crash");

        match (i, notification) {
            (0..=3, AppNotification::EntityStale { entity_type, .. }) => {
                assert!(entity_type.is_none());
            }
            (4, AppNotification::EntityOutdated { entity_type, .. }) => {
                assert!(entity_type.is_none());
            }
            (_, other) => panic!("unexpected crash notification at iteration {i}: {other:?}"),
        }
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn action_timeout_returns_structured_timeout_error() {
    let daemon = TestDaemon::start().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("timeout-plugin", "test-entity", "item-timeout");
    plugin
        .send_entity(urn.clone(), "test-entity", serde_json::json!({"ready": true}))
        .await;
    settle().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    let response = app.handshake("timeout-app").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));
    app.subscribe("test-entity").await;
    settle().await;

    let action_id = uuid::Uuid::new_v4();
    app.send(&AppMessage::TriggerAction {
        urn: urn.clone(),
        action: "slow-action".to_string(),
        action_id,
        params: serde_json::Value::Null,
        timeout_ms: Some(1),
    })
    .await;

    let _ = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("plugin should receive TriggerAction");

    match app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive ActionError")
    {
        AppNotification::ActionError {
            action_id: recv_id,
            error,
            error_details,
        } => {
            assert_eq!(recv_id, action_id);
            assert_eq!(error, "action timed out");
            let details = error_details.expect("expected timeout details");
            assert_eq!(details.code, "action.timeout");
            assert!(details.retryable);
        }
        other => panic!("expected ActionError, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn plugin_disconnect_during_inflight_action_returns_structured_error() {
    let daemon = TestDaemon::start().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("disconnect-plugin", "test-entity", "item-disconnect");
    plugin
        .send_entity(urn.clone(), "test-entity", serde_json::json!({"ready": true}))
        .await;
    settle().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    let response = app.handshake("disconnect-app").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));

    let action_id = uuid::Uuid::new_v4();
    app.send(&AppMessage::TriggerAction {
        urn: urn.clone(),
        action: "disconnect-me".to_string(),
        action_id,
        params: serde_json::Value::Null,
        timeout_ms: Some(5_000),
    })
    .await;

    let _ = plugin
        .recv_timeout(TIMEOUT)
        .await
        .expect("plugin should receive TriggerAction");
    drop(plugin);

    match app
        .recv_timeout(TIMEOUT)
        .await
        .expect("app should receive ActionError")
    {
        AppNotification::ActionError {
            action_id: recv_id,
            error,
            error_details,
        } => {
            assert_eq!(recv_id, action_id);
            assert_eq!(error, "plugin disconnected");
            let details = error_details.expect("expected structured error details");
            assert_eq!(details.code, "action.execution");
        }
        other => panic!("expected ActionError, got: {other:?}"),
    }

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn negotiated_plugin_entity_type_mismatch_is_rejected() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;
    settle().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let response = plugin.handshake("test-plugin", "test-plugin-impl").await;
    assert!(matches!(response, HandshakeMessage::HelloAck(_)));
    plugin
        .send(&PluginMessage::EntityUpdated {
            urn: Urn::new("test-plugin", "test-entity", "item-mismatch"),
            entity_type: Some("wrong-entity".to_string()),
            data: serde_json::json!({"value": 42}),
        })
        .await;

    assert!(app.recv_timeout(Duration::from_millis(200)).await.is_none());

    daemon.shutdown().await;
}

#[tokio::test]
#[serial]
async fn legacy_plugin_entity_type_mismatch_is_dropped() {
    let daemon = TestDaemon::start().await;

    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("test-entity").await;
    settle().await;

    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    plugin
        .send(&PluginMessage::EntityUpdated {
            urn: Urn::new("test-plugin", "test-entity", "item-legacy-mismatch"),
            entity_type: Some("wrong-entity".to_string()),
            data: serde_json::json!({"value": 42}),
        })
        .await;

    assert!(app.recv_timeout(Duration::from_millis(200)).await.is_none());

    daemon.shutdown().await;
}
