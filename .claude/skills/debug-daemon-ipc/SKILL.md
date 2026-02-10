# Debug Daemon IPC

Reference for debugging daemon plugin communication issues between waft-overview and plugin daemons.

## When to Use

Use this skill when:
- A daemon plugin isn't showing widgets in the overview
- Actions (toggles, sliders, buttons) aren't working
- Socket connection errors appear in logs
- Investigating serialization or protocol mismatches

## IPC Architecture

```
waft-overview (GTK4 app)
    |
    +-- DaemonSpawner          spawns 11 daemon binaries at startup
    +-- PluginManager           manages IPC connections to all daemons
    |   +-- client.rs           per-plugin Unix socket client
    |   +-- router.rs           routes actions to correct plugin
    |   +-- discovery.rs        discovers running daemon sockets
    |   +-- registry.rs         tracks plugin widget state
    +-- DaemonWidgetReconciler  converts Widget descriptions to GTK widgets
    |
    v  (Unix socket IPC)
    |
waft-*-daemon (standalone binary)
    +-- PluginServer            listens on Unix socket, handles messages
    +-- PluginDaemon impl       domain logic, returns NamedWidget vec
    +-- WidgetNotifier          signals state changes -> push to clients
```

## Socket Paths

Default: `/run/user/{uid}/waft/plugins/{daemon-name}.sock`

Override for development: `WAFT_PLUGIN_SOCKET_PATH=/tmp/custom.sock`

Daemon directory override: `WAFT_DAEMON_DIR=./target/debug`

## Protocol Format

**Framing**: 4-byte big-endian length prefix + JSON payload

```
[4 bytes: payload length as u32 BE] [N bytes: JSON payload]
```

**Max frame size**: 10 MB (10 * 1024 * 1024 bytes)

## Message Types

### Overview -> Plugin (`OverviewMessage`)

```json
// Request current widgets
"GetWidgets"

// Trigger user action
{"TriggerAction": {"widget_id": "darkman:toggle", "action": {"id": "toggle", "params": "None"}}}
```

### Plugin -> Overview (`PluginMessage`)

```json
// Send widget descriptions (response to GetWidgets, or push after state change)
{"SetWidgets": {"widgets": [
    {
        "id": "darkman:toggle",
        "weight": 190,
        "widget": {
            "FeatureToggle": {
                "title": "Dark Mode",
                "icon": "weather-clear-night-symbolic",
                "details": null,
                "active": true,
                "busy": false,
                "expandable": false,
                "expanded_content": null,
                "on_toggle": {"id": "toggle", "params": "None"}
            }
        }
    }
]}}
```

## Action Parameter Types (`ActionParams`)

```json
"None"                    // No parameters (toggle, button click)
{"Value": 0.75}          // Float value (slider)
{"Text": "some string"}  // Text value
```

## Diagnostic Commands

### Check if daemons are running

```bash
ps aux | grep waft-.*-daemon
```

### Check if sockets exist

```bash
ls -la /run/user/$(id -u)/waft/plugins/
```

### Run a single daemon with debug logging

```bash
RUST_LOG=debug cargo run -p waft-plugin-clock --bin waft-clock-daemon
```

### Run overview with debug logging

```bash
RUST_LOG=debug WAFT_DAEMON_DIR=./target/debug cargo run
```

### Send test message to a daemon socket

```bash
# Python one-liner to send GetWidgets
python3 -c "
import socket, struct, json
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect('/run/user/$(id -u)/waft/plugins/clock-daemon.sock')
msg = json.dumps('GetWidgets').encode()
s.send(struct.pack('>I', len(msg)) + msg)
length = struct.unpack('>I', s.recv(4))[0]
print(json.loads(s.recv(length)))
s.close()
"
```

## Common Issues

### Socket not created

**Symptom**: No `.sock` file in `/run/user/{uid}/waft/plugins/`

**Causes**:
- Daemon binary not found. Check `WAFT_DAEMON_DIR` or that binary is built.
- Parent directory doesn't exist. `PluginServer` creates it, but check permissions.
- Daemon crashed on startup. Run standalone with `RUST_LOG=debug` to see errors.
- Stale socket file. `PluginServer` removes stale sockets, but check for permission issues.

### Daemon crashes on startup

**Symptom**: Socket appears briefly then disappears, or daemon exits immediately.

**Causes**:
- Missing D-Bus service (e.g., darkman not running, UPower not available)
- Config file parse error
- Missing runtime dependency

**Debug**: Run the daemon binary directly with `RUST_LOG=debug`.

### Widgets not appearing

**Symptom**: Daemon is running, socket exists, but no widgets in overview.

**Causes**:
- PluginManager hasn't connected yet (timing). Restart overview.
- `get_widgets()` returns empty vec. Run daemon standalone and check.
- Widget weight puts it off-screen. Check weight values.
- DaemonWidgetReconciler error. Check overview logs for rendering errors.

### Actions not working

**Symptom**: Clicking toggle/button does nothing.

**Causes**:
- Action ID mismatch between `on_toggle`/`on_click` and `handle_action` match arms.
- `handle_action` returns error (logged by PluginServer).
- Widget not pushing updated state after action. Call `notifier.notify()` or return from `handle_action` (server auto-pushes after actions).

### Serialization mismatch

**Symptom**: `JSON error` in daemon or overview logs.

**Causes**:
- `waft-ipc` version mismatch between overview and plugin. Ensure both use workspace dependency.
- Widget enum variant added but not deployed to all components. Rebuild everything with `cargo build --workspace`.

## Testing Module (`waft_plugin_sdk::testing`)

Available test utilities:

```rust
use waft_plugin_sdk::testing::*;

// Pre-built test daemons
let mock = MockPluginDaemon::new("test", widgets);
let test = TestPlugin::new();

// Spawn in background for integration tests
let (handle, socket_path) = spawn_test_plugin("test", daemon).await;
wait_for_socket(&socket_path, Duration::from_secs(1)).await.unwrap();

// Unique paths for parallel tests
let path = unique_test_socket_path("my-test");

// Cleanup
cleanup_test_sockets(&["test1", "test2"]);
```

## Key Source Files

- **PluginServer**: `crates/plugin-sdk/src/server.rs`
- **PluginDaemon trait**: `crates/plugin-sdk/src/daemon.rs`
- **Widget builders**: `crates/plugin-sdk/src/builder.rs`
- **Testing utilities**: `crates/plugin-sdk/src/testing.rs`
- **IPC types**: `crates/ipc/src/lib.rs` (OverviewMessage, PluginMessage, Widget, Action, NamedWidget)
- **DaemonSpawner**: `crates/overview/src/daemon_spawner.rs`
- **PluginManager**: `crates/overview/src/plugin_manager/`
- **Widget reconciler**: `crates/overview/src/daemon_widget_reconciler.rs`
