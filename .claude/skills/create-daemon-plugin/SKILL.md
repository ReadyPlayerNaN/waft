# Create Daemon Plugin

Guide for creating a new daemon plugin for Waft. Daemon plugins are standalone binaries that communicate with waft-overview via Unix socket IPC.

## When to Use

Use this skill when creating a **new** plugin. All new plugins must use the daemon architecture. The legacy cdylib (.so) architecture is deprecated.

## Plugin Structure

```
plugins/your-plugin/
    Cargo.toml
    bin/
        waft-your-plugin-daemon.rs    # Daemon binary entry point
    src/
        lib.rs                         # Optional: shared library code (for tests)
```

## Step 1: Cargo.toml

```toml
[package]
name = "waft-plugin-your-plugin"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "waft-your-plugin-daemon"
path = "bin/waft-your-plugin-daemon.rs"

[dependencies]
waft-plugin-sdk = { path = "../../crates/plugin-sdk" }
waft-ipc = { path = "../../crates/ipc" }
waft-i18n = { path = "../../crates/i18n" }       # If locale support needed

anyhow = "1"
async-trait = "0.1"
env_logger = "0.11"
log = "0.4"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
toml = "0.8"

# Add if D-Bus integration is needed:
# zbus = { version = "5", default-features = false, features = ["tokio"] }
# futures-util = "0.3"
```

Add the plugin to the workspace root `Cargo.toml`:

```toml
members = [
    # ... existing members ...
    "plugins/your-plugin",
]
```

## Step 2: Implement PluginDaemon Trait

The daemon binary implements `PluginDaemon` from `waft-plugin-sdk`:

```rust
use waft_plugin_sdk::*;

#[async_trait::async_trait]
pub trait PluginDaemon: Send + Sync {
    /// Return current widget descriptions (called on every GetWidgets request)
    fn get_widgets(&self) -> Vec<NamedWidget>;

    /// Handle user interaction from the overview UI
    async fn handle_action(
        &mut self,
        widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
```

## Step 3: Canonical Example (Clock Daemon Pattern)

Minimal daemon with config and timer-based updates:

```rust
use anyhow::Result;
use serde::Deserialize;
use waft_plugin_sdk::*;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct MyConfig {
    // Plugin-specific config fields
}

struct MyDaemon {
    config: MyConfig,
}

impl MyDaemon {
    fn new() -> Result<Self> {
        let config = Self::load_config().unwrap_or_default();
        Ok(Self { config })
    }

    fn load_config() -> Result<MyConfig> {
        let config_path = dirs::config_dir()
            .context("No config directory")?
            .join("waft/config.toml");

        if !config_path.exists() {
            return Ok(MyConfig::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let root: toml::Table = toml::from_str(&content)?;

        if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
            for plugin in plugins {
                if let Some(table) = plugin.as_table() {
                    if let Some(id) = table.get("id").and_then(|v| v.as_str()) {
                        if id == "waft::your-plugin-daemon" || id == "your-plugin-daemon" {
                            return toml::Value::Table(table.clone())
                                .try_into()
                                .context("Failed to parse config");
                        }
                    }
                }
            }
        }

        Ok(MyConfig::default())
    }
}

#[async_trait::async_trait]
impl PluginDaemon for MyDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![NamedWidget {
            id: "my-plugin:toggle".to_string(),
            weight: 100,
            widget: FeatureToggleBuilder::new("My Feature")
                .icon("emblem-system-symbolic")
                .active(false)
                .on_toggle("toggle")
                .build(),
        }]
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.id.as_str() {
            "toggle" => {
                // Handle toggle
            }
            _ => {}
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting my-plugin daemon...");

    let daemon = MyDaemon::new()?;
    let (server, notifier) = PluginServer::new("your-plugin-daemon", daemon);

    // Optional: spawn background task that calls notifier.notify() on state changes

    server.run().await?;
    Ok(())
}
```

## Step 4: Widget Builders

The SDK provides builders for all widget types:

```rust
// Feature toggle (most common)
FeatureToggleBuilder::new("Title")
    .icon("icon-name-symbolic")
    .details("Status text")
    .active(true)
    .busy(false)
    .expandable(true)
    .expanded_content(menu_widget)
    .on_toggle("action_id")
    .build()

// Slider (volume, brightness)
SliderBuilder::new(0.75)              // value 0.0..1.0
    .icon("audio-volume-high-symbolic")
    .muted(false)
    .expandable(true)
    .expanded_content(device_menu)
    .on_value_change("set_volume")
    .on_icon_click("toggle_mute")
    .build()

// Menu row (list items, settings)
MenuRowBuilder::new("Label")
    .icon("icon-symbolic")
    .sublabel("Description")
    .trailing(SwitchBuilder::new().active(true).build())
    .sensitive(true)
    .on_click("action_id")
    .build()

// Container (layout)
ContainerBuilder::new(Orientation::Vertical)
    .spacing(4)
    .css_class("menu-section")
    .child(row1)
    .child(row2)
    .build()

// Button, Label, InfoCard, Switch also available
ButtonBuilder::new().label("Power Off").icon("system-shutdown-symbolic").on_click("shutdown").build()
LabelBuilder::new("Text").css_class("dim-label").build()
InfoCardBuilder::new("Title").icon("icon").description("Details").build()
SwitchBuilder::new().active(true).on_toggle("toggle").build()
```

