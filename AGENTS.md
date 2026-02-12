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

# Run with daemon plugins from build output
WAFT_DAEMON_DIR=./target/debug cargo run

# Run a single daemon standalone (for development/debugging)
cargo run -p waft-plugin-clock --bin waft-clock-daemon
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
- NetworkManager library migration (custom D-Bus -> nmrs crate)
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

All plugins run as **daemon binaries** communicating with the overview app via Unix socket IPC.

### Technology Stack

- **UI:** GTK4, libadwaita, gtk4-layer-shell (Wayland layer-shell protocol)
- **Async:** Tokio (multi-threaded runtime), flume (executor-agnostic channels)
- **System:** zbus 5.0 (DBus), nmrs 2.0 (NetworkManager bindings)
- **Plugin SDK:** waft-plugin-sdk (daemon binaries with IPC)
- **Widget rendering:** waft-ui-gtk (declarative Widget -> GTK reconciler)
- **Config:** TOML (`~/.config/waft/config.toml`)
- **Localization:** waft-i18n (Fluent internationalization)

### Core Components

- **`waft-core`** - Shared infrastructure: store pattern (`PluginStore`, `StoreOp`, `StoreState`, `set_field!`), menu state (`MenuStore`, `MenuOp`), `DbusHandle` (zbus wrapper), `Callback<T>`, `VoidCallback`. Re-exports `waft-config` and `waft-ipc`.
- **`waft-config`** - Configuration loading from `~/.config/waft/config.toml`
- **`waft-ipc`** - IPC protocol types: `OverviewMessage`, `PluginMessage`, `Widget`, `Action`, `ActionParams`, `NamedWidget`, `Node`, `Orientation`, `WidgetSet`. Also CLI command parsing (`IpcCommand`) and socket path helpers.
- **`waft-i18n`** - Fluent localization: `system_locale()` returns BCP47 locale, `I18n` struct for translations with `t()` and `t_args()`.
- **`waft-plugin-sdk`** - Daemon plugin SDK: `PluginDaemon` trait (Send+Sync), `PluginServer`, `WidgetNotifier`, widget builders (`FeatureToggleBuilder`, `SliderBuilder`, `MenuRowBuilder`, `ContainerBuilder`, `ButtonBuilder`, `LabelBuilder`, `InfoCardBuilder`, `SwitchBuilder`), testing utilities.
- **`waft-ui-gtk`** - GTK4 renderer: `Reconcilable` trait, `WidgetReconciler`, widget implementations (`FeatureToggleWidget`, `SliderWidget`, `IconWidget`, `MenuChevronWidget`, `MenuItemWidget`), `renderer` module.
- **`waft-overview`** - Main GTK4 overlay application binary. Spawns daemon plugins via `DaemonSpawner`, manages IPC connections via `PluginManager`, reconciles daemon widgets via `DaemonWidgetReconciler`, manages the layer-shell window.

### Plugin Architecture

The application has 14 plugins plus 1 internal feature:

| Plugin | Architecture | Purpose |
|--------|-------------|---------|
| **clock** | Daemon | Current time and date with locale support |
| **darkman** | Daemon | Dark mode toggle via darkman D-Bus |
| **caffeine** | Daemon | Prevent sleep/screensaver (Portal/ScreenSaver) |
| **battery** | Daemon | Battery percentage, health, charging (UPower D-Bus) |
| **brightness** | Daemon | Display brightness (brightnessctl/ddcutil) |
| **keyboard-layout** | Daemon | Input method display/switch (Niri/Sway/Hyprland/localed) |
| **systemd-actions** | Daemon | Shutdown, lock, logout via systemd login1 |
| **blueman** | Daemon | Bluetooth device management (BlueZ D-Bus) |
| **audio** | Daemon | Volume sliders, device selection (pactl) |
| **networkmanager** | Daemon | WiFi/Ethernet/VPN management (nmrs + zbus) |
| **weather** | Daemon | Weather information via HTTP API |
| **notifications** | Daemon | D-Bus notification server, toasts, DND |
| **eds** | Daemon (entity-based) | EDS calendar integration |
| **sunsetr** | Daemon | Night light control via sunsetr CLI |
| *session* | Internal | Session lock detection (in overview/src/features/) |

### Daemon Architecture

The primary plugin pattern. Daemon plugins are standalone tokio binaries that communicate with waft-overview via Unix socket IPC.

