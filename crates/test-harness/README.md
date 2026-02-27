# Test Harness

Integration test helpers for exercising the waft daemon's entity routing, action forwarding, and subscription management over real Unix sockets.

## Overview

The test harness provides three structs that start a real `WaftDaemon` on a temporary Unix socket and connect lightweight async clients:

- **`TestDaemon`** -- spawns a daemon with no plugins discovered (empty `WAFT_DAEMON_DIR`), manages lifecycle
- **`TestApp`** -- async client that connects as an app, sends `AppMessage`, receives `AppNotification`
- **`TestPlugin`** -- async client that connects as a plugin, sends `PluginMessage`, receives `PluginCommand`

No GTK, glib, or D-Bus dependencies. The harness uses raw length-prefixed JSON framing over Unix sockets, matching the daemon's transport protocol.

## Usage

Add as a dev-dependency in the crate you want to test:

```toml
[dev-dependencies]
waft-test-harness = { path = "../../crates/test-harness" }
serial_test = "3"
```

All tests that start a `TestDaemon` must be annotated with `#[serial]` (from `serial_test`) because the daemon sets the `WAFT_DAEMON_DIR` environment variable.

## Example

```rust
use std::time::Duration;
use serial_test::serial;
use waft_protocol::urn::Urn;
use waft_protocol::AppNotification;
use waft_test_harness::{TestDaemon, TestApp, TestPlugin};

#[tokio::test]
#[serial]
async fn plugin_entity_routed_to_app() {
    // Start daemon on a temp socket (no plugins discovered)
    let daemon = TestDaemon::start().await;

    // Connect a plugin and send an entity update
    let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
    let urn = Urn::new("my-plugin", "my-entity", "item-1");
    plugin.send_entity(urn.clone(), "my-entity", serde_json::json!({"value": 42})).await;

    // Small yield to let the daemon process the message
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect an app and subscribe to the entity type
    let mut app = TestApp::connect(&daemon.socket_path).await;
    app.subscribe("my-entity").await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send another update -- app should receive it
    plugin.send_entity(urn.clone(), "my-entity", serde_json::json!({"value": 100})).await;

    let notification = app.recv_timeout(Duration::from_secs(2)).await
        .expect("app should receive EntityUpdated");

    match notification {
        AppNotification::EntityUpdated { urn: recv_urn, data, .. } => {
            assert_eq!(recv_urn, urn);
            assert_eq!(data["value"], 100);
        }
        other => panic!("expected EntityUpdated, got: {other:?}"),
    }

    daemon.shutdown().await;
}
```

## API

### TestDaemon

```rust
// Start a daemon on a temporary Unix socket
let daemon = TestDaemon::start().await;
// Access the socket path for client connections
let path: &Path = &daemon.socket_path;
// Shut down when done
daemon.shutdown().await;
```

### TestApp

```rust
let mut app = TestApp::connect(&daemon.socket_path).await;
app.subscribe("entity-type").await;          // Subscribe to an entity type
app.send(&AppMessage::Status { .. }).await;  // Send any AppMessage
let msg = app.recv_timeout(Duration::from_secs(2)).await;  // None on timeout
```

### TestPlugin

```rust
let mut plugin = TestPlugin::connect(&daemon.socket_path).await;
plugin.send_entity(urn, "entity-type", json_data).await;       // EntityUpdated
plugin.send_entity_removed(urn, "entity-type").await;           // EntityRemoved
plugin.send(&PluginMessage::ActionSuccess { .. }).await;        // Any PluginMessage
let cmd = plugin.recv_timeout(Duration::from_secs(2)).await;    // None on timeout
```

## Test Organization

Integration tests live in `crates/waft/tests/` organized in two tiers:

- **Tier 1** (`tier1_entity_routing.rs`): Subscribe, EntityUpdated relay, entity cache, EntityRemoved
- **Tier 2** (`tier2_action_routing.rs`, `tier2_multi_subscriber.rs`): TriggerAction forwarding, ActionSuccess/Error relay, multiple app subscribers

Run with: `cargo test -p waft -- --test-threads=1`
