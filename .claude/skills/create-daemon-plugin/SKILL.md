---
name: create-daemon-plugin
description: Guide for creating a new daemon plugin for Waft. Daemon plugins are standalone binaries implementing the Plugin trait that communicate with the central waft daemon via Unix socket IPC using the entity-based protocol.
---

# Create Daemon Plugin

## When to Use

Use this skill when creating a **new** plugin for Waft. All plugins use the entity-based daemon architecture with `waft-plugin` SDK.

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
waft-plugin = { path = "../../crates/plugin" }
waft-i18n = { path = "../../crates/i18n" }       # If locale support needed

anyhow = "1"
async-trait = "0.1"
log = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }

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

## Step 2: Define Entity Type in Protocol

Add your entity type to `crates/protocol/src/entity/`:

```rust
// crates/protocol/src/entity/your_domain.rs
use serde::{Deserialize, Serialize};

pub const ENTITY_TYPE: &str = "your-entity";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YourEntity {
    pub name: String,
    pub active: bool,
    // Entity data fields -- these are the domain data, NOT widgets
}
```

Register in `crates/protocol/src/entity/mod.rs`:
```rust
pub mod your_domain;
```

## Step 3: Implement Plugin Trait

```rust
use anyhow::{Context, Result};
use waft_plugin::*;

struct YourPlugin {
    // Plugin state -- must be Send + Sync
    state: std::sync::Arc<std::sync::Mutex<YourState>>,
}

#[async_trait::async_trait]
impl Plugin for YourPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[your-plugin] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };

        vec![Entity::new(
            Urn::new("your-plugin", entity::your_domain::ENTITY_TYPE, "default"),
            entity::your_domain::ENTITY_TYPE,
            &entity::your_domain::YourEntity {
                name: state.name.clone(),
                active: state.active,
            },
        )]
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        match action.as_str() {
            "toggle" => {
                let mut state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("[your-plugin] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                state.active = !state.active;
            }
            other => {
                log::warn!("[your-plugin] unknown action: {other}");
            }
        }
        Ok(())
    }

    fn can_stop(&self) -> bool {
        true
    }
}
```

## Step 4: Main Function

There are three levels of manifest support. Use the simplest one that fits:

```rust
fn main() -> Result<()> {
    // Option A: Basic manifest (entity types only)
    if waft_plugin::manifest::handle_provides(&[entity::your_domain::ENTITY_TYPE]) {
        return Ok(());
    }

    // Option B: With display name and description (for `waft plugin ls`)
    if waft_plugin::manifest::handle_provides_full(
        &[entity::your_domain::ENTITY_TYPE],
        Some("Your Plugin"),
        Some("Does useful things"),
    ) {
        return Ok(());
    }

    // Option C: Full descriptions (for `waft plugin describe <name>`)
    // Requires creating the plugin before the runtime to call describe()
    let plugin = YourPlugin::new()?;
    if waft_plugin::manifest::handle_provides_described(
        &[entity::your_domain::ENTITY_TYPE],
        Some("Your Plugin"),
        Some("Does useful things"),
        &plugin,
    ) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting your-plugin...");

    // Build tokio runtime manually so `handle_provides` runs without it
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = YourPlugin::new()?;
        let (runtime, notifier) = PluginRuntime::new("your-plugin", plugin);

        // Optional: spawn background task that calls notifier.notify() on state changes
        tokio::spawn(async move {
            // D-Bus signal monitoring, timer, etc.
            // When state changes: notifier.notify();
        });

        runtime.run().await?;
        Ok(())
    })
}
```

For Option C, implement `describe()` on the Plugin trait:

```rust
fn describe(&self) -> Option<waft_protocol::PluginDescription> {
    Some(waft_protocol::PluginDescription {
        name: "your-plugin".to_string(),
        display_name: "Your Plugin".to_string(),
        description: "Does useful things".to_string(),
        entity_types: vec![
            waft_protocol::description::EntityTypeDescription {
                entity_type: "your-entity".to_string(),
                display_name: "Your Entity".to_string(),
                description: "A domain entity".to_string(),
                properties: vec![/* PropertyDescription */],
                actions: vec![/* ActionDescription */],
            },
        ],
    })
}
```

## Step 5: D-Bus Integration (Optional)

For plugins that monitor D-Bus signals:

