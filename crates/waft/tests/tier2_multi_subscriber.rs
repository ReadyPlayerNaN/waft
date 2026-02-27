//! Tier 2 integration tests: multiple subscribers.
//!
//! Tests verify that when multiple apps subscribe to the same entity type,
//! all of them receive EntityUpdated notifications.

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

/// Two apps subscribe to the same entity type; both receive EntityUpdated.
#[tokio::test]
#[serial]
async fn both_subscribers_receive_entity_update() {
    let daemon = TestDaemon::start().await;

    // Two apps subscribe to the same entity type
    let mut app1 = TestApp::connect(&daemon.socket_path).await;
    app1.subscribe("test-entity").await;

    let mut app2 = TestApp::connect(&daemon.socket_path).await;
    app2.subscribe("test-entity").await;

    settle().await;

    // Plugin sends entity
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("test-plugin", "test-entity", "shared-1");
    let data = serde_json::json!({"shared": true});
    plugin
        .send_entity(urn.clone(), "test-entity", data.clone())
        .await;

    // Both apps should receive the update
    let n1 = app1
        .recv_timeout(TIMEOUT)
        .await
        .expect("app1 should receive EntityUpdated");
    let n2 = app2
        .recv_timeout(TIMEOUT)
        .await
        .expect("app2 should receive EntityUpdated");

    for (label, notification) in [("app1", n1), ("app2", n2)] {
        match notification {
            AppNotification::EntityUpdated {
                urn: recv_urn,
                entity_type,
                data: recv_data,
            } => {
                assert_eq!(recv_urn, urn, "{label} URN mismatch");
                assert_eq!(entity_type, "test-entity", "{label} entity_type mismatch");
                assert_eq!(recv_data, data, "{label} data mismatch");
            }
            other => panic!("{label}: expected EntityUpdated, got: {other:?}"),
        }
    }

    daemon.shutdown().await;
}
