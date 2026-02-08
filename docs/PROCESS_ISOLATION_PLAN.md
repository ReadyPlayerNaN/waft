# Process Isolation Architecture Plan

**Status**: Planning
**Date**: 2026-02-07
**Decision**: Migrate from cdylib in-process plugins to process-isolated plugins with declarative UI

## Problem Statement

Current architecture uses Rust cdylib plugins with direct GTK widget creation:
- ❌ Rust ABI instability prevents truly independent plugin updates
- ❌ Requires same rustc version across all packages
- ❌ Shared dependency versions (gtk4, libadwaita) must match exactly
- ❌ "Discipline required" for coordinated builds is fragile for side project

**Goal**: Enable separate Arch Linux packages with true independent updates, no coordination required.

## Architectural Decision

**Option 2: Full Process Isolation with Declarative UI Protocol**

Accept the complexity, build it properly. Split each plugin into:
- **Backend daemon** (standalone binary): System integration, state management, business logic
- **UI protocol**: Declarative widget descriptions sent to overview
- **Renderer** (in overview): Builds GTK widgets from protocol

### Why This Works

```
┌─────────────────────────────────────────────────────┐
│         True Independence Achieved                  │
├─────────────────────────────────────────────────────┤
│                                                     │
│  waft-plugin-audio v1.0.1 update:                  │
│  - Rebuild with any Rust version ✓                 │
│  - Uses protocol v1.0 (stable) ✓                   │
│  - Ship independently ✓                            │
│  - No coordination with overview ✓                 │
│  - No shared gtk4 dependency ✓                     │
│                                                     │
│  Benefits:                                          │
│  - Crash isolation (plugin crash ≠ overview crash) │
│  - Normal tokio (no cdylib TLS issues)             │
│  - Language agnostic (could use Python, Go)        │
│  - Multiple UI frontends possible                  │
│                                                     │
└─────────────────────────────────────────────────────┘
```

## Key Design Decisions

### 1. IPC Transport

**Choice**: Unix domain sockets + bincode/JSON serialization

- **Not D-Bus**: Too slow for high-frequency updates (volume sliders)
- **Socket path**: `/run/user/1000/waft/plugins/{plugin-name}.sock`
- **Framing**: Length-prefixed messages `[4 bytes length][N bytes payload]`
- **Serialization**: Start with JSON (debuggable), optimize to bincode later

### 2. Multiple Widgets: State-Based Updates

Plugins provide a **set of named widgets**. Overview diffs by ID:

```rust
#[derive(Serialize, Deserialize)]
struct WidgetSet {
    widgets: Vec<NamedWidget>,
}

#[derive(Serialize, Deserialize)]
struct NamedWidget {
    id: String,           // "bluetooth:adapter0"
    slot: Slot,           // FeatureToggles, Controls, Actions
    weight: u32,          // Ordering
    widget: Widget,       // Widget tree
}
```

**Update model**:
- Plugin maintains state
- When state changes, send entire widget set
- Overview diffs by ID (add/update/remove)
- **Optional optimization**: Incremental updates for single widgets

**Advantage**: Plugin doesn't track "did I add this?" - just sends current state.

### 3. Widget Vocabulary

Define **specific widgets waft needs**, not a general UI framework:

```rust
pub enum Widget {
    FeatureToggle {
        title: String,
        icon: String,
        details: Option<String>,
        active: bool,
        busy: bool,
        expandable: bool,
        expanded_content: Option<Box<Widget>>,
        on_toggle: Action,
    },

    Slider {
        icon: String,
        value: f64,
        muted: bool,  // Semantic state, renderer picks icon
        expandable: bool,
        expanded_content: Option<Box<Widget>>,
        on_value_change: Action,
        on_icon_click: Action,
    },

    Container {
        orientation: Orientation,
        spacing: u32,
        css_classes: Vec<String>,
        children: Vec<Widget>,
    },

    MenuRow {
        icon: Option<String>,
        label: String,
        sublabel: Option<String>,
        trailing: Option<Box<Widget>>,  // Switch, Spinner, Checkmark
        sensitive: bool,
        on_click: Option<Action>,
    },

    Switch { active: bool, sensitive: bool, on_toggle: Action },
    Spinner { spinning: bool },
    Checkmark { visible: bool },
    Button { label: Option<String>, icon: Option<String>, on_click: Action },
    Label { text: String, css_classes: Vec<String> },
}
```

