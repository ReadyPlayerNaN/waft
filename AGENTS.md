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
- **`waft-launcher`** - Standalone GTK4/libadwaita launcher application. Searches XDG applications and niri compositor windows. Supports fuzzy matching, keyboard navigation, and usage-based ranking. Subscribes to `app` and `window` entity types.
- **`waft-settings`** - Standalone GTK4/libadwaita settings application. `AdwNavigationSplitView` with categorized sidebar, `gtk::Stack` for page switching, and `adw::NavigationView` for sub-page drill-down. Pages: Bluetooth, WiFi, Wired, Online Accounts (Connectivity); Appearance, Display, Windows, Wallpaper (Visual); Audio, Notifications, Sounds (Feedback); Keyboard, Keyboard Shortcuts (Inputs); Weather (Info); Plugins, Services, Startup (System). Uses same `WaftClient` + `EntityStore` pattern as overview. Startup, Keyboard Shortcuts, and Windows pages use direct KDL config file editing (niri config) rather than entity-based approach. Appearance page has sub-pages for dark mode automation and night light configuration. Settings-app-specific preferences stored in `~/.config/waft/settings-app.toml`.
- **`waft-core`** - Common types: `Callback<T>`, `VoidCallback`, `DbusHandle` (zbus wrapper). Re-exports `waft-config`.
- **`waft-ipc`** - Legacy widget protocol types (being phased out).

### Plugins

All 18 plugins are standalone daemon binaries implementing the `Plugin` trait from `waft-plugin`. They provide domain entities to the central daemon, which routes updates to subscribed apps.

| Plugin              | Entity Types                                                                                                                | Purpose                                                             |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| **clock**           | `clock`                                                                                                                     | Current time and date with locale support                           |
| **darkman**         | `dark-mode`                                                                                                                 | Dark mode toggle via darkman D-Bus                                  |
| **caffeine**        | `sleep-inhibitor`                                                                                                           | Prevent sleep/screensaver (Portal/ScreenSaver)                      |
| **battery**         | `battery`                                                                                                                   | Battery percentage, health, charging (UPower D-Bus)                 |
| **brightness**      | `display`                                                                                                                   | Display brightness with connector resolution, inotify monitoring (brightnessctl/ddcutil) |
| **keyboard-layout** | `keyboard-layout`                                                                                                           | Input method display/switch (Niri/Sway/Hyprland/localed)            |
| **niri**            | `keyboard-layout`, `keyboard-layout-config`, `display-output`, `window`                                                     | Niri compositor integration (layouts, displays, windows)            |
| **systemd**         | `session`, `user-service`                                                                                                   | Session actions and user service management via systemd             |
| **bluez**           | `bluetooth-adapter`, `bluetooth-device`                                                                                     | Bluetooth device management (BlueZ D-Bus)                           |
| **audio**           | `audio-device`                                                                                                              | Volume sliders, device selection (pactl)                            |
| **networkmanager**  | `network-adapter`, `wifi-network`, `ethernet-connection`, `vpn`, `tethering-connection`                                     | WiFi/Ethernet/VPN/Tethering management (nmrs + zbus)                |
| **weather**         | `weather`                                                                                                                   | Weather information via HTTP API                                    |
| **notifications**   | `notification`, `dnd`, `notification-group`, `notification-profile`, `active-profile`, `sound-config`, `notification-sound`, `recording` | D-Bus notification server, toasts, DND, filtering, sound, recording |
| **eds**             | `calendar-event`                                                                                                            | EDS calendar integration                                            |
| **gnome-online-accounts** | `online-account`, `online-account-provider`                                                                           | GNOME Online Accounts status, service toggles, provider discovery, add-account (GOA D-Bus) |
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

- `Plugin` trait (Send+Sync): `get_entities()`, `handle_action() -> Result<serde_json::Value>` (returns response data or `Value::Null`), `can_stop()`, `describe()` (optional)
- `PluginRuntime` manages socket connection and message handling
- `EntityNotifier` pushes updates via `notify()`
- `PluginManifest`: `entity_types`, optional `name`, `description`; extended `provides --describe` returns `PluginDescription`

**Protocol (`waft-protocol`):**

- Entity types organized by domain (e.g. `entity::display::DarkMode`, `entity::audio::AudioDevice`)
- URN format: `{plugin}/{entity-type}/{id}[/{entity-type}/{id}]*`
- Messages: `AppMessage` (Subscribe, TriggerAction, Describe), `PluginMessage` (EntityUpdated, EntityRemoved, ActionSuccess/Error with optional response data), `AppNotification` (DescribeResponse, ActionSuccess with optional data)
- Static protocol registry: `entity::registry::all_entity_types()` returns compile-time entity type metadata (descriptions, URN patterns, properties, actions)
- Plugin descriptions: `description::PluginDescription` with entity type details, obtained via `provides --describe` at discovery time
- Transport: 4-byte big-endian length prefix + JSON payload over Unix sockets

**Overview app (`waft-overview`):**

- `WaftClient` connects to `$XDG_RUNTIME_DIR/waft/daemon.sock` with retry + D-Bus activation
- `EntityRenderer` maps entity types to GTK widgets via `WidgetReconciler`
- Write path: `std::sync::mpsc` + OS thread (GTK->daemon, bypasses tokio)
- Read path: tokio task -> flume -> `glib::spawn_future_local`

**Plugin Pattern:** See `create-daemon-plugin` skill for Plugin trait, main function pattern, and full step-by-step guide.

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
    launcher/                     # waft-launcher: standalone launcher application
        src/
            app.rs                # Entity subscriptions, action dispatch, usage tracking
            ranking.rs            # RankedResult enum (App/Window), fuzzy scoring, usage boost
            fuzzy.rs              # Fuzzy string matching algorithm
            usage.rs              # Launch frequency tracking (XDG data dir)
            window.rs             # LauncherWindow, layer-shell setup, animations
    ipc/                          # waft-ipc: legacy widget protocol (being phased out)
