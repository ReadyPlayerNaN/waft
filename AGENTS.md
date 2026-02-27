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
cargo run --bin waft-clock-daemon

# CLI commands (waft daemon binary)
waft                           # Start daemon (default)
waft plugin ls                 # List discovered plugins
waft plugin ls --json          # List plugins as JSON
waft plugin describe <name>    # Show plugin details (entity types, properties, actions)
waft protocol                  # List all protocol entity types
waft protocol --domain audio   # Filter by domain
waft protocol --entity-type clock  # Show single entity type (verbose)
```

---

## OpenSpec & Project Specifications

**This project uses OpenSpec for specification-driven development.** All changes are documented with structured specifications in the `openspec/` directory. Each change includes:

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

1. Check `openspec/` for existing specifications
2. Review related `proposal.md` and `design.md` for context
3. Follow tasks outlined in `tasks.md`
4. Keep specifications up-to-date with implementation

---

## Architecture Overview

**Waft** (formerly sacrebleui) is a Wayland-only overlay UI application using Rust, GTK4, and libadwaita. A central daemon (`waft`) discovers, spawns, and supervises plugin daemons, routing entity data and actions between plugins and apps via Unix sockets. The overview app (`waft-overview`) subscribes to entity types and renders UI.

### Technology Stack

- **UI:** GTK4, libadwaita, gtk4-layer-shell (Wayland layer-shell protocol)
- **Async:** Tokio (multi-threaded runtime), flume (executor-agnostic channels)
- **System:** zbus 5.0 (DBus), nmrs 2.0 (NetworkManager bindings)
- **Plugin SDK:** waft-plugin (`Plugin` trait, `PluginRuntime`, `EntityNotifier`)
- **Widget rendering:** waft-ui-gtk (GTK widget implementations with `WidgetBase`, `Child`, `Children` types)
- **Config:** TOML (`~/.config/waft/config.toml`)
- **Localization:** waft-i18n (Fluent internationalization)

### Core Components

- **`waft-protocol`** - Entity types (domain-organized), messages (`AppMessage`, `PluginMessage`, `AppNotification`, `PluginCommand`), URN format and parsing, transport (length-prefixed JSON).
- **`waft-plugin`** - Plugin SDK: `Plugin` trait (Send+Sync), `PluginRuntime`, `EntityNotifier`, manifest handling (`handle_provides`), D-Bus monitoring helpers.
- **`waft`** - Central daemon: plugin discovery, on-demand spawning, entity routing, action tracking, crash recovery, D-Bus activation (`org.waft.Daemon`). Also provides CLI (`waft plugin ls`, `waft plugin describe`, `waft protocol`) and emits `plugin-status` entities as a meta-plugin.
- **`waft-overview`** - Main GTK4 overlay application binary. Connects to daemon via `WaftClient`, subscribes to entity types, renders entities via `EntityRenderer` and `WidgetReconciler`.
- **`waft-ui-gtk`** - GTK4 widget library: `WidgetBase` trait, `Child`/`Children` container types, `WidgetReconciler`, widget implementations (`FeatureToggleWidget`, `SliderWidget`, `IconWidget`, `MenuChevronWidget`).
- **`waft-config`** - Configuration loading from `~/.config/waft/config.toml`
- **`waft-i18n`** - Fluent localization: `system_locale()` returns BCP47 locale, `I18n` struct for translations with `t()` and `t_args()`.
- **`waft-settings`** - Standalone GTK4/libadwaita settings application. `AdwNavigationSplitView` with categorized sidebar, `gtk::Stack` for page switching, and `adw::NavigationView` for sub-page drill-down. Pages: Bluetooth, WiFi, Wired (Connectivity); Appearance, Display, Windows, Wallpaper (Visual); Audio, Notifications, Sounds (Feedback); Keyboard, Keyboard Shortcuts (Inputs); Weather (Info); Plugins, Services, Startup (System). Uses same `WaftClient` + `EntityStore` pattern as overview. Startup, Keyboard Shortcuts, and Windows pages use direct KDL config file editing (niri config) rather than entity-based approach. Appearance page has sub-pages for dark mode automation and night light configuration. Settings-app-specific preferences stored in `~/.config/waft/settings-app.toml`.
- **`waft-core`** - Common types: `Callback<T>`, `VoidCallback`, `DbusHandle` (zbus wrapper). Re-exports `waft-config`.
- **`waft-ipc`** - Legacy widget protocol types (being phased out).

### Plugins

All 16 plugins are standalone daemon binaries implementing the `Plugin` trait from `waft-plugin`. They provide domain entities to the central daemon, which routes updates to subscribed apps.

| Plugin              | Entity Types                                                                                                                | Purpose                                                             |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| **clock**           | `clock`                                                                                                                     | Current time and date with locale support                           |
| **darkman**         | `dark-mode`                                                                                                                 | Dark mode toggle via darkman D-Bus                                  |
| **caffeine**        | `sleep-inhibitor`                                                                                                           | Prevent sleep/screensaver (Portal/ScreenSaver)                      |
| **battery**         | `battery`                                                                                                                   | Battery percentage, health, charging (UPower D-Bus)                 |
| **brightness**      | `display`                                                                                                                   | Display brightness (brightnessctl/ddcutil)                          |
| **keyboard-layout** | `keyboard-layout`                                                                                                           | Input method display/switch (Niri/Sway/Hyprland/localed)            |
| **systemd**         | `session`, `user-service`                                                                                                   | Session actions and user service management via systemd             |
| **bluez**           | `bluetooth-adapter`, `bluetooth-device`                                                                                     | Bluetooth device management (BlueZ D-Bus)                           |
| **audio**           | `audio-device`                                                                                                              | Volume sliders, device selection (pactl)                            |
| **networkmanager**  | `network-adapter`, `wifi-network`, `ethernet-connection`, `vpn`                                                             | WiFi/Ethernet/VPN management (nmrs + zbus)                          |
| **weather**         | `weather`                                                                                                                   | Weather information via HTTP API                                    |
| **notifications**   | `notification`, `dnd`, `notification-group`, `notification-profile`, `active-profile`, `sound-config`, `notification-sound`, `recording` | D-Bus notification server, toasts, DND, filtering, sound, recording |
| **eds**             | `calendar-event`                                                                                                            | EDS calendar integration                                            |
| **gsettings**       | `gtk-appearance`                                                                                                            | GTK accent colour configuration via gsettings CLI                   |
| **sunsetr**         | `night-light`                                                                                                               | Night light control via sunsetr CLI                                 |
| **syncthing**       | `backup-method`                                                                                                             | Syncthing service toggle                                            |

Additionally, _session lock detection_ is an internal feature in `crates/overview/src/features/session/`.

### Entity-Based Architecture

All communication flows through the central `waft` daemon via Unix sockets using length-prefixed JSON.

```
Plugin (daemon)  <-->  waft (central daemon)  <-->  waft-overview (GTK overlay)
                                               <-->  waft-settings (GTK settings app)