## Step 5: D-Bus Integration (Optional)

For plugins that monitor D-Bus signals, follow the darkman daemon pattern:

```rust
use std::sync::{Arc, Mutex as StdMutex};
use zbus::Connection;
use futures_util::StreamExt;

struct MyDaemon {
    state: Arc<StdMutex<MyState>>,
    conn: Connection,
}

impl MyDaemon {
    async fn new() -> Result<Self> {
        let conn = Connection::session().await?;   // or Connection::system()
        let initial_state = get_state_from_dbus(&conn).await?;
        Ok(Self {
            state: Arc::new(StdMutex::new(initial_state)),
            conn,
        })
    }
}

// Spawn signal monitoring task before starting server:
async fn monitor_signals(conn: Connection, state: Arc<StdMutex<MyState>>, notifier: WidgetNotifier) -> Result<()> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.example.Service")?
        .path("/org/example/path")?
        .interface("org.example.Interface")?
        .member("SignalName")?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn).await?;
    dbus_proxy.add_match_rule(rule).await?;

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        // Update shared state
        *state.lock().unwrap() = new_state;
        notifier.notify();  // Push updated widgets to overview
    }
    Ok(())
}

// In main():
let shared_state = daemon.shared_state();
let monitor_conn = daemon.conn.clone();
let (server, notifier) = PluginServer::new("my-daemon", daemon);

tokio::spawn(async move {
    if let Err(e) = monitor_signals(monitor_conn, shared_state, notifier).await {
        log::error!("Signal monitoring failed: {}", e);
    }
});

server.run().await?;
```

Key D-Bus patterns:
- **Shared state**: `Arc<StdMutex<T>>` between daemon struct and monitoring tasks
- **Signal monitoring**: `tokio::spawn` + `zbus::MessageStream` + `notifier.notify()`
- **zbus v5**: Always use `features = ["tokio"]`, disable default features
- **NO POLLING**: Sleep to next event boundary (D-Bus signals, timer boundaries)

## Step 6: Register with DaemonSpawner

Add your daemon binary name to `crates/overview/src/daemon_spawner.rs`:

```rust
pub fn spawn_all_daemons(&mut self) {
    let daemon_names = vec![
        // ... existing daemons ...
        "waft-your-plugin-daemon",
    ];
    // ...
}
```

## Step 7: Build and Test

```bash
# Build the daemon
cargo build -p waft-plugin-your-plugin

# Run standalone (for development)
cargo run -p waft-plugin-your-plugin --bin waft-your-plugin-daemon

# Run full system with all daemons
WAFT_DAEMON_DIR=./target/debug cargo run

# Run tests
cargo test -p waft-plugin-your-plugin

# Verify socket is created
ls /run/user/$(id -u)/waft/plugins/your-plugin-daemon.sock
```

## Testing with SDK Helpers

The `waft_plugin_sdk::testing` module provides test utilities:

- `MockPluginDaemon` - configurable mock for testing
- `TestPlugin` - minimal toggle plugin
- `spawn_test_plugin(name, daemon)` - spawn daemon in background tokio task
- `wait_for_socket(path, timeout)` - wait for socket file to appear
- `unique_test_socket_path(prefix)` - generate unique socket path for parallel tests
- `cleanup_test_sockets(names)` - clean up test socket files

```rust
use waft_plugin_sdk::testing::*;

#[tokio::test]
async fn test_my_plugin() {
    let daemon = TestPlugin::new();
    let (handle, socket_path) = spawn_test_plugin("test", daemon).await;
    wait_for_socket(&socket_path, Duration::from_secs(1)).await.unwrap();
    // ... test via IPC ...
    handle.abort();
}
```

## IPC Protocol Reference

- **Transport**: Unix socket at `/run/user/{uid}/waft/plugins/{name}.sock`
- **Framing**: 4-byte big-endian length prefix + JSON payload
- **Overview -> Plugin**: `OverviewMessage::GetWidgets`, `OverviewMessage::TriggerAction { widget_id, action }`
- **Plugin -> Overview**: `PluginMessage::SetWidgets { widgets }`
- **Push updates**: Call `notifier.notify()` to push `SetWidgets` to all connected clients

## Checklist

- [ ] `Cargo.toml` with `[[bin]]` entry and `waft-plugin-sdk` dependency
- [ ] Added to workspace `Cargo.toml` members
- [ ] `PluginDaemon` trait implemented (`get_widgets`, `handle_action`)
- [ ] `PluginServer::new()` + `server.run().await` in `main()`
- [ ] Background tasks use `notifier.notify()` for state change push
- [ ] Daemon name added to `DaemonSpawner::spawn_all_daemons()`
- [ ] Config loading from `~/.config/waft/config.toml` (if configurable)
- [ ] Tests written
- [ ] `cargo build --workspace && cargo test --workspace` pass
