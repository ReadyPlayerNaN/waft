# Entity-Based Architecture

This document describes the entity-based architecture used by Waft, sourced from the current implementation code.

## Overview

Waft uses a central daemon (`waft`) that routes typed domain entities between plugin processes and consumer applications via Unix sockets. Plugins produce entities; apps subscribe to entity types and render UI independently.

```
Plugin (daemon)  <-->  waft (central daemon)  <-->  waft-overview (GTK app)
```

All communication uses length-prefixed JSON over Unix sockets at `$XDG_RUNTIME_DIR/waft/daemon.sock`.

---

## 1. Plugin Trait

Plugins implement the `Plugin` trait from `crates/plugin/src/plugin.rs`. The trait requires `Send + Sync` because the runtime may call `get_entities()` from a different context than `handle_action()`.

```rust
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    /// Return all current entities.
    /// Called on connect and whenever the EntityNotifier fires.
    /// The runtime diffs against previous state and sends
    /// EntityUpdated/EntityRemoved messages to the daemon.
    fn get_entities(&self) -> Vec<Entity>;

    /// Handle an action triggered by an app via the daemon.
    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value>;

    /// Whether the plugin can stop gracefully (default: true).
    fn can_stop(&self) -> bool { true }
}
```

### Entity struct

Each entity carries a URN, entity type string, and JSON data payload (`crates/plugin/src/plugin.rs`):

```rust
pub struct Entity {
    pub urn: Urn,
    pub entity_type: String,
    pub data: serde_json::Value,
}

impl Entity {
    pub fn new<T: Serialize>(urn: Urn, entity_type: &str, data: &T) -> Self;
}
```

### Send+Sync requirement

Use `Arc<StdMutex<State>>` for shared mutable state between the plugin struct and background monitoring tasks. The runtime wraps the plugin in `Arc<P>` internally (`crates/plugin/src/runtime.rs:37`).

---

## 2. Daemon Routing Architecture

The central daemon (`crates/waft/src/daemon.rs`) accepts Unix socket connections, identifies each connection as either a plugin or an app based on the first message received, and routes messages between them.

### Connection identification

When a new connection sends its first message (`daemon.rs:216-254`):

- If the message parses as `PluginMessage::EntityUpdated` or `PluginMessage::EntityRemoved`, the connection is identified as a **plugin**. The plugin name is extracted from the URN's first segment.
- If the message parses as `AppMessage`, the connection is identified as an **app**.
- Other first messages are rejected.

### Message flow

**Plugin to Apps** (entity updates):
1. Plugin sends `PluginMessage::EntityUpdated { urn, entity_type, data }` to daemon
2. Daemon caches entity data in `entity_cache` (keyed by URN string)
3. Daemon converts to `AppNotification::EntityUpdated` and forwards to all apps subscribed to that `entity_type`

**Apps to Plugins** (actions):
1. App sends `AppMessage::TriggerAction { urn, action, action_id, params, timeout_ms }` to daemon
2. Daemon looks up plugin connection via `PluginRegistry::connection_for_urn()` (matches on `urn.plugin()`)
3. Daemon tracks the action in `ActionTracker` with the given timeout (default 5 seconds)
4. Daemon forwards as `PluginCommand::TriggerAction` to the plugin
5. Plugin responds with `PluginMessage::ActionSuccess` or `PluginMessage::ActionError`
6. Daemon resolves the action and forwards the result back to the requesting app

### Entity cache

The daemon caches the latest `EntityUpdated` data per URN (`daemon.rs:269-271`). When an app sends `AppMessage::Status { entity_type }`, the daemon replies immediately with all cached entities of that type without querying the plugin (`daemon.rs:426-446`). This enables fast reconnection: a newly connected app gets current state from the cache.

### Registries