```

**Central daemon (`waft`):**

- Discovers plugin binaries (`waft-*-daemon`) via `WAFT_DAEMON_DIR` env var or auto-detection (`./target/{debug,release}`, `/usr/bin`)
- Spawns plugins on demand when an app first subscribes to their entity types
- Routes entity updates from plugins to subscribed apps, actions from apps to plugins
- Tracks actions by UUID with configurable timeouts
- Detects crashes: sends `EntityStale` on restart, `EntityOutdated` after 5 crashes in 60s
- Graceful shutdown via `CanStop` when no subscribers remain
- Emits `plugin-status` entities as a meta-plugin (Available/Running/Stopped/Failed lifecycle states)
- Handles `Describe` requests: returns `PluginDescription` data cached from plugin discovery

**Plugin SDK (`waft-plugin`):**

- `Plugin` trait (Send+Sync): `get_entities()`, `handle_action()`, `can_stop()`, `describe()` (optional)
- `PluginRuntime` manages socket connection and message handling
- `EntityNotifier` pushes updates via `notify()`
- `PluginManifest`: `entity_types`, optional `name`, `description`; extended `provides --describe` returns `PluginDescription`

**Protocol (`waft-protocol`):**

- Entity types organized by domain (e.g. `entity::display::DarkMode`, `entity::audio::AudioDevice`)
- URN format: `{plugin}/{entity-type}/{id}[/{entity-type}/{id}]*`
- Messages: `AppMessage` (Subscribe, TriggerAction, Describe), `PluginMessage` (EntityUpdated, EntityRemoved, ActionSuccess/Error), `AppNotification` (DescribeResponse)
- Static protocol registry: `entity::registry::all_entity_types()` returns compile-time entity type metadata (descriptions, URN patterns, properties, actions)
- Plugin descriptions: `description::PluginDescription` with entity type details, obtained via `provides --describe` at discovery time
- Transport: 4-byte big-endian length prefix + JSON payload over Unix sockets

**Overview app (`waft-overview`):**

- `WaftClient` connects to `$XDG_RUNTIME_DIR/waft/daemon.sock` with retry + D-Bus activation
- `EntityRenderer` maps entity types to GTK widgets via `WidgetReconciler`
- Write path: `std::sync::mpsc` + OS thread (GTK->daemon, bypasses tokio)
- Read path: tokio task -> flume -> `glib::spawn_future_local`

**Plugin Pattern:**

```rust
#[async_trait::async_trait]
impl Plugin for MyPlugin {
    fn get_entities(&self) -> Vec<Entity>;
    async fn handle_action(&self, urn: Urn, action: String, params: serde_json::Value)
        -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn can_stop(&self) -> bool { true }
}
let (runtime, notifier) = PluginRuntime::new("name", plugin);
runtime.run().await?;
```

**Main function pattern:**

```rust
fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides(&[ENTITY_TYPE]) { return Ok(()); }
    waft_plugin::init_plugin_logger("info");
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { /* create plugin, runtime, spawn tasks */ })
}
```

### Directory Structure

```
Cargo.toml                        # Workspace root
crates/
    protocol/                     # waft-protocol: entity types, messages, URN, transport
    plugin/                       # waft-plugin: Plugin trait, PluginRuntime, EntityNotifier
    waft/                         # waft: central daemon (routing, discovery, lifecycle)
    overview/                     # waft-overview: main GTK4 overlay binary
        src/
            main.rs               # Tokio entrypoint
            app.rs                # Window setup, entity event loop
            waft_client.rs        # WaftClient, daemon_connection_task, OverviewEvent
            ui/
                calendar/
                    month_grid.rs # Calendar grid with ISO week numbers
            components/
                right_column_stack.rs  # Tabbable right column (controls/exit ViewStack)
            features/
                session/          # Session lock detection (internal, not a plugin)
    waft-ui-gtk/                  # GTK4 widget library
        src/
            widgets/              # FeatureToggleWidget, SliderWidget, IconWidget, etc.
    config/                       # waft-config: TOML config loading
    core/                         # waft-core: Callback, DbusHandle (legacy, being reduced)
    i18n/                         # waft-i18n: Fluent localization
    ipc/                          # waft-ipc: legacy widget protocol (being phased out)
