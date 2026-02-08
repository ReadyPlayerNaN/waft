# Waft Plugin SDK

The Waft Plugin SDK provides infrastructure for building plugin daemons that communicate with waft-overview via Unix sockets. Plugins run as standalone processes, enabling crash isolation, independent updates, and language-agnostic development.

## Table of Contents

- [Quick Start](#quick-start)
- [Architecture Overview](#architecture-overview)
- [Getting Started](#getting-started)
- [Widget Builders](#widget-builders)
- [IPC Protocol](#ipc-protocol)
- [Plugin Lifecycle](#plugin-lifecycle)
- [Best Practices](#best-practices)
- [Examples](#examples)

## Quick Start

Create a simple plugin in three steps:

```rust
use waft_plugin_sdk::*;

// 1. Define your plugin state
struct MyPlugin {
    enabled: bool,
}

// 2. Implement PluginDaemon trait
#[async_trait::async_trait]
impl PluginDaemon for MyPlugin {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![
            NamedWidget {
                id: "my_plugin:toggle".into(),
                slot: Slot::FeatureToggles,
                weight: 100,
                widget: Widget::FeatureToggle {
                    title: "My Feature".into(),
                    icon: "emblem-system-symbolic".into(),
                    details: None,
                    active: self.enabled,
                    busy: false,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle".into(),
                        params: ActionParams::None,
                    },
                },
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
                self.enabled = !self.enabled;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

// 3. Run the server
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let daemon = MyPlugin { enabled: false };
    let server = PluginServer::new("my_plugin", daemon);
    server.run().await?;
    Ok(())
}
```

## Architecture Overview

### Process Isolation Architecture

Waft uses a process isolation architecture where each plugin runs as a standalone daemon:

```
┌──────────────────────────────────────────────────────────┐
│                    waft-overview                         │
│  ┌─────────────┐   ┌──────────────┐   ┌──────────────┐ │
│  │   Widget    │   │  IPC Client  │   │ GTK Renderer │ │
│  │  Registry   │◄──┤   Manager    │◄──┤              │ │
│  └─────────────┘   └──────────────┘   └──────────────┘ │
└───────────────────────────┬──────────────────────────────┘
                            │ Unix Socket
                 ┌──────────┼──────────┐
                 │          │          │
      ┌──────────▼──┐  ┌───▼──────┐  ┌▼───────────┐
      │   Plugin 1  │  │ Plugin 2 │  │  Plugin 3  │
      │  (audio)    │  │(battery) │  │  (clock)   │
      │             │  │          │  │            │
      │ State       │  │ State    │  │ State      │
      │ D-Bus       │  │ Sysfs    │  │ Timer      │
      └─────────────┘  └──────────┘  └────────────┘
```

**Benefits:**
- **Crash isolation**: Plugin crash doesn't affect overview or other plugins
- **Independent updates**: Update plugins without recompiling overview
- **No Rust ABI issues**: No shared library linking, just message passing
- **Language agnostic**: Could write plugins in Python, Go, etc. (JSON protocol)
- **Normal async**: No cdylib tokio TLS issues

### Communication Flow

```
1. Plugin starts → Creates Unix socket at /run/user/{uid}/waft/plugins/{name}.sock
2. Overview discovers plugin → Connects to socket
3. Overview requests widgets → GetWidgets message
4. Plugin sends widget state → SetWidgets message
5. User interacts with UI → Overview sends TriggerAction
6. Plugin handles action → Updates state → Sends SetWidgets
7. Overview diffs widgets → Re-renders only changed widgets
```

## Getting Started

### Dependencies

Add to your `Cargo.toml`:

```toml
[dependencies]
waft-plugin-sdk = { path = "../plugin-sdk" }
tokio = { version = "1", features = ["full"] }
log = "0.4"
env_logger = "0.11"
```

### Project Structure

```
my-plugin/
├── Cargo.toml
├── src/
│   ├── main.rs         # Server setup and entry point
│   ├── daemon.rs       # PluginDaemon implementation
│   └── state.rs        # Plugin state management (optional)
└── README.md
```

### Minimal Plugin

```rust
use waft_plugin_sdk::*;

struct MinimalPlugin;

#[async_trait::async_trait]
impl PluginDaemon for MinimalPlugin {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![
            NamedWidget {
                id: "minimal:label".into(),
                slot: Slot::FeatureToggles,
                weight: 100,
                widget: Widget::Label {
                    text: "Hello from plugin!".into(),
                    css_classes: vec![],
                },
            }
        ]
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        _action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let daemon = MinimalPlugin;
    let server = PluginServer::new("minimal", daemon);
    server.run().await?;
    Ok(())
}
```

Run with: `cargo run`

The plugin will create a socket at `/run/user/{uid}/waft/plugins/minimal.sock`.

## Widget Builders

The SDK provides ergonomic builders for constructing widgets with sensible defaults.

### FeatureToggleBuilder

Most commonly used widget for on/off features:

```rust
use waft_plugin_sdk::builder::*;

let widget = FeatureToggleBuilder::new("Wi-Fi")
    .icon("network-wireless-symbolic")
    .details("Connected to HomeNetwork")
    .active(true)
    .expandable(true)
    .expanded_content(
        ContainerBuilder::new(Orientation::Vertical)
            .spacing(4)
            .child(
                MenuRowBuilder::new("HomeNetwork")
                    .icon("network-wireless-symbolic")
                    .trailing(SwitchBuilder::new().active(true).build())
                    .build()
            )
            .build()
    )
    .on_toggle("toggle_wifi")
    .build();
```

**Builder methods:**
- `icon(name)` - Icon name (default: "emblem-system-symbolic")
- `details(text)` - Subtitle text
- `active(bool)` - Toggle state (default: false)
- `busy(bool)` - Shows spinner instead of toggle (default: false)
- `expandable(bool)` - Can expand to show more content (default: false)
- `expanded_content(widget)` - Content shown when expanded (auto-enables expandable)
- `on_toggle(action_id)` - Action ID for toggle events
- `on_toggle_action(action)` - Full Action object

### SliderBuilder

For volume, brightness, or other continuous values (0.0 to 1.0):

```rust
let widget = SliderBuilder::new(0.75)
    .icon("audio-volume-high-symbolic")
    .muted(false)
    .on_value_change("set_volume")
    .on_icon_click("toggle_mute")
    .build();
```

**Builder methods:**
- `icon(name)` - Icon name (default: "emblem-system-symbolic")
- `muted(bool)` - Semantic mute state (renderer picks icon, default: false)
- `expandable(bool)` - Can expand (default: false)
- `expanded_content(widget)` - Expanded content
- `on_value_change(action_id)` - Action for slider changes
- `on_icon_click(action_id)` - Action for icon clicks

**Note:** Values are automatically clamped to [0.0, 1.0]

### MenuRowBuilder

For menu items with labels, icons, and trailing widgets:

```rust
let widget = MenuRowBuilder::new("Settings")
    .icon("preferences-system-symbolic")
    .sublabel("Configure system")
    .trailing(SwitchBuilder::new().active(true).build())
    .sensitive(true)
    .on_click("open_settings")
    .build();
```

**Builder methods:**
- `icon(name)` - Icon name (optional)
- `sublabel(text)` - Secondary label below main label
- `trailing(widget)` - Widget on the right (Switch, Spinner, Checkmark)
- `sensitive(bool)` - Whether clickable (default: true)
- `on_click(action_id)` - Action for click events

### ContainerBuilder

For organizing child widgets vertically or horizontally:

```rust
let widget = ContainerBuilder::new(Orientation::Vertical)
    .spacing(12)
    .css_class("menu-section")
    .child(LabelBuilder::new("Header").build())
    .child(MenuRowBuilder::new("Item 1").build())
    .child(MenuRowBuilder::new("Item 2").build())
    .build();
```

**Builder methods:**
- `spacing(pixels)` - Space between children (default: 0)
- `css_class(name)` - Add CSS class
- `css_classes(vec)` - Add multiple CSS classes
- `child(widget)` - Add single child
- `children(vec)` - Add multiple children

### Other Builders

**SwitchBuilder:**
```rust
SwitchBuilder::new()
    .active(true)
    .sensitive(true)
    .on_toggle("toggle_feature")
    .build()
```

**ButtonBuilder:**
```rust
ButtonBuilder::new()
    .label("Power Off")
    .icon("system-shutdown-symbolic")
    .on_click("shutdown")
    .build()
```

**LabelBuilder:**
```rust
LabelBuilder::new("Status text")
    .css_class("title")
    .css_class("bold")
    .build()
```

**Primitives (no builder needed):**
```rust
Widget::Spinner { spinning: true }
Widget::Checkmark { visible: true }
```

## IPC Protocol

### Socket Location

Plugins create Unix sockets at:
```
/run/user/{uid}/waft/plugins/{plugin_name}.sock
```

The directory is automatically created by the plugin server.

### Message Framing

Messages use length-prefixed framing:
```
[4 bytes: u32 big-endian length][N bytes: JSON payload]
```

Maximum frame size: 10MB

### Protocol Messages

#### Overview → Plugin

**GetWidgets** - Request current widget state:
```json
{
  "type": "GetWidgets"
}
```

**TriggerAction** - User interaction:
```json
{
  "type": "TriggerAction",
  "widget_id": "audio:output",
  "action": {
    "id": "set_volume",
    "params": { "Value": 0.75 }
  }
}
```

#### Plugin → Overview

**SetWidgets** - Full widget set (replaces all):
```json
{
  "type": "SetWidgets",
  "widgets": [
    {
      "id": "audio:output",
      "slot": "Controls",
      "weight": 100,
      "widget": {
        "Slider": {
          "icon": "audio-volume-high-symbolic",
          "value": 0.75,
          "muted": false,
          "expandable": false,
          "expanded_content": null,
          "on_value_change": {
            "id": "set_volume",
            "params": { "Value": 0.75 }
          },
          "on_icon_click": {
            "id": "toggle_mute",
            "params": "None"
          }
        }
      }
    }
  ]
}
```

**UpdateWidget** - Single widget update (optional optimization):
```json
{
  "type": "UpdateWidget",
  "id": "audio:output",
  "widget": { /* widget tree */ }
}
```

**RemoveWidget** - Remove widget by ID:
```json
{
  "type": "RemoveWidget",
  "id": "audio:output"
}
```

### Action Parameters

```rust
pub enum ActionParams {
    None,                                    // Simple actions (toggle, click)
    Value(f64),                              // Sliders, numeric values
    String(String),                          // Device IDs, text input
    Map(HashMap<String, serde_json::Value>), // Complex structured data
}
```

### Widget Slots

Widgets are organized into three slots:

- **`Slot::FeatureToggles`** - Primary toggleable features (Wi-Fi, Bluetooth, etc.)
- **`Slot::Controls`** - Continuous controls (volume, brightness sliders)
- **`Slot::Actions`** - Quick action buttons (power off, settings, etc.)

Widgets within each slot are sorted by `weight` (lower = higher priority).

## Plugin Lifecycle

### 1. Startup

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    log::info!("Starting my plugin...");

    // Initialize plugin state (connect to D-Bus, read config, etc.)
    let daemon = MyPlugin::new().await?;

    // Create and run server (blocks until shutdown)
    let server = PluginServer::new("my_plugin", daemon);
    server.run().await?;

    Ok(())
}
```

### 2. Socket Creation

The `PluginServer` automatically:
- Creates `/run/user/{uid}/waft/plugins/{name}.sock`
- Creates parent directories if needed
- Removes stale sockets from previous runs
- Binds Unix listener

### 3. Connection Handling

When overview connects:
1. Server spawns a tokio task per connection
2. Reads framed messages in a loop
3. Calls `handle_message()` on daemon
4. Sends responses
5. Handles disconnects gracefully

### 4. Message Handling

**GetWidgets flow:**
```
Overview sends GetWidgets
    ↓
Server calls daemon.get_widgets()
    ↓
Server sends SetWidgets response with current state
```

**TriggerAction flow:**
```
User clicks toggle in UI
    ↓
Overview sends TriggerAction { widget_id, action }
    ↓
Server calls daemon.handle_action(widget_id, action).await
    ↓
Daemon updates internal state
    ↓
Server calls daemon.get_widgets()
    ↓
Server sends SetWidgets response with updated state
    ↓
Overview diffs and re-renders changed widgets
```

### 5. State Updates

**State-based model:** Plugins send the entire widget set when state changes.

```rust
async fn handle_action(&mut self, widget_id: String, action: Action)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    match action.id.as_str() {
        "toggle_wifi" => {
            // Perform async operation (D-Bus call, etc.)
            self.wifi_manager.set_enabled(!self.wifi_enabled).await?;

            // Update internal state
            self.wifi_enabled = !self.wifi_enabled;

            // No need to send update - server automatically calls get_widgets()
            // and sends SetWidgets response
            Ok(())
        }
        _ => Ok(())
    }
}
```

The server automatically:
- Calls `get_widgets()` after `handle_action()` returns
- Sends updated widget set to overview
- Overview performs diff and only re-renders changed widgets

### 6. Shutdown

On SIGTERM/SIGINT:
- Server stops accepting connections
- Active connections complete current operations
- Socket file is removed
- Process exits

## Best Practices

### Widget IDs

Use namespaced IDs to avoid collisions:

```rust
// Good: namespace:specific_widget
"audio:output_slider"
"audio:input_slider"
"bluetooth:adapter0"
"battery:main"

// Bad: generic IDs
"slider"
"toggle"
"button"
```

### Action IDs

Use descriptive, semantic action IDs:

```rust
// Good
"toggle_wifi"
"set_volume"
"select_device"
"open_settings"

// Bad
"click"
"change"
"action1"
```

### State Management

Keep plugin state in a single struct:

```rust
struct AudioPlugin {
    output_volume: f64,
    input_volume: f64,
    output_muted: bool,
    input_muted: bool,
    default_sink: String,
}

impl AudioPlugin {
    fn build_output_slider(&self) -> NamedWidget {
        NamedWidget {
            id: "audio:output".into(),
            slot: Slot::Controls,
            weight: 100,
            widget: SliderBuilder::new(self.output_volume)
                .icon("audio-volume-high-symbolic")
                .muted(self.output_muted)
                .on_value_change("set_output_volume")
                .on_icon_click("toggle_output_mute")
                .build(),
        }
    }
}

#[async_trait::async_trait]
impl PluginDaemon for AudioPlugin {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![
            self.build_output_slider(),
            self.build_input_slider(),
            // ...
        ]
    }

    async fn handle_action(&mut self, widget_id: String, action: Action)
        -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    {
        match (widget_id.as_str(), action.id.as_str()) {
            ("audio:output", "set_output_volume") => {
                if let ActionParams::Value(v) = action.params {
                    self.output_volume = v;
                    // Perform actual volume change...
                }
            }
            ("audio:output", "toggle_output_mute") => {
                self.output_muted = !self.output_muted;
                // Perform actual mute toggle...
            }
            _ => {}
        }
        Ok(())
    }
}
```

### Error Handling

Return errors from `handle_action()` for actionable failures:

```rust
async fn handle_action(&mut self, widget_id: String, action: Action)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    match action.id.as_str() {
        "toggle_bluetooth" => {
            // Propagate D-Bus errors
            self.bluetooth_adapter
                .set_powered(!self.enabled)
                .await
                .map_err(|e| format!("Failed to toggle Bluetooth: {}", e))?;

            self.enabled = !self.enabled;
            Ok(())
        }
        _ => Ok(()) // Unknown actions are not errors
    }
}
```

### Logging

Use structured logging for debugging:

```rust
log::info!("Plugin started: {}", plugin_name);
log::debug!("Received action: widget={}, action={:?}", widget_id, action.id);
log::warn!("Failed to update state: {}", error);
log::error!("Fatal error: {}", error);
```

### Async Operations

The `handle_action()` method is async - use it for I/O:

```rust
async fn handle_action(&mut self, widget_id: String, action: Action)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    match action.id.as_str() {
        "scan_wifi" => {
            // Set busy state immediately
            self.scanning = true;

            // Perform async scan (could take seconds)
            let networks = self.wifi_manager.scan().await?;

            // Update state
            self.available_networks = networks;
            self.scanning = false;

            Ok(())
        }
        _ => Ok(())
    }
}
```

The server will send updated widgets (with `busy: true`) immediately after the method returns.

### Widget Weights

Use consistent weight ranges for ordering:

```rust
// Primary features: 0-99
NamedWidget {
    id: "wifi:toggle",
    slot: Slot::FeatureToggles,
    weight: 10,  // High priority
    // ...
}

// Secondary features: 100-199
NamedWidget {
    id: "vpn:toggle",
    slot: Slot::FeatureToggles,
    weight: 100,
    // ...
}

// Controls: 0-99
NamedWidget {
    id: "audio:volume",
    slot: Slot::Controls,
    weight: 10,  // Most important control
    // ...
}
```

Lower weight = higher in the list.

## Examples

### Example: Feature Toggle with Expanded Content

```rust
fn build_bluetooth_toggle(&self) -> NamedWidget {
    let expanded = if !self.devices.is_empty() {
        Some(
            ContainerBuilder::new(Orientation::Vertical)
                .spacing(0)
                .children(
                    self.devices.iter().map(|device| {
                        MenuRowBuilder::new(&device.name)
                            .icon("bluetooth-symbolic")
                            .sublabel(&device.address)
                            .trailing(
                                if device.connected {
                                    Widget::Checkmark { visible: true }
                                } else {
                                    Widget::Button {
                                        label: Some("Connect".into()),
                                        icon: None,
                                        on_click: Action {
                                            id: format!("connect:{}", device.address),
                                            params: ActionParams::None,
                                        },
                                    }
                                }
                            )
                            .on_click(format!("toggle_device:{}", device.address))
                            .build()
                    }).collect()
                )
                .build()
        )
    } else {
        None
    };

    NamedWidget {
        id: "bluetooth:adapter".into(),
        slot: Slot::FeatureToggles,
        weight: 20,
        widget: FeatureToggleBuilder::new("Bluetooth")
            .icon(if self.enabled {
                "bluetooth-active-symbolic"
            } else {
                "bluetooth-disabled-symbolic"
            })
            .details(if self.enabled {
                Some(format!("{} devices", self.devices.len()))
            } else {
                None
            })
            .active(self.enabled)
            .busy(self.scanning)
            .expandable(!self.devices.is_empty())
            .expanded_content(expanded.unwrap_or_else(||
                LabelBuilder::new("No devices").build()
            ))
            .on_toggle("toggle_bluetooth")
            .build(),
    }
}
```

### Example: Volume Slider with Device Selector

```rust
fn build_audio_widgets(&self) -> Vec<NamedWidget> {
    let mut widgets = vec![
        // Main volume slider
        NamedWidget {
            id: "audio:output".into(),
            slot: Slot::Controls,
            weight: 10,
            widget: SliderBuilder::new(self.volume)
                .icon(if self.muted {
                    "audio-volume-muted-symbolic"
                } else if self.volume > 0.66 {
                    "audio-volume-high-symbolic"
                } else if self.volume > 0.33 {
                    "audio-volume-medium-symbolic"
                } else {
                    "audio-volume-low-symbolic"
                }
                .muted(self.muted)
                .on_value_change("set_volume")
                .on_icon_click("toggle_mute")
                .build(),
        }
    ];

    // Add device selector if multiple outputs available
    if self.outputs.len() > 1 {
        let device_menu = ContainerBuilder::new(Orientation::Vertical)
            .spacing(0)
            .children(
                self.outputs.iter().map(|output| {
                    MenuRowBuilder::new(&output.description)
                        .icon("audio-card-symbolic")
                        .trailing(
                            if output.name == self.default_output {
                                Widget::Checkmark { visible: true }
                            } else {
                                Widget::Checkmark { visible: false }
                            }
                        )
                        .on_click(format!("select_output:{}", output.name))
                        .build()
                }).collect()
            )
            .build();

        widgets[0].widget = match widgets[0].widget.clone() {
            Widget::Slider { icon, value, muted, on_value_change, on_icon_click, .. } => {
                Widget::Slider {
                    icon,
                    value,
                    muted,
                    expandable: true,
                    expanded_content: Some(Box::new(device_menu)),
                    on_value_change,
                    on_icon_click,
                }
            }
            _ => unreachable!()
        };
    }

    widgets
}
```

### Example: Battery Status with System Actions

```rust
fn get_widgets(&self) -> Vec<NamedWidget> {
    vec![
        // Battery indicator (feature toggle format)
        NamedWidget {
            id: "battery:status".into(),
            slot: Slot::FeatureToggles,
            weight: 90,  // Lower priority
            widget: FeatureToggleBuilder::new("Battery")
                .icon(self.get_battery_icon())
                .details(Some(format!("{}% • {}",
                    self.percentage,
                    self.time_remaining
                )))
                .active(self.charging)
                .busy(false)
                .expandable(false)
                .on_toggle("") // No action for read-only widget
                .build(),
        },

        // Power actions
        NamedWidget {
            id: "power:actions".into(),
            slot: Slot::Actions,
            weight: 100,
            widget: ContainerBuilder::new(Orientation::Horizontal)
                .spacing(8)
                .child(
                    ButtonBuilder::new()
                        .label("Sleep")
                        .icon("system-suspend-symbolic")
                        .on_click("sleep")
                        .build()
                )
                .child(
                    ButtonBuilder::new()
                        .label("Power Off")
                        .icon("system-shutdown-symbolic")
                        .on_click("poweroff")
                        .build()
                )
                .build(),
        }
    ]
}

fn get_battery_icon(&self) -> &'static str {
    match (self.charging, self.percentage) {
        (true, _) => "battery-charging-symbolic",
        (false, 0..=20) => "battery-empty-symbolic",
        (false, 21..=50) => "battery-low-symbolic",
        (false, 51..=80) => "battery-medium-symbolic",
        _ => "battery-full-symbolic",
    }
}
```

## See Also

- [waft-ipc crate](../ipc/) - Protocol message types and widget definitions
- [examples/simple_plugin.rs](examples/simple_plugin.rs) - Complete working example
- [Process Isolation Architecture Plan](../../docs/PROCESS_ISOLATION_PLAN.md) - Design rationale

## License

This project is licensed under the same terms as the main Waft project.