**Components:**
- **`DaemonSpawner`** (`crates/overview/src/daemon_spawner.rs`) - Spawns all daemon binaries at startup. Discovers binaries via `WAFT_DAEMON_DIR` env var or standard paths.
- **`PluginManager`** (`crates/overview/src/plugin_manager/`) - IPC client that connects to daemon sockets, sends `GetWidgets`/`TriggerAction` messages, receives `SetWidgets` pushes. Submodules: `client.rs`, `router.rs`, `discovery.rs`, `registry.rs`.
- **`DaemonWidgetReconciler`** (`crates/overview/src/daemon_widget_reconciler.rs`) - Converts declarative `Widget` descriptions from daemons into actual GTK widgets using `waft-ui-gtk`.
- **`PluginServer`** (`crates/plugin-sdk/src/server.rs`) - Daemon-side socket server. Handles connections, message routing, and push notifications via `WidgetNotifier`.

**IPC Protocol:**
- Transport: Unix sockets at `/run/user/{uid}/waft/plugins/{name}.sock`
- Framing: 4-byte big-endian length prefix + JSON payload
- Messages: `OverviewMessage` (GetWidgets, TriggerAction) -> `PluginMessage` (SetWidgets)
- Push updates: `WidgetNotifier::notify()` triggers `SetWidgets` to all connected clients

**Daemon Pattern:**
```rust
#[async_trait::async_trait]
impl PluginDaemon for MyDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget>;
    async fn handle_action(&mut self, widget_id: String, action: Action) -> Result<...>;
}
let (server, notifier) = PluginServer::new("name", daemon);
server.run().await?;
```

For creating new daemon plugins, use the `create-daemon-plugin` skill. For debugging IPC issues, use the `debug-daemon-ipc` skill.

### Directory Structure

```
Cargo.toml                        # Workspace root
crates/
    config/                       # waft-config: TOML config loading
    core/                         # waft-core: store, menu_state, DbusHandle, Callback
    i18n/                         # waft-i18n: Fluent localization, system_locale()
    ipc/                          # waft-ipc: Widget protocol, IPC message types, socket path
    overview/                     # waft-overview: main GTK4 overlay binary
        src/
            main.rs               # Tokio entrypoint
            app.rs                # Plugin loading, IPC, window
            daemon_spawner.rs     # Spawns daemon binaries
            daemon_widget_reconciler.rs  # Widget desc -> GTK widgets
            plugin_manager/       # IPC client (manager, client, router, discovery, registry)
            plugin.rs             # Plugin type definitions
            plugin_registry.rs    # Plugin lifecycle
            ui/                   # UI components (main_window, feature_grid, feature_toggle, icon)
            features/
                session/          # Session lock detection (internal, not a user plugin)
    plugin-sdk/                   # waft-plugin-sdk: daemon SDK
        src/
            lib.rs                # Re-exports PluginDaemon, PluginServer, builders, IPC types
            daemon.rs             # PluginDaemon trait (Send + Sync)
            server.rs             # PluginServer, WidgetNotifier, socket handling
            builder.rs            # Widget builders (FeatureToggle, Slider, MenuRow, etc.)
            testing.rs            # MockPluginDaemon, TestPlugin, test socket helpers
    waft-ui-gtk/                  # GTK4 renderer library
        src/
            lib.rs                # reconcile, renderer, widget_reconciler, widgets
            widgets/              # FeatureToggleWidget, SliderWidget, IconWidget, etc.
plugins/
    clock/          bin/          # Daemon: time/date display
    darkman/        bin/          # Daemon: dark mode toggle (D-Bus)
    caffeine/       bin/          # Daemon: prevent sleep (Portal/ScreenSaver)
    battery/        bin/          # Daemon: battery status (UPower D-Bus)
    brightness/     bin/          # Daemon: display brightness (brightnessctl/ddcutil)
    keyboard-layout/ bin/         # Daemon: input method (multi-backend)
    systemd-actions/ bin/         # Daemon: session + power menus (login1 D-Bus)
    blueman/        bin/          # Daemon: Bluetooth management (BlueZ D-Bus)
    audio/          bin/          # Daemon: volume + device selection (pactl)
    networkmanager/ bin/          # Daemon: WiFi/Ethernet/VPN (nmrs + zbus)
    weather/        bin/          # Daemon: weather info (HTTP API)
    notifications/  bin/          # Daemon: notification server + toasts
    eds/            bin/          # Daemon: EDS calendar integration
    sunsetr/        bin/          # Daemon: night light control
```