plugins/
    clock/          bin/          # Entity types: clock
    darkman/        bin/          # Entity types: dark-mode
    caffeine/       bin/          # Entity types: sleep-inhibitor
    battery/        bin/          # Entity types: battery
    brightness/     bin/          # Entity types: display
    keyboard-layout/ bin/         # Entity types: keyboard-layout
    systemd/        bin/          # Entity types: session, user-service
    bluez/          bin/          # Entity types: bluetooth-adapter, bluetooth-device
    audio/          bin/          # Entity types: audio-device
    networkmanager/ bin/          # Entity types: network-adapter, wifi-network, etc.
    weather/        bin/          # Entity types: weather
    notifications/  bin/          # Entity types: notification, dnd, recording
    eds/            bin/          # Entity types: calendar-event
    gsettings/      bin/          # Entity types: gtk-appearance
    sunsetr/        bin/          # Entity types: night-light
    syncthing/      bin/          # Entity types: backup-method
crates/
    settings/                     # waft-settings: standalone settings application
        src/
            main.rs               # GTK entrypoint
            app.rs                # Entity subscriptions, action writer, client setup
            window.rs             # AdwNavigationSplitView with gtk::Stack page switching
            sidebar.rs            # Categorized sidebar (Connectivity, Visual, Feedback, Inputs, Info, System)
            pages/
                appearance.rs     # Thin composer: dark mode, night light, accent colour sections + sub-page navigation
                bluetooth.rs      # Smart container: adapter groups + device lists
                wifi.rs           # Smart container: WiFi adapters + network lists
                wired.rs          # Smart container: Ethernet adapters + connection profiles
                display.rs        # Smart container: per-output display controls
                keyboard.rs       # Smart container: keyboard layout selection
                keyboard_shortcuts.rs  # Smart container: niri keyboard bind management (KDL)
                niri_windows.rs   # Smart container: niri window appearance settings (KDL)
                notifications.rs  # Smart container: groups, profiles, DND
                sounds.rs         # Thin composer: defaults + gallery sections
                startup.rs        # Smart container: niri spawn-at-startup entries (KDL)
                wallpaper.rs      # Smart container: wallpaper mode, preview, gallery
                weather.rs        # Smart container: weather display
                plugins.rs        # Smart container: plugin lifecycle status
                services.rs       # Smart container: systemd user services
            bluetooth/            # Dumb widgets: adapter_group, device_row, paired/discovered groups
            display/              # Widgets: accent_colour_section, dark_mode_section, night_light_section, settings_sub_page, and more
            wifi/                 # Dumb widgets: adapter_group, network_row, known/available groups
            wired/                # Dumb widgets: adapter_group, connection_row
            niri_windows/         # Dumb widgets: focus_ring, border, shadow, tab_indicator, gaps, struts, derive_colors sections
            wallpaper/            # Widgets: gallery_section, thumbnail_widget, preview_section, mode_section, background_color_section
            startup/              # Widgets: startup_row, entry_dialog
            keyboard_shortcuts/   # Widgets: bind_row, bind_editor
            plugins/              # Dumb widgets: plugin_row
            sounds/               # Smart sections: defaults_section, gallery_section
            kdl_niri_windows.rs   # KDL I/O for niri layout block (focus-ring, border, shadow, etc.)
            prefs.rs              # Settings-app-specific preferences (settings-app.toml)
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

