# Plugin Daemon Migration Guide

This guide walks you through converting a traditional Waft plugin (.so shared library) into a standalone daemon process that communicates with the overview via IPC.

## Table of Contents

1. [Overview](#overview)
2. [Why Migrate?](#why-migrate)
3. [Prerequisites](#prerequisites)
4. [Step-by-Step Conversion](#step-by-step-conversion)
5. [Code Examples](#code-examples)
6. [Common Pitfalls](#common-pitfalls)
7. [Testing Your Daemon](#testing-your-daemon)
8. [Systemd Integration](#systemd-integration)
9. [Migration Checklist](#migration-checklist)

## Overview

**Traditional Plugin Architecture:**
- Compiled as cdylib (.so)
- Loaded directly into overview process
- Uses GTK4 widgets directly
- Shares memory space with host

**Daemon Plugin Architecture:**
- Standalone executable binary
- Runs as separate process
- Communicates via Unix socket IPC
- Uses serializable widget protocol

## Why Migrate?

### Benefits

1. **Process Isolation**: Plugin crashes don't crash the overview
2. **Independent Lifecycle**: Plugins can restart without restarting overview
3. **Simpler Threading**: No cdylib tokio TLS isolation issues
4. **Better Debugging**: Can attach debuggers to individual plugins
5. **Resource Management**: OS manages plugin memory/CPU independently
6. **Development Speed**: Faster iteration (no need to restart overview)

### Trade-offs

- Slightly higher memory overhead (separate process)
- IPC communication overhead (negligible for most plugins)
- Widget protocol must be serializable (no direct GTK access)

## Prerequisites

Before starting, ensure you have:

1. **Dependencies** in `Cargo.toml`:
   ```toml
   [dependencies]
   waft-plugin-sdk = { path = "../../crates/plugin-sdk" }
   waft-ipc = { path = "../../crates/ipc" }
   tokio = { version = "1", features = ["full"] }
   async-trait = "0.1"
   anyhow = "1.0"
   env_logger = "0.11"
   ```

2. **Daemon Binary Configuration**:
   ```toml
   [[bin]]
   name = "waft-<plugin>-daemon"
   path = "bin/waft-<plugin>-daemon.rs"
   ```

3. **Existing Plugin Code** to reference

## Step-by-Step Conversion

### Step 1: Create the Daemon Binary

Create a new file at `plugins/<plugin>/bin/waft-<plugin>-daemon.rs`:

```rust
//! <Plugin> daemon - <brief description>

use anyhow::Result;
use waft_plugin_sdk::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    log::info!("Starting <plugin> daemon...");

    // Create daemon
    let daemon = <Plugin>Daemon::new()?;

    // Create and run server
    let server = PluginServer::new("<plugin>-daemon", daemon);
    server.run().await?;

    Ok(())
}
```

### Step 2: Define Your Daemon State

Convert your plugin state into a daemon struct:

```rust
/// <Plugin> daemon state
struct <Plugin>Daemon {
    config: <Plugin>Config,
    // ... other state
}

impl <Plugin>Daemon {
    fn new() -> Result<Self> {
        let config = Self::load_config().unwrap_or_default();
        Ok(Self { config })
    }

    fn load_config() -> Result<<Plugin>Config> {
        // Load from ~/.config/waft/config.toml
        let config_path = dirs::config_dir()
            .context("No config directory")?
            .join("waft/config.toml");

        if !config_path.exists() {
            return Ok(<Plugin>Config::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let root: toml::Table = toml::from_str(&content)?;

        // Find plugin config
        if let Some(plugins) = root.get("plugins").and_then(|v| v.as_array()) {
            for plugin in plugins {
                if let Some(table) = plugin.as_table() {
                    if let Some(id) = table.get("id").and_then(|v| v.as_str()) {
                        if id == "waft::<plugin>-daemon" {
                            return toml::Value::Table(table.clone())
                                .try_into()
                                .context("Failed to parse config");
                        }
                    }
                }
            }
        }

        Ok(<Plugin>Config::default())
    }
}
```

### Step 3: Implement the PluginDaemon Trait

Convert your widget-building logic to the daemon trait:

```rust
#[async_trait::async_trait]
impl PluginDaemon for <Plugin>Daemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![
            NamedWidget {
                id: "<plugin>:main".to_string(),
                slot: Slot::FeatureToggles,  // or Header, Sliders, Body
                weight: 100,
                widget: self.build_main_widget(),
            }
        ]
    }

    async fn handle_action(
        &mut self,
        widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.id.as_str() {
            "toggle" => {
                // Handle toggle action
                self.toggle_feature().await?;
                Ok(())
            }
            "click" => {
                // Handle click action
                if let ActionParams::Value(value) = action.params {
                    self.set_value(value).await?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
```

### Step 4: Convert GTK Widgets to Widget Protocol

Map your GTK widgets to the serializable widget protocol:

**Before (GTK direct):**
```rust
let button = gtk::Button::builder()
    .label("Click Me")
    .css_classes(["primary"])
    .build();

button.connect_clicked(move |_| {
    // handle click
});
```

**After (Widget Protocol):**
```rust
use waft_plugin_sdk::builder::*;

let button = ButtonBuilder::new()
    .label("Click Me")
    .on_click("handle_click")
    .build();
```

**Before (GTK FeatureToggle via waft-plugin-api):**
```rust
use waft_plugin_api::ui::feature_toggle::*;

let toggle = FeatureToggleWidget::new(FeatureToggleProps {
    title: "Bluetooth".into(),
    icon: "bluetooth-active-symbolic".into(),
    active: true,
    details: Some("Connected".into()),
    on_toggle: Rc::new(RefCell::new(Some(Box::new(|_| {
        // handle toggle
    })))),
});
```

**After (Widget Protocol):**
```rust
use waft_plugin_sdk::builder::*;

let toggle = FeatureToggleBuilder::new("Bluetooth")
    .icon("bluetooth-active-symbolic")
    .details("Connected")
    .active(true)
    .on_toggle("toggle_bluetooth")
    .build();
```

### Step 5: Handle Configuration

Daemons must load their own config from TOML:

```rust
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct <Plugin>Config {
    #[serde(default)]
    on_click: String,

    // other config fields
}
```

Config file example (`~/.config/waft/config.toml`):
```toml
[[plugins]]
id = "waft::<plugin>-daemon"
on_click = "gnome-calendar"
```

### Step 6: Spawn External Commands Safely

For executing commands (e.g., on click):

```rust
async fn handle_action(&mut self, widget_id: String, action: Action) -> Result<...> {
    if action.id == "click" && !self.config.on_click.is_empty() {
        let cmd = self.config.on_click.clone();

        tokio::task::spawn_blocking(move || {
            match std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .spawn()
            {
                Ok(mut child) => {
                    if let Err(e) = child.wait() {
                        log::error!("Command failed: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("Failed to spawn command: {}", e);
                }
            }
        });
    }
    Ok(())
}
```

### Step 7: Update Cargo.toml

Add daemon binary configuration:

```toml
[package]
name = "waft-plugin-<name>"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]  # Keep for backward compatibility if needed

[[bin]]
name = "waft-<name>-daemon"
path = "bin/waft-<name>-daemon.rs"

[dependencies]
waft-plugin-sdk = { path = "../../crates/plugin-sdk" }
waft-ipc = { path = "../../crates/ipc" }
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
dirs = "5.0"
log = "0.4"
env_logger = "0.11"

# Plugin-specific dependencies
```

## Code Examples

### Complete Clock Daemon Example

See `/home/just-paja/Work/shell/sacrebleui/plugins/clock/bin/waft-clock-daemon.rs` for the reference implementation.

**Key highlights:**

1. **Configuration Loading**: Lines 39-72
2. **Widget Building**: Lines 82-106
3. **get_widgets Implementation**: Lines 111-118
4. **Action Handling**: Lines 120-149
5. **Main Entry Point**: Lines 152-169

### Widget Protocol Builders

The SDK provides ergonomic builders for common widgets:

```rust
use waft_plugin_sdk::builder::*;

// Feature Toggle (most common)
let toggle = FeatureToggleBuilder::new("Wi-Fi")
    .icon("network-wireless-symbolic")
    .details("Connected to NetworkName")
    .active(true)
    .expandable(true)
    .on_toggle("toggle_wifi")
    .build();

// Slider (volume, brightness)
let slider = SliderBuilder::new(0.75)
    .icon("audio-volume-high-symbolic")
    .on_value_change("set_volume")
    .on_icon_click("toggle_mute")
    .build();

// Menu Row (settings items)
let row = MenuRowBuilder::new("Settings")
    .icon("preferences-system-symbolic")
    .sublabel("Configure device")
    .on_click("open_settings")
    .build();

// Container (grouping)
let container = ContainerBuilder::new(Orientation::Vertical)
    .spacing(12)
    .css_class("device-list")
    .child(row1)
    .child(row2)
    .build();

// Button
let button = ButtonBuilder::new()
    .label("Power Off")
    .icon("system-shutdown-symbolic")
    .on_click("shutdown")
    .build();

// Label
let label = LabelBuilder::new("Status: Ready")
    .css_class("title-3")
    .css_class("dim-label")
    .build();

// Switch (for menu row trailing)
let switch = SwitchBuilder::new()
    .active(true)
    .on_toggle("toggle_feature")
    .build();
```

## Common Pitfalls

### 1. GTK Direct Access

**Problem**: Trying to use GTK widgets directly in daemon

**Wrong:**
```rust
let label = gtk::Label::new(Some("Text"));
```

**Right:**
```rust
let label = Widget::Label {
    text: "Text".to_string(),
    css_classes: vec![],
};
```

### 2. Callback Closures

**Problem**: Storing Rc<RefCell<Option<Box<dyn Fn()>>>>

**Wrong:**
```rust
on_toggle: Rc::new(RefCell::new(Some(Box::new(|| {
    // handle toggle
}))))
```

**Right:**
```rust
on_toggle: Action {
    id: "toggle".into(),
    params: ActionParams::None,
}
```

### 3. Tokio Runtime Issues

**Problem**: Creating separate tokio runtimes (cdylib TLS issues are GONE!)

**Wrong (old cdylib pattern):**
```rust
let handle = tokio::runtime::Handle::current();  // TLS issues
handle.spawn(async { ... });  // Panics in cdylib
```

**Right (daemon pattern):**
```rust
tokio::task::spawn(async { ... });  // Just works!
tokio::task::spawn_blocking(|| { ... });  // For blocking code
```

### 4. Missing #[tokio::main]

**Problem**: Not using tokio runtime in main

**Wrong:**
```rust
fn main() -> Result<()> {
    let server = PluginServer::new("plugin", daemon);
    server.run().await?;  // ERROR: await in non-async
    Ok(())
}
```

**Right:**
```rust
#[tokio::main]
async fn main() -> Result<()> {
    let server = PluginServer::new("plugin", daemon);
    server.run().await?;
    Ok(())
}
```

### 5. Socket Path Issues

**Problem**: Socket not found or permission errors

**Solution**: The SDK automatically handles socket paths:
- Default: `/run/user/{uid}/waft/plugins/{name}.sock`
- Override: Set `WAFT_PLUGIN_SOCKET_PATH` env var for testing

### 6. Config Loading Errors

**Problem**: Silently failing to load config

**Solution**: Use `unwrap_or_default()` and log:
```rust
let config = Self::load_config().unwrap_or_default();
log::debug!("Loaded config: {:?}", config);
```

### 7. Widget ID Consistency

**Problem**: Changing widget IDs breaks state tracking

**Solution**: Use stable, descriptive widget IDs:
```rust
// Good
id: "bluetooth:main_toggle".to_string()
id: "audio:volume_slider".to_string()

// Bad
id: format!("widget_{}", random_id)  // Changes every render
```

### 8. Missing Logging

**Problem**: Hard to debug without logs

**Solution**: Always initialize env_logger:
```rust
env_logger::Builder::from_env(
    env_logger::Env::default().default_filter_or("info")
).init();

log::info!("Starting daemon...");
log::debug!("Config: {:?}", config);
```

## Testing Your Daemon

### Manual Testing

1. **Build the daemon:**
   ```bash
   cargo build --bin waft-<plugin>-daemon
   ```

2. **Run daemon manually:**
   ```bash
   RUST_LOG=debug ./target/debug/waft-<plugin>-daemon
   ```

   You should see:
   ```
   [INFO] Starting <plugin> daemon...
   [INFO] Plugin server started: <plugin>-daemon
   [INFO] Socket path: /run/user/1000/waft/plugins/<plugin>-daemon.sock
   [INFO] Listening on: /run/user/1000/waft/plugins/<plugin>-daemon.sock
   ```

3. **Verify socket exists:**
   ```bash
   ls -l /run/user/$UID/waft/plugins/<plugin>-daemon.sock
   ```

4. **Run overview (in separate terminal):**
   ```bash
   RUST_LOG=debug cargo run --bin waft-overview
   ```

5. **Check connection logs:**
   ```
   [DEBUG] Client connected
   [DEBUG] Handling client connection
   [DEBUG] Received message: GetWidgets
   [DEBUG] Sending response: SetWidgets { widgets: [...] }
   ```

### Integration Testing

Use the SDK test utilities:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use waft_plugin_sdk::testing::*;

    #[tokio::test]
    async fn test_daemon_basic() {
        let daemon = <Plugin>Daemon::new().unwrap();

        // Test get_widgets
        let widgets = daemon.get_widgets();
        assert!(!widgets.is_empty());
        assert_eq!(widgets[0].id, "<plugin>:main");
    }

    #[tokio::test]
    async fn test_action_handling() {
        let mut daemon = <Plugin>Daemon::new().unwrap();

        let action = Action {
            id: "toggle".into(),
            params: ActionParams::None,
        };

        daemon.handle_action("<plugin>:main".into(), action).await.unwrap();

        // Verify state changed
        let widgets = daemon.get_widgets();
        // assert expected state
    }
}
```

### Performance Testing

Monitor resource usage:

```bash
# Start daemon
./target/debug/waft-<plugin>-daemon &
DAEMON_PID=$!

# Monitor CPU/memory
watch -n 1 "ps -p $DAEMON_PID -o %cpu,%mem,cmd"

# Stress test with rapid actions
# (send many GetWidgets requests)

# Check for memory leaks
kill -TERM $DAEMON_PID
```

## Systemd Integration

### Service File Template

Create `systemd/waft-<plugin>-daemon.service`:

```ini
[Unit]
Description=Waft <Plugin> Plugin Daemon
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/waft-<plugin>-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

### Installation

```bash
# Copy service file
sudo cp systemd/waft-<plugin>-daemon.service \
    /usr/lib/systemd/user/

# Reload systemd
systemctl --user daemon-reload

# Enable auto-start
systemctl --user enable waft-<plugin>-daemon.service

# Start now
systemctl --user start waft-<plugin>-daemon.service

# Check status
systemctl --user status waft-<plugin>-daemon.service

# View logs
journalctl --user -u waft-<plugin>-daemon.service -f
```

### Development Workflow

During development, DON'T use systemd:

```bash
# Run manually with logging
RUST_LOG=debug cargo run --bin waft-<plugin>-daemon

# In another terminal
RUST_LOG=debug cargo run --bin waft-overview
```

Only enable systemd service for production use:

```bash
# Disable during development
systemctl --user stop waft-<plugin>-daemon.service
systemctl --user disable waft-<plugin>-daemon.service
```

## Migration Checklist

Use this checklist when converting a plugin:

### Pre-Migration

- [ ] Read this entire guide
- [ ] Study the clock daemon implementation
- [ ] Back up current plugin code
- [ ] Review plugin's current features and dependencies

### Code Conversion

- [ ] Create `bin/waft-<plugin>-daemon.rs` with main entry point
- [ ] Define daemon state struct with config
- [ ] Implement config loading from TOML
- [ ] Implement `PluginDaemon` trait
  - [ ] `get_widgets()` returns all plugin widgets
  - [ ] `handle_action()` processes user interactions
- [ ] Convert GTK widgets to Widget protocol
  - [ ] Use builders (FeatureToggleBuilder, SliderBuilder, etc.)
  - [ ] Replace callbacks with Action structs
  - [ ] Map CSS classes correctly
- [ ] Update `Cargo.toml`
  - [ ] Add `[[bin]]` section
  - [ ] Add waft-plugin-sdk dependency
  - [ ] Add tokio with "full" features
  - [ ] Add async-trait, anyhow, env_logger

### Testing

- [ ] Build daemon: `cargo build --bin waft-<plugin>-daemon`
- [ ] Run daemon manually and verify socket creation
- [ ] Run overview and verify plugin connection
- [ ] Test all widget interactions
- [ ] Test configuration loading
- [ ] Test error handling (daemon restart, etc.)
- [ ] Add unit tests for daemon logic
- [ ] Add integration tests if applicable

### Systemd Integration

- [ ] Create systemd service file
- [ ] Test service installation
- [ ] Test auto-start on login
- [ ] Test restart on failure
- [ ] Verify logs in journalctl

### Documentation

- [ ] Update plugin README if exists
- [ ] Document config options in TOML comments
- [ ] Add usage examples
- [ ] Note any breaking changes

### Cleanup

- [ ] Remove old cdylib code (if fully migrated)
- [ ] Remove waft-plugin-api dependency (if not shared)
- [ ] Remove gtk4 dependency from daemon
- [ ] Update workspace Cargo.toml if needed

### Deployment

- [ ] Update installation scripts
- [ ] Update package manifests (RPM, DEB, etc.)
- [ ] Test fresh installation
- [ ] Test upgrade from old version

## Next Steps

After completing migration:

1. **Profile Performance**: Compare daemon vs cdylib
2. **Monitor Stability**: Watch for crashes over days/weeks
3. **Gather Metrics**: Track IPC overhead, memory usage
4. **Document Lessons**: Add to phase5-lessons-learned.md

## Getting Help

- Study reference: `/home/just-paja/Work/shell/sacrebleui/plugins/clock/`
- Check SDK docs: `/home/just-paja/Work/shell/sacrebleui/crates/plugin-sdk/`
- Review IPC protocol: `/home/just-paja/Work/shell/sacrebleui/crates/ipc/`
- See examples: `/home/just-paja/Work/shell/sacrebleui/crates/plugin-sdk/examples/`

## Summary

Daemon migration simplifies plugin development by eliminating cdylib complexities:

- **No more TLS issues**: Standard tokio usage works
- **Better isolation**: Crashes don't affect overview
- **Easier debugging**: Attach to individual processes
- **Cleaner code**: Widget protocol instead of GTK callbacks

The trade-off is losing direct GTK access, but the widget protocol covers all common use cases with ergonomic builders.

Follow the steps, use the checklist, and refer to the clock daemon as your guide.