### Key Architectural Patterns

**Async-First Architecture with Clear Threading Boundaries:**
- **Overview (main thread):** GTK widgets and UI rendering (not `Send`/`Sync`)
- **Daemon plugins:** Pure tokio, all `Send + Sync`, no GTK dependency
- **Tokio Runtime:** All async I/O, D-Bus, file operations, background tasks
- **Channel-based Communication:** flume (executor-agnostic) for tokio <-> glib in overview

**Session Lock Awareness:**
- `on_session_lock()` hook pauses animations and expensive operations when locked
- Reduces power consumption and visual artifacts during lock screen

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
    pub input: bool,       // "input" answers "Is input?" -> true/false
    pub default: bool,     // "default" answers "Is default?" -> true/false
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

Never use `gtk::Image::builder().icon_name(...)` to create icons. Use `waft_ui_gtk::widgets::IconWidget` instead -- it provides theme resolution, fallback handling, and consistent API.

- `IconWidget::from_name("icon-name", pixel_size)` for simple named icons
- `IconWidget::new(icon_hints, pixel_size)` for multi-source icons (themed/file/bytes)

### UI Component Architecture

**React-ish component pattern: dumb widgets + smart containers**

Structure UI as presentational (dumb) widgets orchestrated by smart containers:

- **Dumb widgets** receive data via `Props` structs and constructor args. They emit events via `Output` enums and `connect_output()` callbacks. They never hold store references or subscribe to stores (exception: self-contained popover tracking via `MenuStore`).
- **Smart containers** own store subscriptions, manage state, create child widgets, connect callbacks, and push state changes down via setter methods (e.g. `set_expanded(bool)`).
- **Data flows down** (Props/setters), **events flow up** (Output callbacks) -- unidirectional.

Naming conventions: `*Props` for input structs, `*Output` for event enums, `connect_output()` for callback registration, `pub root` for the GTK root widget, `widget()` accessor.

When a widget has no events (purely presentational), skip the `Output` enum and `connect_output`.

### Threading Model

**Overview (GTK host):**
- GTK widgets are **not** `Send`/`Sync` -- live on main thread only
- Never mutate GTK from Tokio tasks
- Use channels or `glib::MainContext::invoke_local` for GTK updates from async code
- Anything moved into `tokio::spawn(...)` must be `Send`

**Daemon plugins:**
- All `Send + Sync` (enforced by `PluginDaemon` trait)
- Pure tokio context -- no GTK, no glib
- Shared state: `Arc<StdMutex<T>>` between daemon struct and monitoring tasks
- D-Bus signal monitoring: `tokio::spawn` + `zbus::MessageStream` + `notifier.notify()`

### Runtime Mixing: Never Run Tokio Futures in glib Context

Never run tokio-dependent futures inside `glib::spawn_future_local()` -- causes 100% CPU busy-polling. Always spawn tokio work on the tokio runtime and communicate via executor-agnostic channels (flume).

**zbus configuration:** Always use `zbus = { version = "5", default-features = false, features = ["tokio"] }`. The default `async-io` backend causes the same busy-poll issue.

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

To constrain max height, use `ScrolledWindow.set_max_content_height()` -- CSS `max-height` on inner widgets won't constrain window size.

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
- Apply normalization (lowercase, whitespace -> `-`, strip punctuation)
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

### UI Components

- **Feature Toggle** - A toggleable tile widget with icon, title, status text, and optional expandable menu. Implemented in `waft-ui-gtk::widgets::FeatureToggleWidget`. Used by plugins to present controls.
- **Toast** - A temporary popup notification that appears on screen and auto-dismisses after a timeout.
- **Notification Card** - A persistent notification item displayed in the notification center panel. Unlike toasts, cards remain until explicitly dismissed.
- **Revealer** - GTK4 widget (`gtk::Revealer`) used for smooth show/hide animations with slide transitions.
- **Menu Chevron** - Arrow icon widget (`waft-ui-gtk::widgets::MenuChevronWidget`) that rotates to indicate expandable menu state.
- **Slider Control** - Volume or brightness widget (`waft-ui-gtk::widgets::SliderWidget`) combining a slider with an expandable menu.

### Architecture Terms