**Two-Tier Plugin Manifest:**

Plugins expose metadata through a two-tier manifest system:

- **Tier 1 (basic)**: `handle_provides(&[entity_types])` or `handle_provides_full(entity_types, name, description)` -- returns `PluginManifest` with entity type list and optional display metadata. Used for `waft plugin ls`.
- **Tier 2 (described)**: `handle_provides_described(entity_types, name, description, &plugin)` -- when called with `provides --describe`, invokes `Plugin::describe()` to return `PluginManifestDescribed` with full `PluginDescription` (entity type descriptions, properties, actions). Falls back to Tier 1 if the plugin returns `None` from `describe()`. Used for `waft plugin describe <name>`.

All three functions live in `waft_plugin::manifest`. Plugins that don't implement `describe()` work with both tiers (Tier 2 gracefully degrades).

**Daemon as Meta-Plugin (plugin-status entities):**

The daemon itself emits `plugin-status` entities with URN `waft/plugin-status/{plugin-name}`. This is the only entity type that uses a fixed `"waft"` prefix instead of `{plugin}` in the URN pattern. The daemon tracks four lifecycle states: `Available` (discovered, not spawned), `Running` (connected), `Stopped` (graceful CanStop), `Failed` (circuit breaker tripped). See `entity::plugin::PluginStatus`.

**Static Protocol Registry vs Runtime Plugin Descriptions:**

There are two distinct sources of entity type documentation:

- **Static registry** (`waft_protocol::entity::registry::all_entity_types()`): Compile-time metadata defined in the protocol crate. Contains domain-level entity type info (descriptions, URN patterns, property schemas, action schemas). Used by `waft protocol` CLI and for reference documentation. **Use this** when you need protocol-level documentation that is independent of which plugins are running.
- **Runtime plugin descriptions** (`PluginDescription` from `provides --describe`): Per-plugin metadata obtained at discovery time, potentially localized. Contains the same structure but from the plugin's perspective. Used by `waft plugin describe` CLI and the daemon's `Describe` message. **Use this** when you need plugin-specific documentation that may include localized labels.

---

## Critical Rules

### Known Tech Debt

**`vrr_supported`/`vrr_enabled` in `DisplayOutput`**: These boolean fields in `entity::display::DisplayOutput` violate the project's boolean naming convention (state names, not questions). Should be renamed to `vrr_support`/`vrr` or similar. Deferred because it requires coordinated changes across protocol, niri plugin, and display settings page.

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

Boolean fields should be named as states/properties, not as questions. Reserve the "is*/has*/can\_" prefix for functions/methods that return booleans.

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

Rationale: Boolean fields are answers to questions, not questions themselves. The "is*/has*" prefix suggests a method that returns a boolean. Use simple, direct property names for boolean fields.

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

### EntityStore Subscription Pattern with Initial Reconciliation

**Problem:** `EntityStore::subscribe_type()` only calls callbacks when entities change, not on initial subscription. If `EntityUpdated` notifications arrive before subscriptions are registered, the UI never reconciles with cached data.

