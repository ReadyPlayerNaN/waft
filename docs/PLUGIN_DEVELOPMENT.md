# Plugin Development Guide

This guide explains how to create dynamic plugins for Waft (`.so` files loaded at runtime).

## Quick Start

1. Create plugin directory:
   ```bash
   mkdir -p plugins/my-plugin/src
   ```

2. Create `plugins/my-plugin/Cargo.toml`:
   ```toml
   [package]
   name = "waft-plugin-my-plugin"
   version = "0.1.0"
   edition = "2024"

   [lib]
   crate-type = ["cdylib"]

   [dependencies]
   waft-plugin-api = { path = "../../crates/waft-plugin-api" }
   waft-core = { path = "../../crates/waft-core" }
   gtk = { version = "0.10", package = "gtk4", features = ["v4_6", "unsafe-assume-initialized"] }
   glib = "0.21"
   async-trait = "0.1"
   anyhow = "1"
   tokio = { version = "1", features = ["sync", "rt"] }
   ```

3. Create `plugins/my-plugin/src/lib.rs` (see template below)

4. Build and test:
   ```bash
   cargo build -p waft-plugin-my-plugin
   WAFT_PLUGIN_DIR=./target/debug cargo run -p waft-overview
   ```

## Plugin Template

```rust
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;

use waft_core::menu_state::MenuStore;
use waft_plugin_api::{
    OverviewPlugin, PluginId, PluginResources, Slot, Widget, WidgetRegistrar,
};

// Export plugin metadata
waft_plugin_api::export_plugin_metadata!("plugin::my-plugin", "MyPlugin", "0.1.0");
waft_plugin_api::export_overview_plugin!(MyPlugin::new());

pub struct MyPlugin {
    dbus: Option<Arc<waft_core::dbus::DbusHandle>>,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for MyPlugin {
    fn default() -> Self {
        Self {
            dbus: None,
            tokio_handle: None,
        }
    }
}

impl MyPlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for MyPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::my-plugin")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        // Get D-Bus connection from host (NEVER create your own!)
        self.dbus = resources.session_dbus.clone();

        // Save tokio handle for spawning async tasks
        self.tokio_handle = resources.tokio_handle.clone();

        // Get initial state from D-Bus if needed
        // ...

        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        // Create GTK widgets here
        let button = gtk::Button::builder()
            .label("My Plugin")
            .build();

        // Register the widget
        registrar.register_widget(Rc::new(Widget {
            id: "my-plugin:button".to_string(),
            slot: Slot::Info,
            weight: 100,
            el: button.upcast::<gtk::Widget>(),
        }));

        Ok(())
    }
}
```

## D-Bus Integration

If your plugin needs to monitor D-Bus signals:

```rust
async fn create_elements(
    &mut self,
    _app: &gtk::Application,
    _menu_store: Rc<MenuStore>,
    registrar: Rc<dyn WidgetRegistrar>,
) -> Result<()> {
    // ... widget creation ...

    // Set up D-Bus monitoring
    if let (Some(dbus), Some(handle)) = (&self.dbus, &self.tokio_handle) {
        let callback = |value: Option<String>| {
            println!("Received D-Bus signal: {:?}", value);
        };

        // IMPORTANT: Use *_with_handle() methods
        dbus.listen_for_values_with_handle(
            "org.example.Service",
            "SignalName",
            callback,
            Some(handle),  // Pass the tokio handle
        )
        .await?;
    }

    Ok(())
}
```

## Critical Requirements

### 1. GTK Initialization Feature
**MUST** add `unsafe-assume-initialized` to gtk4:
```toml
gtk = { version = "0.10", package = "gtk4", features = ["v4_6", "unsafe-assume-initialized"] }
```

Without this, you'll get: `GTK has not been initialized` panic.

### 2. Use Provided Resources
**NEVER** create `DbusHandle` in your plugin:
```rust
// ❌ WRONG - will cause "no reactor running" panic
let dbus = Arc::new(DbusHandle::connect().await?);

// ✅ CORRECT - use provided connection
self.dbus = resources.session_dbus.clone();
```

### 3. Use Tokio Handle for D-Bus
**NEVER** call `tokio::spawn()` directly:
```rust
// ❌ WRONG - no reactor running
tokio::spawn(async { ... });

// ✅ CORRECT - use provided handle
if let Some(handle) = &self.tokio_handle {
    handle.spawn(async { ... });
}
```

For D-Bus signal monitoring, use `*_with_handle()` methods:
- `listen_for_values_with_handle(interface, member, callback, Some(handle))`
- `listen_signals_with_handle(match_rule, Some(handle))`

### 4. No GTK in init()
**NEVER** create GTK widgets in `init()`:
```rust
async fn init(&mut self, resources: &PluginResources) -> Result<()> {
    // ❌ WRONG - GTK not initialized yet
    let button = gtk::Button::new();

    // ✅ CORRECT - pure Rust state only
    self.dbus = resources.session_dbus.clone();
    Ok(())
}

async fn create_elements(...) -> Result<()> {
    // ✅ CORRECT - create widgets here
    let button = gtk::Button::new();
    Ok(())
}
```

## Plugin Configuration

Add to `~/.config/waft/config.toml`:
```toml
[[plugins]]
id = "plugin::my-plugin"

# Plugin-specific settings (optional)
[plugins.settings]
some_option = "value"
```

Access in your plugin:
```rust
fn configure(&mut self, settings: &toml::Table) -> Result<()> {
    if let Some(value) = settings.get("some_option") {
        // Parse settings
    }
    Ok(())
}
```

## Common Errors

| Error | Cause | Solution |
|-------|-------|----------|
| "GTK has not been initialized" | Missing `unsafe-assume-initialized` | Add feature to gtk4 in Cargo.toml |
| "no reactor running" | Creating DbusHandle in plugin | Use `resources.session_dbus` |
| "no reactor running" | Calling `tokio::spawn()` | Use `handle.spawn()` with provided handle |
| Widget creation crash | Creating widgets in `init()` | Move to `create_elements()` |

## Build and Test

```bash
# Build your plugin
cargo build -p waft-plugin-my-plugin

# Check .so was created
ls -la target/debug/libwaft_plugin_my_plugin.so

# Verify exports
nm -D target/debug/libwaft_plugin_my_plugin.so | grep waft

# Run with your plugin
WAFT_PLUGIN_DIR=./target/debug cargo run -p waft-overview

# Run with debug logs
WAFT_PLUGIN_DIR=./target/debug RUST_LOG=debug cargo run -p waft-overview
```

## Examples

See existing dynamic plugins for reference:
- `plugins/clock/` - Simple info widget with no D-Bus
- `plugins/darkman/` - D-Bus integration with signal monitoring

## Resources

- Main documentation: [AGENTS.md](../AGENTS.md)
- Plugin API: [crates/waft-plugin-api/src/overview.rs](../crates/waft-plugin-api/src/overview.rs)
- D-Bus handle: [crates/waft-core/src/dbus.rs](../crates/waft-core/src/dbus.rs)
