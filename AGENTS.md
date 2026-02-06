# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) and other AI agents working with code in this repository.

## Agent Rule

Before implementing changes, ask for clarification on DBus ownership, threading boundaries, and API changes. Do not start coding until key behavioral decisions are confirmed.

## Build & Test Commands

```bash
cargo build --workspace        # Build all crates and plugins
cargo test --workspace         # Run all tests across workspace
cargo test -p waft-core        # Run tests for a specific crate
cargo test notifications_store # Run specific test module

# Run with dynamic plugins from build output
WAFT_PLUGIN_DIR=./target/debug cargo run

# Verify .so symbols
nm -D target/debug/libwaft_plugin_clock.so | grep waft
```

---

## OpenSpec & Project Specifications

**This project uses OpenSpec for specification-driven development.** All changes are documented with structured specifications in the `specs/` directory. Each change includes:

- **proposal.md** - Rationale: why this change is needed
- **design.md** - Implementation details and technical decisions
- **tasks.md** - Work breakdown and step-by-step tasks
- **specs/** - Detailed capability specifications

**Active OpenSpec Changes (10+ archived specs):**
- Display brightness plugin control with brightnessctl/ddcutil backends
- VPN network support and toggle
- WiFi and Ethernet network management
- Menu UI consistency across Bluetooth, WiFi, VPN
- NetworkManager library migration (custom D-Bus → nmrs crate)
- Dynamic plugin widget registration at runtime
- Session lock awareness (pause animations when locked)
- Icon resolution and theme support
- DBus signal testing and error handling
- Notification store reducer pattern

When implementing features:
1. Check `specs/` for existing specifications
2. Review related `proposal.md` and `design.md` for context
3. Follow tasks outlined in `tasks.md`
4. Keep specifications up-to-date with implementation

---

## Architecture Overview

**Waft** (formerly sacrebleui) is a Wayland-only overlay UI application using Rust, GTK4, and libadwaita. It acts as a notification server (owns `org.freedesktop.Notifications` on DBus) and provides an extensible overlay panel with feature toggles and a plugin-based architecture.

The project is structured as a **Cargo workspace** with shared crates and independently packageable dynamic plugins (`.so` files loaded at runtime via `libloading`).

### Technology Stack

- **UI:** GTK4, libadwaita (modern GTK4 library), gtk4-layer-shell (Wayland layer-shell protocol)
- **Async:** Tokio (multi-threaded runtime), flume (executor-agnostic channels)
- **System:** zbus 5.0 (DBus), nmrs 2.0 (NetworkManager bindings)
- **Plugin loading:** libloading (dynamic `.so` loading)
- **Config:** TOML (`~/.config/waft/config.toml`)
- **Localization:** Fluent (internationalization support)

### Core Components

- **`waft-core`** — Shared infrastructure: store pattern (`PluginStore`, `StoreOp`, `StoreState`, `set_field!` macro), menu state coordination (`MenuStore`, `MenuOp`), re-exports of `waft-config` and `waft-ipc`
- **`waft-plugin-api`** — Plugin trait definitions and types: `OverviewPlugin` trait, `PluginId`, `PluginMetadata`, widget types (`Widget`, `Slot`, `WidgetRegistrar`, `WidgetFeatureToggle`), plugin loader (`loader.rs`), export macros (`export_plugin_metadata!`, `export_overview_plugin!`)
- **`waft-config`** — Configuration loading from `~/.config/waft/config.toml`
- **`waft-ipc`** — IPC protocol over Unix socket (show/hide/toggle/ping/stop)
- **`waft-overview`** — Main GTK4 overlay application binary. Loads dynamic plugins from `.so` files, manages the layer-shell window, plugin registry, IPC server
- **`plugins/clock`** — First dynamic plugin (cdylib). Independently compiled `.so` loaded at runtime

### Plugin Architecture

The application is **plugin-centric** with 14 plugins. Plugins can be either:
- **Dynamic plugins** — `.so` files loaded at runtime from `/usr/lib/waft/plugins/` (or `WAFT_PLUGIN_DIR`)
- **Static plugins** — compiled directly into `waft-overview` (being migrated to dynamic)

| Plugin | Type | Loading | Purpose |
|--------|------|---------|---------|
| **clock** | Info | **Dynamic (.so)** | Current time and date with timezone support |
| **notifications** | Core | Static | DBus notification server, toast display, Do Not Disturb toggle |
| **audio** | Control | Static | Volume slider, audio device selection |
| **brightness** | Control | Static | Master brightness slider + per-display fine-tuning |
| **networkmanager** | Control | Static | WiFi toggle, Ethernet adapter info, VPN connection toggle |
| **bluetooth** | Control | Static | Device discovery, connection management, menu |
| **battery** | Info | Static | Battery percentage, health, charging status |
| **darkman** | Toggle | Static | Dark mode control via darkman DBus service |
| **sunsetr** | Toggle | Static | Night light control via sunsetr CLI |
| **caffeine** | Toggle | Static | Prevent sleep/screensaver |
| **agenda** | Info | Static | Calendar/event display |
| **weather** | Info | Static | Weather information via HTTP API |
| **keyboard-layout** | Info | Static | Display and switch input methods |
| **systemd-actions** | Actions | Static | Shutdown, lock, logout via systemd |

**Plugin Lifecycle:**
- `configure()` - Parse plugin-specific TOML config
- `init()` - Async initialization (DBus, channels, state setup)
- `create_elements()` - Construct GTK widgets after GTK init
- `cleanup()` - Graceful shutdown

**Widget Slots:** `Info`, `Controls`, `Header`, `Actions`

### Plugin System Implementation

Plugins implement the `OverviewPlugin` trait (`#[async_trait(?Send)]`) from `waft-plugin-api`:
- `id()` - returns `PluginId` for config matching
- `configure()` - parse plugin-specific TOML settings
- `init()` - async initialization (DBus, channels, pure Rust state only)
- `create_elements()` - GTK widget construction (after GTK init)
- `cleanup()` - graceful shutdown
- Lifecycle hooks: `on_overlay_visible()`, `on_session_lock()`, `on_session_unlock()`

**Plugin Loading Order (in `app.rs`):**
1. Dynamic plugins loaded first via `waft_plugin_api::loader::discover_plugins()`
2. Static plugins loaded next (will be migrated to dynamic over time)
3. All plugins registered into `PluginRegistry` (via `register()` or `register_boxed()`)

**Dynamic Plugin Entry Points (exported from `.so`):**
- `waft_plugin_metadata() -> PluginMetadata` — plugin identity and version
- `waft_create_overview_plugin() -> *mut dyn OverviewPlugin` — factory function

**Export Macros (from `waft-plugin-api`):**
```rust
waft_plugin_api::export_plugin_metadata!("plugin::clock", "Clock", "0.1.0");
waft_plugin_api::export_overview_plugin!(ClockPlugin::new());
```

**Documentation requirement:** When adding or modifying plugin configuration options, always update the plugin's README.md file.

### Directory Structure

```
Cargo.toml                     # Workspace root
crates/
├── config/                    # waft-config: TOML config loading
│   └── src/lib.rs
├── ipc/                       # waft-ipc: Unix socket IPC protocol
│   └── src/
│       ├── lib.rs             # Command parsing, socket path
│       └── net.rs             # Async client/server
├── waft-core/                 # Shared infrastructure
│   └── src/
│       ├── lib.rs             # Re-exports config, ipc
│       ├── store.rs           # PluginStore, StoreOp, StoreState, set_field!
│       ├── menu_state.rs      # MenuStore, MenuOp, create_menu_store()
│       └── menu_state_tests.rs
├── waft-plugin-api/           # Plugin API for all apps
│   └── src/
│       ├── lib.rs             # PluginId, PluginMetadata, export macros
│       ├── overview.rs        # OverviewPlugin trait, Widget, Slot, WidgetRegistrar
│       └── loader.rs          # Dynamic .so discovery and loading
└── overview/                  # waft-overview: main GTK4 overlay binary
    └── src/
        ├── main.rs            # Tokio entrypoint
        ├── app.rs             # Plugin loading (dynamic + static), IPC, window
        ├── plugin.rs          # Re-exports from waft-plugin-api
        ├── plugin_registry.rs # Plugin lifecycle management
        ├── store.rs           # Re-exports from waft-core
        ├── menu_state.rs      # Re-exports from waft-core
        ├── dbus.rs            # DBus handle (zbus)
        ├── common.rs          # Callback<T>, ConnectionState
        ├── runtime.rs         # Async runtime helpers
        ├── i18n/              # Fluent localization
        ├── ui/                # UI components (main_window, feature_grid, etc.)
        └── features/          # Static plugins (13, being migrated to dynamic)
            ├── agenda/
            ├── audio/
            ├── battery/
            ├── bluetooth/
            ├── brightness/
            ├── caffeine/
            ├── darkman/
            ├── keyboard_layout/
            ├── networkmanager/
            ├── notifications/
            ├── session/        # Session lock detection (not a user plugin)
            ├── sunsetr/
            ├── systemd_actions/
            └── weather/
plugins/
└── clock/                     # Dynamic plugin: libwaft_plugin_clock.so
    ├── Cargo.toml             # crate-type = ["cdylib"]
    └── src/lib.rs             # Plugin + widget, self-contained
```

### Recent Development

**Ecosystem Split (current work on `larger-picture` branch):**
- Extracted `waft-core` and `waft-plugin-api` crates from monolithic overview
- Clock plugin as first dynamic `.so` — validates the plugin loading architecture
- Dynamic plugin loader with `libloading`, `catch_unwind` safety, rustc version checking
- Export macros for plugin entry points (`export_plugin_metadata!`, `export_overview_plugin!`)

**Prior Feature Development (on `relm4` branch):**
- Display brightness control (brightnessctl/ddcutil backends)
- VPN connection toggle, WiFi/Ethernet management via nmrs
- Unified menu item design across all plugins
- i18n support with Fluent localization
- OpenSpec specification-driven development workflow

**Main Branch:** `relm4` (integration target)
**Active Branch:** `larger-picture` (ecosystem split)

### Key Architectural Patterns

**Async-First Architecture with Clear Threading Boundaries:**
- **Main Thread:** GTK widgets and UI rendering (not `Send`/`Sync`)
- **Tokio Runtime:** All async I/O, DBus, file operations, background tasks
- **Channel-based Communication:** flume (executor-agnostic) for tokio ↔ glib communication

**Important:** Never run tokio futures inside `glib::spawn_future_local()` - causes 100% CPU busy-polling. Always spawn tokio work on the tokio runtime and communicate via executor-agnostic channels.

**Widget Registration Pattern:**
- Plugins use `WidgetRegistrar` to dynamically register/unregister widgets at runtime
- Enables hot-reloading and graceful shutdown
- Prevents widget lifecycle coupling

**Non-`Send` Plugin Trait:**
- Uses `#[async_trait(?Send)]` for GTK compatibility
- Plugins live on main thread, never moved into tokio tasks directly
- Plugin state split: Send-safe for background tasks, GTK types for UI thread

**Layer-Shell Dynamic Resizing:**
- Manual `window.set_default_size(width, -1)` when content changes
- Deferred via `idle_add_local` to prevent recursion
- Animated content (revealers) triggers resize after animation completes

**Session Lock Awareness:**
- `on_session_lock()` hook pauses animations and expensive operations when locked
- Reduces power consumption and visual artifacts during lock screen

### State Management

- **Notifications store:** `AsyncReducible` reducer pattern in `crates/overview/src/features/notifications/store.rs`
- **Domain types:** `crates/overview/src/features/notifications/types.rs`
- **Operations:** `NotificationOp` enum (ingress, dismiss, retract, tick)
- **Plugin-local state:** Each plugin owns domain state; UI composes what plugins provide
- **Explicit state flow:** Avoid hidden couplings; expose explicit APIs or events

---

## Critical Rules

### Naming Conventions (MUST follow)

**FORBIDDEN: Generic "utils" naming**

Never use `utils`, `helpers`, `misc`, or similar vague module/file names. Every module must be named semantically based on what it contains or does.

```rust
// BAD - vague, meaningless
mod wifi_utils;
mod helpers;
mod misc;

// GOOD - semantic, descriptive
mod wifi_icon;          // Contains WiFi icon selection logic
mod signal_strength;    // Signal strength calculations
mod network_scanner;    // Network scanning functionality
```

This rule applies to:
- Module names (`mod foo`)
- File names (`foo.rs`)
- Directory names (`src/features/foo/`)

**Boolean field naming: State, not question**

Boolean fields should be named as states/properties, not as questions. Reserve the "is_/has_/can_" prefix for functions/methods that return booleans.

```rust
// BAD - sounds like a function/question
pub struct AudioDevice {
    pub is_input: bool,    // Reads like "is input?"
    pub is_default: bool,  // Reads like "is default?"
}

// GOOD - state/property naming
pub struct AudioDevice {
    pub input: bool,       // "input" answers "Is input?" → true/false
    pub default: bool,     // "default" answers "Is default?" → true/false
}

// Functions/methods can use "is_" prefix
impl AudioDevice {
    pub fn is_input(&self) -> bool {  // OK - function asking question
        self.input
    }
}
```

Rationale: Boolean fields are answers to questions, not questions themselves. The "is_/has_" prefix suggests a method that returns a boolean. Use simple, direct property names for boolean fields.

### Icon Usage Rule

**FORBIDDEN: Using `gtk::Image` directly for icons**

Never use `gtk::Image::builder().icon_name(...)` to create icons. Use `ui::icon::IconWidget` instead — it provides theme resolution, fallback handling, and consistent API.

- `IconWidget::from_name("icon-name", pixel_size)` for simple named icons
- `IconWidget::new(icon_hints, pixel_size)` for multi-source icons (themed/file/bytes)

### UI Component Architecture

**React-ish component pattern: dumb widgets + smart containers**

Structure UI as presentational (dumb) widgets orchestrated by smart containers:

- **Dumb widgets** receive data via `Props` structs and constructor args. They emit events via `Output` enums and `connect_output()` callbacks. They never hold store references or subscribe to stores (exception: self-contained popover tracking via `MenuStore`).
- **Smart containers** own store subscriptions, manage state, create child widgets, connect callbacks, and push state changes down via setter methods (e.g. `set_expanded(bool)`).
- **Data flows down** (Props/setters), **events flow up** (Output callbacks) — unidirectional.

Naming conventions: `*Props` for input structs, `*Output` for event enums, `connect_output()` for callback registration, `pub root` for the GTK root widget, `widget()` accessor.

When a widget has no events (purely presentational), skip the `Output` enum and `connect_output`.

### GTK Init Boundary (has caused crashes)

Plugins are initialized **before** GTK. Creating widgets in `init()` will crash with `GTK has not been initialized`.

**Allowed in `init()`:** DBus connections, async tasks, channels, pure Rust state
**NOT allowed in `init()`:** Any GTK widget construction

Construct widgets lazily in `create_elements()` or `get_widgets()`.

**CRITICAL for cdylib plugins:** Dynamic `.so` plugins MUST use `gtk4` feature `unsafe-assume-initialized`. Each `.so` gets its own copy of gtk4's `static INITIALIZED: AtomicBool` — the host app sets it to `true` via `gtk::init()` but the plugin's copy stays `false`, causing "GTK has not been initialized" panics. The `unsafe-assume-initialized` feature skips this Rust-side check (zero-cost compile-time flag). This is safe because GTK is actually initialized by the host — it's only the Rust-side assertion that fails due to cdylib symbol isolation. Example in Cargo.toml:

```toml
gtk = { version = "0.10", package = "gtk4", features = ["v4_6", "unsafe-assume-initialized"] }
```

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

## Project-Specific Terminology

This section defines domain-specific terms used throughout the codebase. Understanding these terms helps AI agents interpret code correctly and maintain consistent naming.

### UI Components

- **Feature Toggle** - A UI component widget (in `crates/overview/src/ui/feature_toggle.rs`) that displays a toggleable tile with icon, title, status text, and optional expandable menu. Used by plugins to present controls (WiFi, Bluetooth, DND, etc.).
- **Toast** - A temporary popup notification that appears on screen and auto-dismisses after a timeout. Part of the notifications plugin (`crates/overview/src/features/notifications/ui/toast_widget.rs`).
- **Notification Card** - A persistent notification item displayed in the notification center panel. Unlike toasts, cards remain until explicitly dismissed (`crates/overview/src/features/notifications/ui/notification_card.rs`).
- **Revealer** - GTK4 widget (`gtk::Revealer`) used extensively for smooth show/hide animations with slide transitions. Controls visibility with `set_reveal_child()`.
- **Menu Chevron** - Small arrow icon widget (`crates/overview/src/ui/menu_chevron.rs`) that rotates to indicate expandable menu state (open/closed).
- **Slider Control** - Volume or brightness control widget (`crates/overview/src/ui/slider_control.rs`) combining a slider with an expandable menu.

### Architecture Terms

- **Plugin** - A self-contained feature module implementing the `OverviewPlugin` trait (re-exported as `Plugin` in overview). Can be static (compiled into binary) or dynamic (`.so` loaded at runtime). Examples: notifications, audio, WiFi, brightness, clock. Plugins provide widgets and handle domain logic.
- **Widget Registrar** - Dynamic registration pattern allowing plugins to add/remove widgets from the UI at runtime without rebuilding the entire tree.
- **Feature Spec** - Declarative data structure (`FeatureSpec`) describing a feature toggle's state (active, open, status text). Separates UI state from plugin logic.
- **Overlay** - The main layer-shell window that appears on top of other applications. Displays the feature grid and notification toasts.

### System Integration

- **Layer-shell** - Wayland protocol (`gtk4-layer-shell`) that positions windows in compositor layers (background, bottom, top, overlay). Enables persistent overlay UI.
- **Session Lock** - System lock screen state. Plugins pause expensive operations when locked (`on_session_lock()` hook) to save power.
- **DND (Do Not Disturb)** - Notification mode that suppresses toast popups while still collecting notifications in the panel.

### Patterns & Techniques

- **Idle Add** - Pattern using `glib::idle_add_local_once()` to defer operations until after current GTK event processing completes. Prevents race conditions and GTK assertions.
- **Hidden Flag** - Boolean flag (`Rc<RefCell<bool>>`) used in dismissable widgets to prevent gesture handlers from accessing destroyed widgets during animations.
- **Deferred Removal** - Pattern combining `idle_add_local_once()` with widget removal to ensure all event handlers complete before destruction.

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

### Ecosystem Split (Active)

Splitting monolithic `waft-overview` into independently packageable Arch Linux packages. See plan file for full details.

**Completed:**
- **Phase 1** — Extracted shared foundations: `waft-core` (store, menu_state) and `waft-plugin-api` (OverviewPlugin trait, PluginId, PluginMetadata, loader, export macros)
- **Phase 2** — Proof of concept: `clock` plugin extracted as first dynamic `.so` (cdylib), loaded at runtime via `libloading`

**Next:**
- **Phase 3** — Extract remaining 13 plugins to dynamic `.so` files (darkman → notifications, simplest first)
- **Phase 4** — `waft` CLI binary (IPC client)
- **Phase 5** — Arch Linux split packaging (PKGBUILD)
- **Phase 6+** — New apps (`waft-settings`, `waft-palette`, etc.)

**Main Branch:** `relm4` (integration target)
**Current Branch:** `larger-picture` (ecosystem split work)

---

## Runtime Mixing: Never Run Tokio Futures in glib Context

**Problem:** Running tokio-dependent futures (`tokio::process::Command`, `tokio::io::BufReader`, etc.) inside `glib::spawn_future_local()` or `glib::MainContext::default().spawn_local()` causes glib to busy-poll with zero-timeout `ppoll` calls, resulting in 100% CPU usage on a core.

**Root cause:** glib's event loop does not integrate with tokio's I/O driver. When a tokio future is polled from glib, glib sees "not ready" and immediately re-polls with no delay, spinning in a tight loop.

**zbus Configuration:** zbus must be configured with the `tokio` feature to integrate with tokio's runtime. By default, zbus uses `async-io`, which causes the same busy-poll issue when polled from `tokio::spawn`. Use `zbus = { version = "5", default-features = false, features = ["tokio"] }` in Cargo.toml.

**Solution:** Always spawn tokio work on the tokio runtime using `tokio::spawn()`. Use executor-agnostic channels (like `flume`) to communicate between runtimes.

```rust
// BAD — causes CPU busy-poll
glib::MainContext::default().spawn_local(async move {
    let mut child = tokio::process::Command::new("sunsetr")
        .spawn()?;
    let mut lines = tokio::io::BufReader::new(child.stdout.take()?).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // process line
    }
});

// GOOD — tokio work stays on tokio runtime
tokio::spawn(async move {
    let mut child = tokio::process::Command::new("sunsetr")
        .spawn()?;
    let mut lines = tokio::io::BufReader::new(child.stdout.take()?).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        sender.send(parsed_event)?; // flume is executor-agnostic
    }
    warn!("[feature] task exited");
});

// glib side receives via flume (executor-agnostic)
glib::spawn_future_local(async move {
    while let Ok(event) = rx.recv_async().await {
        // update GTK widgets
    }
});
```

**Detection:** Use `strace -p <pid> -e ppoll,read` to check for excessive ppoll calls (~2000+/sec) with zero timeout on eventfd descriptors.

---

## Coding Rules: Prevent Silent Hangs

This app is a long-running daemon. A silent failure in any async loop, channel consumer, or background task makes the overlay permanently unresponsive with no clue in the logs. Follow these rules to keep every failure path visible.

### Never discard Results with `let _ =`

Silent `let _ = expr` on fallible operations hides the exact moment something breaks. Always log or act on the error.

```rust
// BAD — silent failure, invisible in logs
let _ = tx.send_blocking(value);
let _ = rt.block_on(server());

// GOOD
if let Err(e) = tx.send_blocking(value) {
    eprintln!("[ipc] failed to forward command: {e}");
}
match rt.block_on(server()) {
    Ok(()) => eprintln!("[ipc] server exited cleanly"),
    Err(e) => eprintln!("[ipc] server error: {e}"),
}
```

Exception: `let _ =` is acceptable for best-effort cleanup where the outcome genuinely doesn't matter (e.g. removing a stale socket file).

### Log when async loops exit

Every `while let Ok(...) = rx.recv().await` loop is a critical event pump. When the channel closes, the loop exits silently and the feature stops responding. Always add a log line after the loop.

```rust
glib::spawn_future_local(async move {
    while let Ok(input) = rx.recv().await {
        handle(input);
    }
    warn!("[feature] receiver loop exited — feature is now unresponsive");
});
```

### Log when background tasks exit

Wrap `tokio::spawn` calls so unexpected exits are visible.

```rust
// BAD — task exits silently
tokio::spawn(my_task(rx));

// GOOD
tokio::spawn(async move {
    if let Err(e) = my_task(rx).await {
        warn!("[feature] task error: {e}");
    }
    debug!("[feature] task stopped");
});
```

### Break send loops when nobody is listening

When a broadcast/channel sender fails, it means all receivers are gone. Continuing to loop wastes resources. Break out and log.

```rust
// BAD — loops forever sending into the void
let _ = tx.send(msg);

// GOOD
if tx.send(msg).is_err() {
    break;
}
// after loop:
debug!("[feature] listener stopped");
```

### Recover from mutex poison, never panic

A poisoned mutex means a thread panicked while holding the lock. In a long-running app, recovering with `e.into_inner()` is better than crashing the entire process.

```rust
// BAD — panics the app
let guard = mutex.lock().unwrap();

// GOOD
let guard = match mutex.lock() {
    Ok(g) => g,
    Err(e) => {
        warn!("[feature] mutex poisoned, recovering: {e}");
        e.into_inner()
    }
};
```

### Reap child processes

Dropping a `std::process::Child` without calling `wait()` creates zombie processes. Spawn a thread to reap.

```rust
// BAD — creates zombie
Command::new("sh").arg("-c").arg(&cmd).spawn().ok();

// GOOD
match Command::new("sh").arg("-c").arg(&cmd).spawn() {
    Ok(child) => {
        std::thread::spawn(move || {
            let mut child = child;
            let _ = child.wait();
        });
    }
    Err(e) => error!("spawn failed: {e}"),
}
```

### Log before panic in bridge code

When a bridge between runtimes (e.g. tokio-to-glib) uses `expect()`, the panic message may never reach logs. Log the error explicitly first.

```rust
// BAD — panic message may be swallowed
rx.recv_async().await.expect("task panicked")

// GOOD
match rx.recv_async().await {
    Ok(val) => val,
    Err(e) => {
        error!("[runtime] task cancelled or panicked: {e}");
        panic!("task cancelled or panicked: {e}");
    }
}
```

### Guard against None in late-init fields

When a field is set to `Some(...)` during `create_elements()` and accessed later, use `match` instead of `.unwrap()` to avoid a panic if initialization order changes.

```rust
// BAD
let handle = self.field.as_ref().unwrap().clone();

// GOOD
let handle = match self.field.as_ref() {
    Some(h) => h.clone(),
    None => {
        error!("[feature] field not initialized");
        return Ok(());
    }
};
```

---

## Design Principles

1. **Stable plugin boundaries** - Plugins expose UI via `widgets()` and/or `feature_toggles()`
2. **Plugin state is plugin-local** - Each plugin owns domain state; UI composes what plugins provide
3. **Explicit state flow** - Avoid hidden couplings; expose explicit APIs or events
4. **Event bus is optional** - Use for declarative tiles; imperative API acceptable for widgets
5. **Main-thread UI, no forced `Send + Sync`** - Use async for blocking work; keep GTK types on main thread

---

## Future Work & Known Limitations

### Planned Features

- **Ecosystem split Phase 3-6** — Extract remaining 13 static plugins to dynamic `.so`, CLI binary, Arch packaging
- **SNI (Status Notifier Items) support** — Systray compatibility
- **Settings app (`waft-settings`)** — Standalone preferences/control center
- **Error handling strategy** — Unified "Failed to load widgets" recovery

### Known Limitations

- **Notifications persistence:** In-memory only; not persisted to disk
- **Desktop app icon resolution:** No `.desktop` file lookup (GDesktopAppInfo not used)
- **Wayland-only:** No X11 support
- **One overlay instance:** Single unified overlay per session

### Configuration

Default config location: `~/.config/waft/config.toml`

```toml
[[plugins]]
id = "plugin::notifications"
toast_limit = 3
disable_toasts = false

[[plugins]]
id = "plugin::brightness"
# Optional backend configuration

[[plugins]]
id = "plugin::networkmanager"
# VPN and network settings
```

See individual plugin README.md files in `crates/overview/src/features/<plugin>/` (static) or `plugins/<plugin>/` (dynamic) for plugin-specific configuration options.