**Start small**: Implement what we need now, add more later.

### 4. Action System

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct Action {
    pub id: String,           // "toggle_power", "set_volume"
    pub params: ActionParams,
}

pub enum ActionParams {
    None,
    Value(f64),              // For sliders
    String(String),          // For device IDs
    Map(HashMap<String, serde_json::Value>),  // Flexible
}
```

**Flow**:
1. User interacts → Overview captures action
2. Overview sends: `TriggerAction { widget_id: "audio:output", action: Action {...} }`
3. Plugin handles async operation
4. Plugin updates state, sends: `UpdateWidget { widget: Slider { value: 0.75, ... } }`
5. Overview re-renders

### 5. Message Protocol

```rust
// Overview → Plugin
pub enum OverviewMessage {
    GetWidgets,  // Request initial state
    TriggerAction { widget_id: String, action: Action },
}

// Plugin → Overview
pub enum PluginMessage {
    SetWidgets { widgets: Vec<NamedWidget> },      // Full update
    UpdateWidget { id: String, widget: Widget },   // Incremental (optional)
    RemoveWidget { id: String },                   // Remove
}
```

## Implementation Plan

### Phase 1: Build the GTK Renderer ⭐ START HERE

**Goal**: Build concrete UI components first, let protocol emerge from reality.

**Create `waft-ui-gtk` crate**:

```
waft-ui-gtk/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API
    ├── types.rs            # Widget types (will move to protocol crate later)
    ├── renderer.rs         # Main: Widget → gtk::Widget
    ├── widgets/
    │   ├── mod.rs
    │   ├── feature_toggle.rs
    │   ├── slider.rs
    │   ├── menu_row.rs
    │   ├── container.rs
    │   ├── button.rs
    │   ├── label.rs
    │   └── primitives.rs   # Switch, Spinner, Checkmark
    └── utils/
        ├── icon.rs         # Icon resolution
        └── menu_state.rs   # Menu coordination
```

**Tasks**:
- [ ] Create `waft-ui-gtk` crate
- [ ] Define `Widget` enum in `types.rs`
- [ ] Implement `WidgetRenderer` with action callbacks
- [ ] Implement `FeatureToggle` renderer (most complex)
- [ ] Implement `Slider` renderer
- [ ] Implement `MenuRow` renderer
- [ ] Implement `Container` renderer
- [ ] Implement primitives (Switch, Spinner, Checkmark, Button, Label)
- [ ] Handle CSS classes, icon resolution, muted states

**Success criteria**:
- Can render all widget types from declarative descriptions
- Matches visual appearance of current UI
- Action callbacks work correctly

### Phase 2: Test with Mock Data

**Goal**: Validate the renderer works before building IPC.

**Tasks**:
- [ ] Create test application that uses `waft-ui-gtk`
- [ ] Build mock `Widget` structs for each plugin type
- [ ] Render them and verify visual/interaction correctness
- [ ] Test dynamic updates (add/remove/update widgets)
- [ ] Test menu coordination with multiple expandable widgets

**Success criteria**:
- Test app looks identical to current overview
- All interactions work (toggles, sliders, menus, clicks)
- Dynamic widget updates work smoothly

### Phase 3: Extract Protocol Crate

**Goal**: Formalize the protocol based on what the renderer needs.

**Create `waft-ipc-protocol` crate**:

```
waft-ipc-protocol/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── widget.rs           # Widget types (moved from waft-ui-gtk)
    ├── message.rs          # OverviewMessage, PluginMessage
    ├── action.rs           # Action, ActionParams
    └── transport.rs        # Socket framing, serialization helpers
