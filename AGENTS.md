# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) and other AI agents working with code in this repository.

## Agent Rule

Before implementing changes, ask for clarification on DBus ownership, threading boundaries, and API changes. Do not start coding until key behavioral decisions are confirmed.

## Build & Test Commands

```bash
cargo build          # Build the project
cargo test           # Run all tests
cargo test --lib     # Run library unit tests only
cargo test notifications_store_reduce  # Run specific test module
```

## Architecture Overview

**sacrebleui** is a Wayland-only overlay UI application using Relm4 + libadwaita. It acts as a notification server (owns `org.freedesktop.Notifications` on DBus) and provides an overlay panel with feature toggles.

### Core Components

- **Entry point:** `src/main.rs` → `app::run()`
- **App model:** `src/app.rs` - Relm4 `SimpleComponent`, manages overlay window (layer-shell), plugin registry, IPC commands
- **Plugin system:** `src/plugin.rs`, `src/plugin_registry.rs` - Non-`Send` plugin trait for GTK compatibility
- **Notifications:** `src/features/notifications/` - DBus server, reducer-based state management, UI components
- **IPC:** `src/ipc/` - JSON commands over Unix socket (show/hide/toggle/ping)

### Plugin System

Plugins implement the `Plugin` trait (`#[async_trait(?Send)]`):
- `init()` - async initialization (DBus, channels, pure Rust state only)
- `create_elements()` / `get_widgets()` - GTK widget construction (after GTK init)
- Widgets placed into slots: `Info`, `Controls`, `Header`
- Registration is manual in `app.rs` via `PluginRegistry`
- Registry stores plugins behind `Arc<Mutex<Box<dyn Plugin>>>`

**Documentation requirement:** When adding or modifying plugin configuration options, always update the plugin's README.md file (`src/features/<plugin>/README.md`) to document the new/changed options.

### State Management

- **Notifications store:** `AsyncReducible` reducer pattern in `src/features/notifications/store.rs`
- **Domain types:** `src/features/notifications/types.rs`
- **Operations:** `NotificationOp` enum (ingress, dismiss, retract, tick)

---

## Critical Rules

### GTK Init Boundary (has caused crashes)

Plugins are initialized **before** GTK. Creating widgets in `init()` will crash with `GTK has not been initialized`.

**Allowed in `init()`:** DBus connections, async tasks, channels, pure Rust state
**NOT allowed in `init()`:** Any GTK widget construction

Construct widgets lazily in `create_elements()` or `get_widgets()`.

### Threading Model

- GTK widgets are **not** `Send`/`Sync` - live on main thread only
- Never mutate GTK from Tokio tasks
- Use channels or `glib::MainContext::invoke_local` for GTK updates from async code
- Split plugin state: Send-safe data for background tasks, GTK state for main thread
- Anything moved into `tokio::spawn(...)` must be `Send`

### Incremental UI Updates (must follow)

For DBus-driven UIs:
- Update only affected widgets, don't rebuild entire trees (causes flicker)
- Keep stable ordering (don't reorder rows on state changes)
- Use wake-on-demand invalidate queues for DBus signal bursts
- Gate updates to when overlay is visible

### Layer-Shell Window Dynamic Resizing

Layer-shell windows don't auto-resize when content changes. To trigger resize:
1. Call `window.set_default_size(width, -1)` when content changes (height `-1` = recalculate from content)
2. For animated content (revealers), trigger resize after animation completes via `revealer.connect_child_revealed_notify()`
3. Use `idle_add_local_once` to defer resize until after GTK event processing

To constrain max height, use `ScrolledWindow.set_max_content_height()` - CSS `max-height` on inner widgets won't constrain window size.

---

## DBus Notifications

### Policy

- Attempts to **replace** existing owner on startup; fails entirely if unable
- Capabilities: `actions`, `body`, `body-markup`
- Not supported: persistence (in-memory only), desktop-file icon resolution
- Clicking action emits `ActionInvoked`, then closes notification
- `replaces_id`: creates new notification and removes the old one

### App Icon Lookup (current limitation)

When no explicit notification icon is provided:
- Try notification app name as themed icon via `gtk::IconTheme::has_icon`
- Apply normalization (lowercase, whitespace → `-`, strip punctuation)
- Fall back to default icon

Non-goals: No `gio`/`GDesktopAppInfo` dependency for `.desktop` file resolution.

### Smoke Test

1. Verify ownership: `busctl --user status org.freedesktop.Notifications`
2. Basic: `notify-send "test" "Hello"`
3. Markup: `notify-send "Markup" "<b>bold</b> <i>italic</i>"`
4. Action: `notify-send --action=default=Open "Action test" "Click action"`
5. Monitor signals: `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`

---

## Detailed Concepts

### FeatureSpec and FeatureToggle

**FeatureSpec** - Declarative "React component" for feature tiles:
- Static metadata: key, title, icon
- UI state: `active`, `open`, `status_text`, optional `MenuSpec`
- Callbacks for user actions (e.g. `on_toggle`)
- UI-oriented and cloneable; should not own arbitrary plugin state

**FeatureToggle** - Lightweight wrapper:
- `id: String` - stable identifier
- `weight: i32` - sorting weight (heavier goes lower)
- `el: FeatureSpec` - the declarative spec (owned, no lifetimes)

### UI Event Bus (partial implementation)

`UiEvent` enum captures what needs to change in UI:
- `FeatureActiveChanged { key, active }`
- `FeatureStatusTextChanged { key, text }`
- `FeatureMenuOpenChanged { key, open }`

Flow:
1. App creates `UiEvent` channel, hands senders to plugins
2. Plugins emit events on domain changes
3. Central task applies changes to UI models

Benefits: plugins never hold model references, clean unloading.

### Wake-on-Demand Repaint Queue

For DBus-driven UIs without polling:

1. **Background task (Send-only):**
   - Decode DBus signals, update Send-safe model
   - Enqueue invalidate token to `Arc<Mutex<VecDeque<InvalidateKey>>>`
   - Never call GTK APIs

2. **GTK thread drain:**
   - Use `AtomicBool scheduled` flag
   - Schedule one GTK callback to drain queue, dedupe keys, apply incremental updates
   - Reset `scheduled` after drain

Rules:
- Never schedule repaints from `feature_toggles()`/`widgets()` loops
- Never capture GTK widgets into background tasks
- Keep drain callbacks small (update properties, don't rebuild trees)

### Overlay Visibility Gating

DBus signals continue while overlay is hidden. To stay responsive:
- Only apply updates while overlay is visible
- On hidden → visible: force drain+repaint (no stale state)
- On visible → hidden: disable scheduling

Plugin hooks: `on_overlay_shown()`, `on_overlay_hidden()`

---

## Migration Status

Currently on `relm4` branch, migrating from legacy GTK to Relm4. See `relm4-migration/` for step-by-step tracker. Each step must leave the app buildable with tests passing.

---

## Design Principles

1. **Stable plugin boundaries** - Plugins expose UI via `widgets()` and/or `feature_toggles()`
2. **Plugin state is plugin-local** - Each plugin owns domain state; UI composes what plugins provide
3. **Explicit state flow** - Avoid hidden couplings; expose explicit APIs or events
4. **Event bus is optional** - Use for declarative tiles; imperative API acceptable for widgets
5. **Main-thread UI, no forced `Send + Sync`** - Use async for blocking work; keep GTK types on main thread
