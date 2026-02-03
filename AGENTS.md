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

**sacrebleui** is a Wayland-only overlay UI application using Rust, GTK4, libadwaita, and Relm4. It acts as a notification server (owns `org.freedesktop.Notifications` on DBus) and provides an extensible overlay panel with feature toggles and a plugin-based architecture.

### Technology Stack

- **Framework:** Relm4 (declarative GTK4 component architecture)
- **UI:** libadwaita (modern GTK4 library), gtk4-layer-shell (Wayland layer-shell protocol)
- **Async:** Tokio (multi-threaded runtime), flume (executor-agnostic channels)
- **System:** zbus 5.0 (DBus), nmrs 2.0 (NetworkManager bindings)
- **Config:** TOML (`~/.config/sacrebleui/config.toml`)
- **Localization:** Fluent (internationalization support)

### Core Components

- **Entry point:** `src/main.rs` → Tokio entrypoint → `app::run()`
- **App model:** `src/app.rs` - Relm4 `SimpleComponent`, manages overlay window (layer-shell), plugin registry, IPC server, async orchestration
- **Plugin system:** `src/plugin.rs`, `src/plugin_registry.rs` - `#[async_trait(?Send)]` plugin trait for GTK compatibility
- **Notifications:** `src/features/notifications/` - DBus server implementation, reducer-based state management, toast UI
- **IPC:** `src/ipc/` - JSON command server over Unix socket (show/hide/toggle/ping)
- **UI Components:** `src/ui/` - Layer-shell window, feature grid, toggles, sliders, menus, icons, clock, battery, weather
- **Features (plugins):** `src/features/` - 11 pluggable features including audio, brightness, WiFi, Bluetooth, VPN, battery, clock, etc.

### Plugin Architecture

The application is **plugin-centric** with 11 plugins:

| Plugin | Type | Purpose |
|--------|------|---------|
| **notifications** | Core | DBus notification server, toast display, Do Not Disturb toggle |
| **audio** | Control | Volume slider, audio device selection |
| **brightness** | Control | Master brightness slider + per-display fine-tuning |
| **networkmanager** | Control | WiFi toggle, Ethernet adapter info, VPN connection toggle |
| **bluetooth** | Control | Device discovery, connection management, menu |
| **battery** | Info | Battery percentage, health, charging status |
| **clock** | Info | Current time and date with timezone support |
| **darkman** | Toggle | Dark mode control via darkman DBus service |
| **sunsetr** | Toggle | Night light control via sunsetr CLI |
| **agenda** | Info | Calendar/event display |
| **weather** | Info | Weather information via HTTP API |

**Plugin Lifecycle:**
- `configure()` - Parse plugin-specific TOML config
- `init()` - Async initialization (DBus, channels, state setup)
- `create_elements()` - Construct GTK widgets after GTK init
- `cleanup()` - Graceful shutdown

**Widget Slots:** `Info`, `Controls`, `Header`

### Plugin System Implementation

Plugins implement the `Plugin` trait (`#[async_trait(?Send)]`):
- `init()` - async initialization (DBus, channels, pure Rust state only)
- `create_elements()` / `get_widgets()` - GTK widget construction (after GTK init)
- Widgets placed into slots: `Info`, `Controls`, `Header`
- Registration is manual in `app.rs` via `PluginRegistry`
- Registry stores plugins behind `Arc<Mutex<Box<dyn Plugin>>>`
- Dynamic widget registration via `WidgetRegistrar` trait

**Documentation requirement:** When adding or modifying plugin configuration options, always update the plugin's README.md file (`src/features/<plugin>/README.md`) to document the new/changed options.

### Directory Structure

```
src/
├── main.rs                 # Tokio entrypoint
├── app.rs                  # Main GTK application, plugin orchestration
├── lib.rs                  # Public library API for tests
├── plugin.rs               # Plugin trait (#[async_trait(?Send)])
├── plugin_registry.rs      # Plugin registration and discovery
├── config.rs               # TOML configuration loading
├── dbus.rs                 # Generic DBus handle for plugins
├── menu_state.rs           # Menu expansion/collapse state
├── store.rs                # Global state store
├── ipc/                    # Unix socket IPC command server
├── i18n/                   # Fluent localization (Internationalization)
├── runtime.rs              # Async runtime helpers
├── ui/                     # Core UI components
│   ├── main_window.rs      # Layer-shell overlay window
│   ├── feature_grid.rs     # Grid layout for feature toggles
│   ├── feature_toggle.rs   # Generic toggle widget
│   ├── feature_toggle_expandable.rs  # Toggle with expandable menu
│   ├── slider_control.rs   # Slider with menu extraction
│   ├── menu_item.rs        # Reusable menu item component
│   ├── menu_chevron.rs     # Chevron icon for menus
│   ├── icon.rs             # Theme icon resolution
│   ├── battery.rs          # Battery status display
│   ├── clock.rs            # Clock display
│   ├── weather.rs          # Weather display
│   └── style.rs            # CSS styling
└── features/               # Plugin implementations (11 total)
    ├── agenda/             # Calendar/agenda plugin
    ├── audio/              # Audio control (volume, device selection)
    ├── battery/            # Battery status and health
    ├── bluetooth/          # Bluetooth device management
    ├── brightness/         # Display brightness control (NEW)
    ├── clock/              # Time and date display
    ├── darkman/            # Dark mode toggle
    ├── networkmanager/     # WiFi, Ethernet, VPN management
    ├── notifications/      # Desktop notification handling (core)
    ├── session/            # Session lock detection
    ├── sunsetr/            # Night light control
    └── weather/            # Weather information
```