```

**Tasks**:
- [ ] Move `Widget`, `NamedWidget`, `Slot` from `waft-ui-gtk/types.rs`
- [ ] Define `OverviewMessage` and `PluginMessage` enums
- [ ] Implement socket framing (length-prefixed messages)
- [ ] Add serialization helpers (JSON/bincode)
- [ ] Document protocol versioning strategy
- [ ] Update `waft-ui-gtk` to depend on `waft-ipc-protocol`

**Success criteria**:
- Protocol types are well-documented
- Serialization/deserialization works
- Socket framing is tested

### Phase 4: Build IPC Infrastructure

**Goal**: Enable overview ↔ plugin communication.

**In `waft-overview`**:
- [ ] Plugin discovery (scan socket directory)
- [ ] Socket client (connect to plugins)
- [ ] Message send/receive loop
- [ ] Widget registry (track widgets by plugin)
- [ ] Diff algorithm (compare widget sets, apply changes)
- [ ] Action routing (user interaction → plugin socket)

**Create `waft-plugin-sdk` helper crate**:
- [ ] Plugin daemon boilerplate
- [ ] Socket server setup
- [ ] Message handling helpers
- [ ] Builder patterns for widgets

**Success criteria**:
- Overview can discover and connect to plugins
- Messages flow bidirectionally
- Action callbacks reach plugins
- Widget updates trigger re-renders

### Phase 5: Prototype Plugin

**Goal**: Validate the full architecture with a real plugin.

**Convert one simple plugin** (caffeine or darkman):

```
plugins/caffeine/
├── daemon/              # New: standalone binary
│   ├── Cargo.toml
│   └── src/
│       └── main.rs     # Socket server, D-Bus client, state management
└── (remove lib.rs)     # Old cdylib code deleted
```

**Tasks**:
- [ ] Implement daemon with socket server
- [ ] Integrate with D-Bus (if needed)
- [ ] Build widget descriptions from state
- [ ] Handle actions (toggle, etc.)
- [ ] Test full loop: user click → daemon → state update → UI update

**Success criteria**:
- Plugin works identically to current version
- Can be updated independently
- No gtk4 dependency in daemon

### Phase 6: Migration

**Migrate remaining plugins** (order by complexity):

1. **Simple toggles**: darkman, caffeine
2. **Simple with menus**: battery, brightness
3. **Medium complexity**: clock, sunsetr, weather, keyboard-layout
4. **Complex state**: audio, blueman, systemd-actions
5. **Very complex**: networkmanager, eds-agenda, notifications

**For each plugin**:
- [ ] Create daemon binary
- [ ] Move D-Bus/system integration to daemon
- [ ] Implement widget building from state
- [ ] Handle all actions
- [ ] Remove cdylib code
- [ ] Test thoroughly

### Phase 7: Polish & Documentation

**Tasks**:
- [ ] Protocol versioning and compatibility strategy
- [ ] Performance optimization (switch to bincode if needed)
- [ ] Error handling and recovery (plugin crashes, socket errors)
- [ ] Logging and debugging tools
- [ ] Developer documentation (how to write plugins)
- [ ] Migration guide for plugin authors

## Crate Dependencies

```
waft-ipc-protocol
  └─ (no dependencies, just serde)

waft-ui-gtk
  ├─ waft-ipc-protocol
  ├─ waft-core (for MenuStore)
  ├─ gtk4, libadwaita
  └─ serde

waft-plugin-sdk
  ├─ waft-ipc-protocol
  ├─ tokio, zbus (optional, for common patterns)
  └─ serde, serde_json

waft-overview
  ├─ waft-ui-gtk
  ├─ waft-ipc-protocol
  └─ (everything else it needs)

waft-plugin-* (daemons)
  ├─ waft-ipc-protocol
  ├─ waft-plugin-sdk
  └─ (plugin-specific deps: zbus, etc.)
  └─ (NO gtk4, NO waft-core)
```

## Example: Bluetooth Plugin (New Architecture)

```rust
// plugins/bluetooth/daemon/src/main.rs

use waft_ipc_protocol::*;
use waft_plugin_sdk::*;

struct BluetoothDaemon {
    adapters: HashMap<String, AdapterState>,
}

