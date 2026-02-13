# Socket Reconnection

When the waft daemon crashes while waft-overview is running, the overview disables all interactive widgets and reconnects automatically.

## Behavior

- All buttons/sliders/toggles become insensitive (grayed out) on disconnect
- Stale data stays visible — components are not cleared
- Reconnection attempts every 1 second with D-Bus activation
- On reconnect: full re-handshake (subscribe all 20 entity types + request_status)
- UI re-enables immediately on successful reconnect
- Startup no longer blocks on daemon — UI creates immediately, enables when daemon is reachable

## Architecture

A persistent `flume` channel carries `OverviewEvent` from a tokio connection-management task to the glib main thread.

```
OverviewEvent = Notification(AppNotification) | Connected | Disconnected
```

### daemon_connection_task

Long-running tokio task (`crates/overview/src/waft_client.rs`) that manages the connection lifecycle:

1. Request D-Bus activation to start the daemon
2. Attempt `WaftClient::connect()` (single attempt)
3. On success: store client in `Arc<Mutex<Option<WaftClient>>>`, subscribe all entity types, send `Connected`, forward notifications
4. On disconnect: clear client handle, send `Disconnected`
5. Sleep 1 second, loop to step 1

The task exits when the flume receiver is dropped (overview closed).

### App integration

In `app.rs`, a glib future consumes the persistent channel:

- `Notification(n)` — forwarded to `EntityStore`
- `Connected` — `clip.set_sensitive(true)`
- `Disconnected` — `clip.set_sensitive(false)`

GTK `set_sensitive(false)` on the `clip` frame (root content container) propagates to all children, disabling every interactive widget.

### Write path during disconnect

The `entity_action_callback` locks `Arc<Mutex<Option<WaftClient>>>` on each action. During disconnect, the handle is `None` and actions are logged and dropped. On reconnect, the handle is set to `Some(new_client)`.

## Files

- `crates/overview/src/waft_client.rs` — `OverviewEvent`, `ENTITY_TYPES`, `daemon_connection_task`
- `crates/overview/src/ui/main_window.rs` — `pub clip: gtk::Frame` field
- `crates/overview/src/app.rs` — persistent channel + event loop

## Smoke test

1. Start daemon: `WAFT_DAEMON_DIR=./target/debug cargo run`
2. Start overview: `WAFT_DAEMON_DIR=./target/debug cargo run -p waft-overview`
3. Show overlay, verify interactive widgets
4. Kill daemon: `pkill waft`
5. Verify widgets gray out, logs show `[app] daemon disconnected, disabling UI`
6. Restart daemon
7. Verify widgets re-enable within ~1 second, logs show `[app] daemon connected, enabling UI`