```rust
use std::sync::{Arc, Mutex as StdMutex};

struct YourPlugin {
    state: Arc<StdMutex<YourState>>,
}

// Spawn signal monitoring task before starting runtime:
async fn monitor_signals(
    conn: zbus::Connection,
    state: Arc<StdMutex<YourState>>,
    notifier: EntityNotifier,
) {
    let proxy = YourProxy::new(&conn).await.unwrap();
    let mut stream = proxy.receive_property_changed().await.unwrap();
    while let Some(change) = stream.next().await {
        {
            let mut s = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::warn!("[your-plugin] mutex poisoned: {e}");
                    e.into_inner()
                }
            };
            s.update_from(change);
        }
        notifier.notify();
    }
    log::warn!("[your-plugin] signal monitoring exited");
}
```

Key D-Bus patterns:
- **Shared state**: `Arc<StdMutex<T>>` between plugin struct and monitoring tasks
- **Signal monitoring**: `tokio::spawn` + signal stream + `notifier.notify()`
- **zbus v5**: Always use `features = ["tokio"]`, disable default features
- **NO POLLING**: Sleep to next event boundary (D-Bus signals, timer boundaries)

## Step 6: Configuration (Optional)

Load config from `~/.config/waft/config.toml`:

```rust
let config: YourConfig =
    waft_plugin::config::load_plugin_config("your-plugin").unwrap_or_default();
```

Config section format:
```toml
[[plugins]]
id = "your-plugin"
some_setting = "value"
```

## Step 7: Build and Test

```bash
# Build the plugin
cargo build -p waft-plugin-your-plugin

# Run standalone (for development/debugging)
cargo run --bin waft-your-plugin-daemon

# Test provides manifest
./target/debug/waft-your-plugin-daemon provides

# Run full system with all plugins
WAFT_DAEMON_DIR=./target/debug cargo run

# Run tests
cargo test -p waft-plugin-your-plugin
cargo test --workspace
```

The central daemon auto-discovers plugins by running `waft-*-daemon provides` in the daemon directory.

## Critical Rules

1. **Plugin trait requires Send + Sync** -- use `Arc<StdMutex<T>>` for shared mutable state, never `Rc<RefCell<T>>`
2. **Never use `let _ =` on fallible operations** -- always log errors
3. **Recover from mutex poison** -- use `e.into_inner()` instead of `.unwrap()`
4. **Log when background tasks exit** -- wrap `tokio::spawn` with error logging
5. **No polling** -- use D-Bus signals, timer boundaries, or other event-driven patterns
6. **Reap child processes** -- spawn a thread to `wait()` on any child processes

## Step 8: Register in Protocol Registry

Add your entity type to `crates/protocol/src/entity/registry.rs` in the `all_entity_types()` function:

```rust
EntityTypeInfo {
    entity_type: "your-entity",
    domain: "your-domain",
    description: "A domain entity",
    urn_pattern: "{plugin}/your-entity/{id}",
    properties: &[
        PropertyInfo { name: "name", type_description: "string", description: "Entity name", optional: false },
        PropertyInfo { name: "active", type_description: "bool", description: "Whether active", optional: false },
    ],
    actions: &[
        ActionInfo {
            name: "toggle",
            description: "Toggle active state",
            params: &[],
        },
    ],
},
```

This enables `waft protocol` CLI to list your entity type with documentation.

## Checklist

- [ ] Entity type defined in `crates/protocol/src/entity/` with `ENTITY_TYPE` constant
- [ ] Entity module registered in `crates/protocol/src/entity/mod.rs`
- [ ] Entity type added to `crates/protocol/src/entity/registry.rs` in `all_entity_types()`
- [ ] `Cargo.toml` with `[[bin]]` entry and `waft-plugin` dependency
- [ ] Added to workspace `Cargo.toml` members
- [ ] `Plugin` trait implemented (`get_entities`, `handle_action`, `can_stop`)
- [ ] `PluginRuntime::new()` + `runtime.run().await` in `main()`
- [ ] `handle_provides()` check before tokio runtime starts (pick one of three tiers)
- [ ] Background tasks use `notifier.notify()` for state change push
- [ ] Config loading from `~/.config/waft/config.toml` (if configurable)
- [ ] Plugin README.md created with Entity Types, Actions, Configuration sections
- [ ] Serde roundtrip tests for entity type
- [ ] `cargo build --workspace && cargo test --workspace` pass