impl BluetoothDaemon {
    fn build_widgets(&self) -> Vec<NamedWidget> {
        self.adapters.iter().map(|(path, adapter)| {
            NamedWidget {
                id: format!("bluetooth:{}", path),
                slot: Slot::FeatureToggles,
                weight: 100,
                widget: Widget::FeatureToggle {
                    title: adapter.name.clone(),
                    icon: "bluetooth-symbolic".into(),
                    details: Some(format!("{} connected", adapter.connected_count)),
                    active: adapter.powered,
                    busy: adapter.busy,
                    expandable: true,
                    expanded_content: Some(Box::new(self.build_device_menu(adapter))),
                    on_toggle: Action {
                        id: "toggle_power".into(),
                        params: ActionParams::String(path.clone()),
                    },
                },
            }
        }).collect()
    }

    fn build_device_menu(&self, adapter: &AdapterState) -> Widget {
        Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 4,
            css_classes: vec![],
            children: adapter.devices.iter().map(|dev| {
                Widget::MenuRow {
                    icon: Some(dev.icon.clone()),
                    label: dev.name.clone(),
                    sublabel: None,
                    trailing: Some(Box::new(match dev.state {
                        DeviceState::Connected => Widget::Switch {
                            active: true,
                            sensitive: true,
                            on_toggle: Action {
                                id: "disconnect".into(),
                                params: ActionParams::String(dev.path.clone()),
                            },
                        },
                        DeviceState::Connecting => Widget::Spinner { spinning: true },
                        DeviceState::Disconnected => Widget::Switch {
                            active: false,
                            sensitive: true,
                            on_toggle: Action {
                                id: "connect".into(),
                                params: ActionParams::String(dev.path.clone()),
                            },
                        },
                    })),
                    sensitive: dev.state != DeviceState::Connecting,
                    on_click: Some(Action {
                        id: "toggle_device".into(),
                        params: ActionParams::String(dev.path.clone()),
                    }),
                }
            }).collect(),
        }
    }

    async fn handle_action(&mut self, action: Action) {
        match action.id.as_str() {
            "toggle_power" => { /* ... */ }
            "connect" => { /* ... */ }
            "disconnect" => { /* ... */ }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    let daemon = BluetoothDaemon::new();
    let server = PluginServer::new("bluetooth", daemon);
    server.run().await;
}
```

**Note how much simpler**:
- No signal blocking/unblocking
- No manual widget updates
- No GTK code at all
- No cdylib TLS issues
- Just build state, send it

## Open Questions

1. **Icon resolution**: Plugin sends icon name, renderer resolves theme? Or plugin sends themed icon name with fallbacks?
2. **CSS classes**: Who decides which classes to apply? Plugin (explicit) or renderer (semantic)?
3. **Animation timing**: Revealer transitions - renderer decides timing?
4. **Error handling**: What happens if plugin crashes? Auto-restart? Show error widget?
5. **Performance**: How many widgets can we diff efficiently? Need benchmarks.
6. **Backwards compatibility**: How to version the protocol? Semver on `waft-ipc-protocol` crate?

## Success Metrics

- [ ] Can update one plugin package without touching others
- [ ] Plugin daemon has no gtk4 dependency
- [ ] UI looks/feels identical to current implementation
- [ ] No performance regression (smooth animations, responsive)
- [ ] Developer experience is good (easy to write new plugins)
- [ ] All 14 current plugins migrated successfully

## Timeline Estimate

- **Phase 1** (GTK Renderer): 1-2 weeks
- **Phase 2** (Mock Testing): 3-5 days
- **Phase 3** (Protocol Crate): 2-3 days
- **Phase 4** (IPC Infrastructure): 1 week
- **Phase 5** (Prototype Plugin): 3-5 days
- **Phase 6** (Migration): 2-3 weeks (1-2 days per plugin)
- **Phase 7** (Polish): 1 week

**Total**: ~6-8 weeks of focused work

## Notes

- Start with Phase 1 (renderer) - build concrete before abstract
- Widget types will evolve as we implement - that's expected
- Protocol will naturally emerge from renderer requirements
- Don't over-engineer - build what we need, iterate

## References

- Original discussion: explore mode session on 2026-02-07
- Current architecture: `plugins/*/src/lib.rs` (cdylib plugins)
- Existing UI widgets: `plugin-api/src/ui/` (will inform new widget types)