- **Daemon Plugin** - A standalone tokio binary implementing `PluginDaemon` (Send+Sync) from `waft-plugin-sdk`. Communicates with overview via Unix socket IPC.
- **Widget Protocol** - The `Widget` enum (in `waft-ipc`) that daemon plugins use to describe their UI declaratively. Variants: `FeatureToggle`, `Slider`, `MenuRow`, `Container`, `Button`, `Label`, `InfoCard`, `Switch`, `Spinner`, `Checkmark`.
- **NamedWidget** - A `Widget` with an `id` (string) and `weight` (i32 for sort order). The unit of plugin-to-overview communication.
- **WidgetReconciler** - Cache-based system in `waft-ui-gtk` that efficiently updates GTK widgets when `Widget` descriptions change, avoiding full rebuilds.
- **PluginManager** - Overview component that manages IPC connections to all daemon plugins (`crates/overview/src/plugin_manager/`).
- **DaemonSpawner** - Overview component that spawns all daemon binaries at startup (`crates/overview/src/daemon_spawner.rs`).
- **WidgetNotifier** - Daemon-side mechanism to push updated widgets to all connected overview clients when state changes.
- **Overlay** - The main layer-shell window that appears on top of other applications.

### System Integration

- **Layer-shell** - Wayland protocol (`gtk4-layer-shell`) that positions windows in compositor layers.
- **Session Lock** - System lock screen state. Plugins pause expensive operations when locked.
- **DND (Do Not Disturb)** - Notification mode that suppresses toast popups while still collecting notifications.

### Patterns & Techniques

- **Idle Add** - Pattern using `glib::idle_add_local_once()` to defer operations until after current GTK event processing completes.
- **Hidden Flag** - Boolean flag (`Rc<RefCell<bool>>`) used in dismissable widgets to prevent gesture handlers from accessing destroyed widgets during animations.
- **Deferred Removal** - Pattern combining `idle_add_local_once()` with widget removal to ensure all event handlers complete before destruction.

---

## Coding Rules: Prevent Silent Hangs

This app is a long-running daemon. A silent failure in any async loop, channel consumer, or background task makes the overlay permanently unresponsive with no clue in the logs. Follow these rules to keep every failure path visible.

### Never discard Results with `let _ =`

Silent `let _ = expr` on fallible operations hides the exact moment something breaks. Always log or act on the error.

```rust
// BAD -- silent failure, invisible in logs
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
    warn!("[feature] receiver loop exited -- feature is now unresponsive");
});
```

### Log when background tasks exit

Wrap `tokio::spawn` calls so unexpected exits are visible.

```rust
// BAD -- task exits silently
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
// BAD -- loops forever sending into the void
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
// BAD -- panics the app
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
// BAD -- creates zombie
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
// BAD -- panic message may be swallowed
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

1. **Plugins describe UI via Widget protocol** -- Daemon plugins return `Vec<NamedWidget>` declarative descriptions; overview renders them via `waft-ui-gtk`
2. **NEVER do exceptional programming. ALWAYS select the systemic approach** -- Define general mechanisms first, then use for specific cases
3. **NO POLLING** -- Sleep to next event boundary (D-Bus signals, timer boundaries)
4. **Plugin state is plugin-local** -- Each plugin owns domain state; UI composes what plugins provide
5. **Explicit state flow** -- Avoid hidden couplings; expose explicit APIs or events
6. **GTK->tokio writes**: `std::sync::mpsc` + `std::thread` (bypasses tokio scheduler)

---

## Migration Status

### Phase 5: Daemon Architecture (Complete)

**All 14 plugins migrated to daemon architecture.**

- `waft-plugin-sdk` with `PluginDaemon` trait, `PluginServer`, `WidgetNotifier`, builders
- `waft-ui-gtk` renderer with `WidgetReconciler` and `Reconcilable` trait
- `waft-ipc` Widget protocol with all widget types
- `DaemonSpawner` and `PluginManager` in overview

**Next:**
- Arch Linux split packaging (PKGBUILD)
- New apps (`waft-settings`, `waft-palette`, etc.)

**Main Branch:** `relm4` (integration target)
**Active Branch:** `larger-larger-picture`

---

## Future Work & Known Limitations

### Planned Features

- **SNI (Status Notifier Items) support** -- Systray compatibility
- **Settings app (`waft-settings`)** -- Standalone preferences/control center
- **Arch Linux split packaging** -- Independent packages per plugin

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

See individual plugin README.md files in `plugins/<plugin>/` for plugin-specific configuration options.

**Documentation requirement:** When adding or modifying plugin configuration options, always update the plugin's README.md file.