plugins/
    clock/          bin/          # Entity types: clock
    darkman/        bin/          # Entity types: dark-mode
    caffeine/       bin/          # Entity types: sleep-inhibitor
    battery/        bin/          # Entity types: battery
    brightness/     bin/          # Entity types: display
    keyboard-layout/ bin/         # Entity types: keyboard-layout
    niri/           bin/          # Entity types: keyboard-layout, keyboard-layout-config, display-output, window
    systemd/        bin/          # Entity types: session, user-service
    bluez/          bin/          # Entity types: bluetooth-adapter, bluetooth-device
    audio/          bin/          # Entity types: audio-device
    networkmanager/ bin/          # Entity types: network-adapter, wifi-network, etc.
    weather/        bin/          # Entity types: weather
    notifications/  bin/          # Entity types: notification, dnd, recording
    eds/            bin/          # Entity types: calendar-event
    gnome-online-accounts/ bin/   # Entity types: online-account
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
                wifi.rs           # Smart container: WiFi adapters + network lists + detail sub-page (forget, settings, QR share)
                wired.rs          # Smart container: Ethernet adapters + connection profiles
                online_accounts.rs # Smart container: GOA accounts + service toggles + provider picker for add-account
                display.rs        # Smart container: unified display sections (brightness + output controls correlated by connector)
                keyboard.rs       # Smart container: keyboard layout selection
                keyboard_shortcuts.rs  # Smart container: niri keyboard bind management (KDL)
                niri_windows.rs   # Smart container: niri window appearance settings (KDL)
                audio.rs          # Smart container: audio device grouping + virtual device controls
                notifications.rs  # Smart container: groups, profiles, DND
                sounds.rs         # Thin composer: defaults + gallery sections
                startup.rs        # Smart container: niri spawn-at-startup entries (KDL)
                wallpaper.rs      # Smart container: wallpaper mode, preview, gallery
                weather.rs        # Smart container: weather display
                plugins.rs        # Smart container: plugin lifecycle status
                services.rs       # Smart container: systemd user services
            bluetooth/            # Dumb widgets: adapter_group, device_row, paired/discovered groups
            display/              # Widgets: accent_colour_section, dark_mode_section, night_light_section, settings_sub_page, and more
            wifi/                 # Dumb widgets: adapter_group, network_row, known/available groups, password_dialog, network_detail, share_dialog
            wired/                # Dumb widgets: adapter_group, connection_row
            online_accounts/      # Dumb widgets: account_row, provider_picker_dialog
            niri_windows/         # Dumb widgets: focus_ring, border, shadow, tab_indicator, gaps, struts, derive_colors sections
            wallpaper/            # Widgets: gallery_section, thumbnail_widget, preview_section, mode_section, background_color_section
            startup/              # Widgets: startup_row, entry_dialog
            audio/                # Dumb widgets: device_card, virtual_devices_section
            keyboard/             # Widgets: layout_row, keymap_grid, xkb_database, xkb_keymap, dialogs
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

### Coding Conventions (MUST follow)

See `waft-coding-conventions` skill: no generic module names (`utils`/`helpers`/`misc`), boolean fields use state names not questions (`input` not `is_input`), icons use `IconWidget` not `gtk::Image`.

### UI Component Architecture

See `widget-coding` skill: dumb widgets (`*Props`/`*Output`/`connect_output()`), smart containers with `Reconciler`, data flows down via props, events flow up via output callbacks.

### EntityStore Subscription Pattern

See `entity-store-subscription` skill: always subscribe first, then trigger initial reconciliation via `idle_add_local_once`.

### Threading and Runtime Mixing

See `async-runtime-bridge` skill: GTK widgets on main thread only, tokio futures never in `glib::spawn_future_local()`, always use `zbus = { features = ["tokio"] }`.

### Incremental UI Updates (must follow)

For DBus-driven UIs:

- Update only affected widgets, don't rebuild entire trees (causes flicker)
- Keep stable ordering (don't reorder rows on state changes)
- Use wake-on-demand invalidate queues for DBus signal bursts
- Gate updates to when overlay is visible

### Layer-Shell Window Dynamic Resizing

See `widget-coding` skill for the layer-shell resizing pattern (`set_default_size`, revealer notify, `idle_add_local_once`).

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

See `prevent-silent-hangs` skill: covers `let _ =` on fallible ops, logging async loop exits, background task error logging, broken send loops, mutex poison recovery, child process reaping, and bridge code panics.

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

**All 18 plugins use the entity-based architecture** with central daemon routing.

- `waft-protocol` with entity types, messages, URN, transport, static entity registry, plugin descriptions
- `waft-plugin` with `Plugin` trait, `PluginRuntime`, `EntityNotifier`, extended manifest (`provides --describe`)
- `waft` central daemon with discovery, spawning, routing, crash recovery, CLI (clap), plugin-status meta-entities
- `waft-overview` with `WaftClient`, `EntityRenderer`, socket reconnection, right column tabs (controls/exit), ISO week numbers in calendar, audio device name deduplication
- `waft-settings` with Bluetooth, WiFi (with network detail sub-page, forget, settings, QR sharing), Wired, Online Accounts (with provider picker for add-account), Appearance (with sub-pages and accent colour), Display (unified brightness + output controls per connector), Wallpaper (with gallery and background colour), Windows (niri layout settings), Audio (with virtual device volume/mute tracking), Notifications (with recording toggle), Sounds, Keyboard, Keyboard Shortcuts, Weather, Plugins, Services, Startup pages

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