**Solution:** Always trigger manual reconciliation after setting up subscriptions:

```rust
// 1. Set up subscriptions
entity_store.subscribe_type(EntityType::ENTITY_TYPE, move || {
    let entities = store.get_entities_typed(EntityType::ENTITY_TYPE);
    Self::reconcile(&state, &entities, &callback);
});

// 2. Trigger initial reconciliation with cached data
{
    let state_clone = state.clone();
    let store_clone = entity_store.clone();
    let cb_clone = action_callback.clone();

    gtk::glib::idle_add_local_once(move || {
        let entities = store_clone.get_entities_typed(EntityType::ENTITY_TYPE);
        if !entities.is_empty() {
            log::debug!("[component] Initial reconciliation: {} entities", entities.len());
            Self::reconcile(&state_clone, &entities, &cb_clone);
        }
    });
}
```

**Why `idle_add_local_once`?** Defers execution until after current GTK event processing completes, ensures all subscription setup is complete, and prevents RefCell borrow conflicts.

**Examples:** See `crates/settings/src/pages/bluetooth.rs`, `wifi.rs`, `wired.rs`

### Threading Model

**Overview (GTK host):**

- GTK widgets are **not** `Send`/`Sync` -- live on main thread only
- Never mutate GTK from Tokio tasks
- Use channels or `glib::MainContext::invoke_local` for GTK updates from async code
- Anything moved into `tokio::spawn(...)` must be `Send`

**Daemon plugins:**

- All `Send + Sync` (enforced by `Plugin` trait)
- Pure tokio context -- no GTK, no glib
- Shared state: `Arc<StdMutex<T>>` between plugin struct and monitoring tasks
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

- **Plugin** - A standalone tokio binary implementing the `Plugin` trait (Send+Sync) from `waft-plugin`. Provides domain entities to the central daemon.
- **Entity** - A typed piece of domain data (e.g. `DarkMode`, `AudioDevice`, `Battery`) defined in `waft-protocol`. Plugins produce entities; apps consume them.
- **URN** - Hierarchical entity identifier: `{plugin}/{entity-type}/{id}[/{entity-type}/{id}]*`.
- **Central Daemon (`waft`)** - Routes entities between plugins and apps, manages plugin lifecycle.
- **WaftClient** - Overview component that connects to the central daemon and manages subscriptions (`crates/overview/src/waft_client.rs`).
- **EntityNotifier** - Plugin-side mechanism to push entity updates to the central daemon.
- **PluginRuntime** - Plugin-side socket server that handles the connection to the central daemon and routes messages.
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

1. **Plugins provide entities, apps render UI** -- Plugins return domain entities via `get_entities()`; apps map entity types to GTK widgets independently
2. **NEVER do exceptional programming. ALWAYS select the systemic approach** -- Define general mechanisms first, then use for specific cases
3. **NO POLLING** -- Sleep to next event boundary (D-Bus signals, timer boundaries)
4. **Plugin state is plugin-local** -- Each plugin owns domain state; UI composes what plugins provide
5. **Explicit state flow** -- Avoid hidden couplings; expose explicit APIs or events
6. **GTK->tokio writes**: `std::sync::mpsc` + `std::thread` (bypasses tokio scheduler)

---

## Current Status

**All 16 plugins use the entity-based architecture** with central daemon routing.

- `waft-protocol` with entity types, messages, URN, transport, static entity registry, plugin descriptions
- `waft-plugin` with `Plugin` trait, `PluginRuntime`, `EntityNotifier`, extended manifest (`provides --describe`)
- `waft` central daemon with discovery, spawning, routing, crash recovery, CLI (clap), plugin-status meta-entities
- `waft-overview` with `WaftClient`, `EntityRenderer`, socket reconnection, right column tabs (controls/exit), ISO week numbers in calendar, audio device name deduplication
- `waft-settings` with Bluetooth, WiFi, Wired, Appearance (with sub-pages and accent colour), Display, Wallpaper (with gallery and background colour), Windows (niri layout settings), Audio, Notifications (with recording toggle), Sounds, Keyboard, Keyboard Shortcuts, Weather, Plugins, Services, Startup pages

**Legacy crates** (`waft-ipc`, parts of `waft-core`) are still in the workspace but being phased out.
**Active Branch:** `larger-larger-picture`

---

## Future Work & Known Limitations

### Planned Features

- **SNI (Status Notifier Items) support** -- Systray compatibility
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