- **PluginRegistry** (`crates/waft/src/registry.rs`): Maps plugin name to connection UUID. Looks up connections by URN via `connection_for_urn()` (extracts plugin name from the URN's first segment).
- **AppRegistry** (`crates/waft/src/registry.rs`): Maps entity type to set of subscribed app connection UUIDs. Supports `subscribe()`, `unsubscribe()`, `subscribers()`, and `has_subscribers()`.

### Event loop

The daemon runs a single-threaded `tokio::select!` loop (`daemon.rs:103-162`) with three arms:

1. **Accept new connections** from the Unix listener
2. **Process events** from connection reader tasks (messages or disconnections)
3. **Handle timeouts** for pending actions and CanStop retries

The timeout arm uses **sleep-to-deadline** (no polling): `next_wakeup()` computes the earliest of any pending action timeout or CanStop retry instant.

---

## 3. Protocol Messages

Defined in `crates/protocol/src/message.rs`. Four message enums define the protocol:

### AppMessage (App -> Daemon)

| Variant | Fields | Purpose |
|---------|--------|---------|
| `Subscribe` | `entity_type: String` | Subscribe to entity type updates |
| `Unsubscribe` | `entity_type: String` | Unsubscribe from entity type |
| `Status` | `entity_type: String` | Request cached entities of a type |
| `TriggerAction` | `urn, action, action_id, params, timeout_ms` | Trigger action on entity |

### PluginMessage (Plugin -> Daemon)

| Variant | Fields | Purpose |
|---------|--------|---------|
| `EntityUpdated` | `urn, entity_type, data` | Entity created or updated |
| `EntityRemoved` | `urn, entity_type` | Entity removed |
| `ActionSuccess` | `action_id` | Action completed successfully |
| `ActionError` | `action_id, error` | Action failed |
| `StopResponse` | `can_stop: bool` | Response to CanStop command |

### AppNotification (Daemon -> App)

| Variant | Fields | Purpose |
|---------|--------|---------|
| `EntityUpdated` | `urn, entity_type, data` | Forwarded entity update |
| `EntityRemoved` | `urn, entity_type` | Forwarded entity removal |
| `ActionSuccess` | `action_id` | Action completed |
| `ActionError` | `action_id, error` | Action failed or timed out |
| `EntityStale` | `urn, entity_type` | Plugin crashed, will restart |
| `EntityOutdated` | `urn, entity_type` | Plugin circuit-broken, no restart |

### PluginCommand (Daemon -> Plugin)

| Variant | Fields | Purpose |
|---------|--------|---------|
| `CanStop` | (none) | Ask plugin if it can stop gracefully |
| `TriggerAction` | `urn, action, action_id, params` | Forward action from app |

---

## 4. URN Format

Defined in `crates/protocol/src/urn.rs`. URNs uniquely identify entities within the protocol.

### Format

```
{plugin}/{entity-type}/{id}[/{entity-type}/{id}]*
```

- First segment: plugin name
- Remaining segments: entity-type/id pairs (must come in complete pairs)
- Total segment count must be odd: 1 (plugin) + 2N (entity-type/id pairs)

### Construction

```rust
// Simple entity
let urn = Urn::new("clock", "clock", "default");
// => "clock/clock/default"

// Nested entity (parent-child)
let parent = Urn::new("bluez", "bluetooth-adapter", "hci0");
let child = parent.child("bluetooth-device", "AA:BB:CC:DD:EE:FF");
// => "bluez/bluetooth-adapter/hci0/bluetooth-device/AA:BB:CC:DD:EE:FF"
```

### Accessors

| Method | Returns | Example (nested URN above) |
|--------|---------|---------------------------|
| `plugin()` | First segment | `"bluez"` |
| `root_entity_type()` | Second segment (subscription target) | `"bluetooth-adapter"` |
| `entity_type()` | Last entity-type segment | `"bluetooth-device"` |
| `id()` | Last segment | `"AA:BB:CC:DD:EE:FF"` |

### Parsing

`Urn::parse()` validates the string:
- Not empty (`UrnError::Empty`)
- At least 3 segments (`UrnError::TooFewSegments`)
- No empty segments (`UrnError::EmptySegment`)
- Complete entity-type/id pairs (`UrnError::IncompleteSegment`)

### Real-world examples

| Plugin | URN | Pattern |
|--------|-----|---------|
| clock | `clock/clock/default` | Simple, single entity |
| battery | `battery/battery/BAT0` | Simple, hardware-based ID |
| audio | `audio/audio-device/speakers` | Simple, device name ID |
| darkman | `darkman/dark-mode/default` | Simple, singleton |
| bluez | `bluez/bluetooth-adapter/hci0` | Simple, adapter |
| bluez | `bluez/bluetooth-adapter/hci0/bluetooth-device/AA:BB:CC` | Nested, device under adapter |
| networkmanager | `networkmanager/network-adapter/wlan0` | Simple, interface name ID |

---

## 5. Transport

Defined in `crates/protocol/src/transport.rs` (sync) and `crates/plugin/src/transport.rs` (async/tokio). Both use the same wire format.

### Frame format

```
[4 bytes: u32 length (big-endian)][N bytes: JSON payload]
```

- Length prefix: 4-byte big-endian unsigned integer
- Payload: JSON-serialized message
- Maximum frame size: 10 MB (`MAX_FRAME_SIZE = 10 * 1024 * 1024`)

### Implementations

- **Sync** (`waft_protocol::transport`): `write_framed<W: Write, T: Serialize>()` and `read_framed<R: Read, T: Deserialize>()` for standard `std::io` traits.
- **Async** (`waft_plugin::transport`): `write_framed<W: AsyncWriteExt + Unpin>()` and `read_framed<R: AsyncReadExt + Unpin>()` for tokio. Returns `Ok(None)` on clean disconnect (EOF).
- **Daemon** (`crates/waft/src/connection.rs`): Inline async implementation using `tokio::io::AsyncReadExt`/`AsyncWriteExt` directly, with per-connection write queues via `mpsc::channel`.

---

## 6. Entity Types (Domain-Organized)

Entity types are defined in `crates/protocol/src/entity/` and organized by **domain**, not by plugin. This is an explicit design rule:

> Entity modules must **never** reference a specific plugin implementation. Names like `darkman`, `sunsetr`, `caffeine` are plugin identifiers -- they describe *who provides* the data, not *what the data is*.

### Domain modules

| Module | Entity Types | Constants |
|--------|-------------|-----------|
| `audio` | `AudioDevice` | `ENTITY_TYPE = "audio-device"` |
| `bluetooth` | `BluetoothAdapter`, `BluetoothDevice` | `BluetoothAdapter::ENTITY_TYPE`, `BluetoothDevice::ENTITY_TYPE` |
| `calendar` | `CalendarEvent` | `ENTITY_TYPE = "calendar-event"` |
| `clock` | `Clock` | `ENTITY_TYPE = "clock"` |
| `display` | `DarkMode`, `Display`, `NightLight` | `DARK_MODE_ENTITY_TYPE`, `DISPLAY_ENTITY_TYPE`, `NIGHT_LIGHT_ENTITY_TYPE` |
| `keyboard` | `KeyboardLayout` | `ENTITY_TYPE = "keyboard-layout"` |
| `network` | `NetworkAdapter`, `WifiNetwork`, `EthernetConnection`, `Vpn`, `TetheringConnection` | Multiple constants |
| `notification` | `Notification`, `Dnd` | `NOTIFICATION_ENTITY_TYPE`, `DND_ENTITY_TYPE` |
| `power` | `Battery` | `ENTITY_TYPE = "battery"` |
| `session` | `SleepInhibitor`, `Session` | `SLEEP_INHIBITOR_ENTITY_TYPE`, `SESSION_ENTITY_TYPE` |
| `storage` | `BackupMethod` | `BACKUP_METHOD_ENTITY_TYPE` |
| `weather` | `Weather` | `ENTITY_TYPE = "weather"` |

---

## 7. PluginRuntime and EntityNotifier

### PluginRuntime (`crates/plugin/src/runtime.rs`)

Manages the plugin's connection to the daemon and runs the event loop.

```rust
let (runtime, notifier) = PluginRuntime::new("clock", plugin);
runtime.run().await?;
```

**Internals:**
- Connects to daemon socket at `$XDG_RUNTIME_DIR/waft/daemon.sock` (overridable via `WAFT_DAEMON_SOCKET`)
- Splits the stream into read/write halves
- Spawns a background write task with `mpsc::channel(64)` buffer
- Maintains a `HashMap<String, serde_json::Value>` of previously-sent entity data for diffing
- On notifier change or after action handling, calls `send_all_entities()` which diffs against previous state

**Entity diffing** (`runtime.rs:173-231`):
- Compares current `get_entities()` output against the previous snapshot
- Only sends `EntityUpdated` for new or changed entities
- Sends `EntityRemoved` for entities that existed previously but are no longer returned

**Event loop** (`runtime.rs:81-125`):
- `tokio::select!` on notifier changes and incoming daemon commands
- `PluginCommand::TriggerAction`: Spawns a new tokio task to handle the action concurrently, then re-sends all entities
- `PluginCommand::CanStop`: Calls `plugin.can_stop()` synchronously and sends `StopResponse`

### EntityNotifier (`crates/plugin/src/notifier.rs`)

A `Clone`-able handle that plugins use to signal state changes. Uses a `tokio::sync::watch` channel with a monotonically incrementing counter.

```rust
pub struct EntityNotifier {
    tx: watch::Sender<u64>,
}

impl EntityNotifier {
    pub fn notify(&self) {
        // Increments counter, waking the runtime's select loop
    }
}
```

Plugins typically clone the notifier into background tasks (D-Bus signal monitors, timers, etc.) and call `notifier.notify()` when state changes.

---

## 8. On-Demand Plugin Spawning

### Plugin discovery (`crates/waft/src/plugin_discovery.rs`)

At daemon startup, `PluginDiscoveryCache::build()` discovers all available plugins:

1. Scans for `waft-*-daemon` binaries in the daemon directory (detected via `WAFT_DAEMON_DIR` env, `./target/debug`, `./target/release`, or `/usr/bin`)
2. Runs each binary with `provides` argument in parallel threads (500ms timeout per binary)
3. Parses the JSON manifest output: `{ "entity_types": ["clock", ...] }`
4. Builds a `HashMap<String, (String, PathBuf)>` mapping entity types to (plugin name, binary path)

### Plugin manifest (`crates/plugin/src/manifest.rs`)

Each plugin handles the `provides` CLI argument in `main()` before starting the tokio runtime:

```rust
fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides(&["clock"]) {
        return Ok(());
    }
    // ... start tokio runtime
}
```

This prints a JSON manifest and exits immediately, without connecting to the daemon.

### On-demand spawning (`crates/waft/src/plugin_spawner.rs`)

When an app subscribes to an entity type (`AppMessage::Subscribe`):

1. Daemon calls `plugin_spawner.ensure_plugin_for_entity_type(entity_type)` (`daemon.rs:408-409`)
2. If the entity type has been attempted before, it's a no-op (`spawn_attempted` set)
3. If the plugin is already spawned, marks the entity type as attempted
4. Otherwise, spawns the binary with `std::process::Command`, inheriting stdout/stderr
5. Spawns a dedicated reaper thread per plugin to call `child.wait()` and prevent zombie processes

### Respawning after disconnect

When a plugin disconnects, `mark_disconnected()` clears the spawn tracking for that plugin's entity types, allowing `ensure_plugin_for_entity_type()` to respawn it on the next subscribe or restart attempt.

---

## 9. Crash Recovery

### CrashTracker (`crates/waft/src/crash_tracker.rs`)

Tracks crash timestamps per plugin using a sliding window.

**Constants:**
- `MAX_CRASHES = 5` -- maximum crashes before circuit breaker trips
- `CRASH_WINDOW = 60 seconds` -- sliding window for counting crashes

**Outcomes:**
- `CrashOutcome::Restart` -- plugin should be restarted (fewer than 5 crashes in 60s)
- `CrashOutcome::CircuitBroken` -- too many crashes, do not restart

### Crash handling flow (`daemon.rs:646-702`)

When a plugin disconnects unexpectedly (not via graceful CanStop):

1. Daemon records crash via `crash_tracker.record_crash(name)`
2. Removes all cached entities for that plugin
3. Notifies subscribed apps:
   - `CrashOutcome::Restart` -> sends `AppNotification::EntityStale` per entity
   - `CrashOutcome::CircuitBroken` -> sends `AppNotification::EntityOutdated` per entity
4. For `Restart`: re-spawns the plugin if it still has subscribers
5. For `CircuitBroken`: logs and does nothing further

### App-side handling (`crates/overview/src/entity_store.rs:68-75`)

The `EntityStore` treats both `EntityStale` and `EntityOutdated` as entity removals, removing the entity from the cache and notifying subscribers. This causes the UI to hide stale data.

---

## 10. Graceful Shutdown (CanStop)

When an app unsubscribes from an entity type or disconnects:

1. Daemon checks if any plugin now has zero subscribers across all its entity types (`daemon.rs:568-601`)
2. If zero subscribers, sends `PluginCommand::CanStop` to the plugin
3. Plugin responds with `PluginMessage::StopResponse { can_stop: true/false }`
4. If `can_stop: true`: Daemon marks the plugin for graceful stop and disconnects it (no crash recovery)
5. If `can_stop: false`: Daemon schedules a retry in 30 seconds (`daemon.rs:371-380`)

Retries are handled by `handle_can_stop_retries()` in the event loop's timeout arm. Before retrying, the daemon re-checks whether the plugin has gained new subscribers.

---

## 11. EntityStore (App-Side Subscription System)

Defined in `crates/overview/src/entity_store.rs`. Lives on the GTK main thread (uses `RefCell`, not `RwLock`).

### Structure

```rust
pub struct EntityStore {
    cache: RefCell<HashMap<String, CachedEntity>>,       // URN string -> entity
    subscribers: RefCell<HashMap<String, Vec<Rc<dyn Fn()>>>>,  // entity_type -> callbacks
}
```

### Subscription

Components subscribe to entity types and receive callbacks when entities of that type change:

```rust
store.subscribe_type("clock", move || {
    // Re-read current entities from the store
    let clocks: Vec<(Urn, Clock)> = store.get_entities_typed("clock");
    // Update UI...
});
```

### Change deduplication

`handle_entity_updated()` compares incoming data against the cached value. If identical, the update is skipped and subscribers are not notified (`entity_store.rs:160-167`).

### Query methods

- `get_entities_typed<T>(entity_type)` -- returns `Vec<(Urn, T)>` for all entities of a type (skips deserialization failures)
- `get_entity_typed<T>(urn)` -- returns `Option<T>` for a specific entity by URN
- `get_entities_raw(entity_type)` -- returns `Vec<(Urn, Value)>` without deserialization

---

## 12. WaftClient (Connection Management)

Defined in `crates/overview/src/waft_client.rs`. Manages the overview app's connection to the daemon.

### Write path

Uses a dedicated OS thread with `std::sync::mpsc` channel so that GTK main thread sends wake immediately via OS condvar, bypassing the tokio scheduler entirely (`waft_client.rs:160-177`).

### Read path

A tokio task reads `AppNotification` messages from the socket and forwards them via a `flume::unbounded()` channel into glib context (`waft_client.rs:180-201`).

### Connection lifecycle (`daemon_connection_task`)

The long-running task (`waft_client.rs:268-359`):

1. Requests D-Bus activation for `org.waft.Daemon` to auto-start the daemon
2. Attempts connection via `WaftClient::connect()`
3. On success: subscribes to all entity types, requests cached status, stores client handle
4. Forwards notifications until the daemon disconnects
5. On disconnect: clears client handle, sends `OverviewEvent::Disconnected`, retries after 1 second

### Entity type list

The overview subscribes to all known entity types defined in `ENTITY_TYPES` constant (`waft_client.rs:39-61`), currently 21 entity types across all domains.

---

## 13. Plugin SDK Utilities

### D-Bus Signal Monitoring (`crates/plugin/src/dbus_monitor.rs`)

Helper for monitoring D-Bus signals in plugins:

```rust
monitor_signal(conn, config, state, notifier, |msg, state| {
    // Process signal, mutate state
    Ok(true)  // true = notify (trigger entity re-send)
}).await?;
```

Also provides `monitor_signal_async()` for handlers that need async operations (does not hold the mutex lock during the handler).

### Plugin Configuration (`crates/plugin/src/config.rs`)

Loads plugin-specific config from `~/.config/waft/config.toml`:

```rust
let config: MyConfig = waft_plugin::config::load_plugin_config("clock")?;
```

Returns `T::default()` if the config file doesn't exist or has no matching plugin entry.

---

## 14. Example: Complete Plugin Implementation

The clock plugin (`plugins/clock/bin/waft-clock-daemon.rs`) demonstrates the full pattern:

```rust
use waft_plugin::*;

struct ClockPlugin { /* state */ }

#[async_trait::async_trait]
impl Plugin for ClockPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let clock = entity::clock::Clock {
            time: "14:30".to_string(),
            date: "Thursday, 12 Feb 2026".to_string(),
        };
        vec![Entity::new(
            Urn::new("clock", entity::clock::ENTITY_TYPE, "default"),
            entity::clock::ENTITY_TYPE,
            &clock,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if action == "click" { /* handle click */ }
        Ok(serde_json::Value::Null)
    }
}

fn main() -> Result<()> {
    // 1. Handle manifest discovery
    if waft_plugin::manifest::handle_provides(&[entity::clock::ENTITY_TYPE]) {
        return Ok(());
    }

    // 2. Initialize logging
    waft_plugin::init_plugin_logger("info");

    // 3. Create tokio runtime
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let plugin = ClockPlugin::new()?;
        let (runtime, notifier) = PluginRuntime::new("clock", plugin);

        // 4. Spawn background task to trigger updates
        tokio::spawn(async move {
            loop {
                let secs_to_next_minute = 60 - chrono::Local::now().second() as u64;
                tokio::time::sleep(Duration::from_secs(secs_to_next_minute)).await;
                notifier.notify();
            }
        });

        // 5. Run the plugin runtime (blocks until daemon disconnects)
        runtime.run().await
    })
}
```

### Key patterns in this example:

1. **`handle_provides` before tokio**: Fast manifest response without starting the runtime
2. **`PluginRuntime::new` returns `(runtime, notifier)`**: Runtime consumes the plugin, notifier is cloned into background tasks
3. **Background task calls `notifier.notify()`**: Triggers the runtime to re-read `get_entities()` and diff against previous state
4. **Sleep-to-deadline**: Timer sleeps until the next minute boundary, not on a fixed interval
5. **`runtime.run().await`**: Blocks until the daemon disconnects or the notifier is dropped

---

## 15. D-Bus Integration

### Daemon registration (`crates/waft/src/main.rs`)

The daemon registers as `org.waft.Daemon` on the session D-Bus with `DoNotQueue` flag -- fails immediately if another instance is running.

### App-side activation (`crates/overview/src/waft_client.rs:375-389`)

The overview app requests D-Bus activation via `StartServiceByName("org.waft.Daemon", 0)` on its first connection attempt. The D-Bus broker looks up the corresponding `.service` file and spawns the daemon binary automatically.

---

## 16. Action Tracking

The daemon tracks in-flight actions via `ActionTracker` (`crates/waft/src/action_tracker.rs`):

- `track(action_id, app_conn_id, plugin_conn_id, timeout_ms)`: Starts tracking with a deadline (default 5 seconds)
- `resolve(action_id)`: Completes a pending action, returning metadata
- `drain_timed_out()`: Returns all actions past their deadline
- `drain_for_connection(conn_id)`: Returns all actions for a disconnected connection
- `next_deadline()`: Earliest pending deadline for sleep-to-deadline scheduling

Timed-out actions result in `AppNotification::ActionError { error: "action timed out" }` sent to the requesting app. Orphaned actions (plugin disconnected) result in `AppNotification::ActionError { error: "plugin disconnected" }`.