### Recent Development (Last 25 Commits)

**Feature Development:**
- **a8faa48** - Display brightness control (brightnessctl/ddcutil backends, master + per-display)
- **b968f72** - VPN connection toggle with menu widget
- **877bb9c** - Wired network adapter info display fix
- **e973f36** - Dynamic widget registration at runtime
- **5fdc658** - Unified menu item design across all plugins
- **413ebc4** - Major refactor: Migrated from custom D-Bus to nmrs crate (450 LOC reduction)
- **a06c14d** - WiFi and Ethernet adapter management
- **f44004a** - i18n support with Fluent localization

**Infrastructure & Testing:**
- **1902c07** - Expanded DBus test coverage
- **5d8e13f** - Icon resolution test suite
- **d89dfa9** - Initialized OpenSpec for specification-driven development

**Current Branch:** `network` (feature development)
**Main Branch:** `relm4` (integration target)
**Branch Status:** Clean (all work committed)

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

- **Notifications store:** `AsyncReducible` reducer pattern in `src/features/notifications/store.rs`
- **Domain types:** `src/features/notifications/types.rs`
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

## Project-Specific Terminology

This section defines domain-specific terms used throughout the codebase. Understanding these terms helps AI agents interpret code correctly and maintain consistent naming.

### UI Components

- **Feature Toggle** - A UI component widget (in `src/ui/feature_toggle.rs`) that displays a toggleable tile with icon, title, status text, and optional expandable menu. Used by plugins to present controls (WiFi, Bluetooth, DND, etc.).
- **Toast** - A temporary popup notification that appears on screen and auto-dismisses after a timeout. Part of the notifications plugin (`src/features/notifications/ui/toast_widget.rs`).
- **Notification Card** - A persistent notification item displayed in the notification center panel. Unlike toasts, cards remain until explicitly dismissed (`src/features/notifications/ui/notification_card.rs`).
- **Revealer** - GTK4 widget (`gtk::Revealer`) used extensively for smooth show/hide animations with slide transitions. Controls visibility with `set_reveal_child()`.
- **Menu Chevron** - Small arrow icon widget (`src/ui/menu_chevron.rs`) that rotates to indicate expandable menu state (open/closed).
- **Slider Control** - Volume or brightness control widget (`src/ui/slider_control.rs`) combining a slider with an expandable menu.

### Architecture Terms

- **Plugin** - A self-contained feature module implementing the `Plugin` trait. Examples: notifications, audio, WiFi, brightness. Plugins provide widgets and handle domain logic.
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

Currently on `relm4` branch, migrating from legacy GTK to Relm4. See `relm4-migration/` for step-by-step tracker. Each step must leave the app buildable with tests passing.

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

From TODO.md:
- **WiFi plugin** - Separate WiFi-specific management (distinct from NetworkManager integration)
- **Caffeine plugin** - Prevent sleep/screensaver (systemd inhibitor)
- **Keyboard layout plugin** - Display and switch input methods
- **SNI (Status Notifier Items) support** - Systray compatibility
- **Action plugins** - Shutdown, lock, logout via systemd
- **Settings integration** - Preferences UI within overlay
- **Error handling strategy** - Unified "Failed to load widgets" recovery

### Known Limitations

- **Notifications persistence:** In-memory only; not persisted to disk
- **Desktop app icon resolution:** No `.desktop` file lookup (GDesktopAppInfo not used)
- **Wayland-only:** No X11 support
- **One overlay instance:** Single unified overlay per session

### Configuration

Default config location: `~/.config/sacrebleui/config.toml`

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

See individual plugin README.md files in `src/features/<plugin>/` for plugin-specific configuration options.
