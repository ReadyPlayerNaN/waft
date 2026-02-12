# waft

A Wayland-native toolkit monorepo containing multiple overlay UI applications.

## waft-overview

A Wayland-only overlay UI application built with GTK4 and libadwaita that provides a notification server and customizable overlay panel with a plugin-based architecture.

## Features

- **Wayland-native overlay** using layer-shell protocol
- **Notification server** with `org.freedesktop.Notifications` DBus ownership
- **Plugin system** for extensible feature toggles
- **IPC interface** for external control via Unix socket
- **GTK4/libadwaita** modern UI components

## Installation

```bash
cargo build --release --package waft-overview
```

## Usage

```bash
# Start the overlay (runs as a daemon)
waft-overview

# Control the overlay from another terminal
waft-overview show      # Show the overlay
waft-overview hide      # Hide the overlay
waft-overview toggle    # Toggle overlay visibility
waft-overview stop      # Stop the daemon
```

## Configuration

Configuration is loaded from `~/.config/waft/config.toml`.

Plugins must be explicitly enabled in the configuration file. If no configuration file exists or no plugins are listed, no plugins will be loaded.

### Example Configuration

```toml
[[plugins]]
id = "plugin::clock"

[[plugins]]
id = "plugin::darkman"

[[plugins]]
id = "plugin::sunsetr"

[[plugins]]
id = "plugin::notifications"
toast_limit = 3
disable_toasts = false
```

## Available Plugins

| Plugin ID | Description | Documentation |
|-----------|-------------|---------------|
| `plugin::clock` | Displays current date and time | [README](crates/overview/src/features/clock/README.md) |
| `plugin::darkman` | Dark mode toggle via darkman | [README](crates/overview/src/features/darkman/README.md) |
| `plugin::sunsetr` | Night light toggle via sunsetr | [README](crates/overview/src/features/sunsetr/README.md) |
| `plugin::notifications` | Desktop notification handling | [README](crates/overview/src/features/notifications/README.md) |

## Architecture

waft uses an async-first architecture with clear boundaries between the main thread (GTK widgets) and background tasks (Tokio runtime).

### Core Components

- **Entry point:** `crates/overview/src/main.rs` → `app::run()`
- **App model:** `crates/overview/src/app.rs` - Pure GTK4 app managing overlay window and plugin registry
- **Plugin system:** All plugins use daemon architecture via `crates/plugin-sdk/`
- **Plugin registry:** `crates/overview/src/plugin_registry.rs` - Non-`Send` plugin lifecycle management
- **Shared infrastructure:** `crates/waft-core/` - Store pattern, DbusHandle, menu state
- **Notifications:** `crates/overview/src/features/notifications/` - DBus server with reducer-based state management
- **IPC:** `crates/ipc/` - Commands over Unix socket (show/hide/toggle/ping/stop)

### Plugin System

All plugins use the **daemon architecture**: standalone executables communicating
with the overview via Unix socket IPC through the central `waft` daemon.

Plugin lifecycle:
- `configure(settings)` - Parse plugin-specific TOML config
- `init(resources)` - Async initialization with shared resources (DbusHandle, tokio Handle)
- `create_elements(app, menu_store, registrar)` - GTK widget construction (after GTK init)
- `cleanup()` - Graceful shutdown

Widgets placed into slots: `Info`, `Controls`, `Header`, `Actions`

**For plugin development:** See [docs/PLUGIN_DEVELOPMENT.md](docs/PLUGIN_DEVELOPMENT.md)

## Testing

```bash
cargo test                # Run all tests
cargo test --lib          # Library unit tests only
cargo test notifications_store_reduce  # Specific test module
```

## Notification Server

The application owns `org.freedesktop.Notifications` on DBus and supports:
- Basic notifications with actions, body, and body-markup capabilities
- In-memory persistence (no persistence across restarts)
- Action invocation and notification replacement
- Basic app icon lookup via theme icons

### Testing Notifications

```bash
# Verify DBus ownership
busctl --user status org.freedesktop.Notifications

# Basic notification
notify-send "test" "Hello"

# Markup support
notify-send "Markup" "<b>bold</b> <i>italic</i>"

# Action support
notify-send --action=default=Open "Action test" "Click action"

# Monitor DBus signals
dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"
```

## Design Principles

1. **Stable plugin boundaries** - Plugins expose UI via `widgets()` and/or `feature_toggles()`
2. **Plugin state is plugin-local** - Each plugin owns domain state
3. **Explicit state flow** - Avoid hidden couplings
4. **Main-thread UI** - Keep GTK widgets on main thread, use async for blocking work
5. **Incremental updates** - Update only affected widgets to prevent flicker

## Threading Model

- GTK widgets are **not** `Send`/`Sync` - main thread only
- Use channels for communication between Tokio tasks and GTK
- Never mutate GTK from Tokio tasks directly
- Split plugin state: Send-safe data for background, GTK state for main thread

## Critical Rules

### GTK Init Boundary
Never create GTK widgets in `init()` - GTK is not initialized yet. Use `create_elements()` or `get_widgets()` for widget construction.

### Runtime Mixing
Never run tokio-dependent futures in glib context - causes 100% CPU usage. Use `tokio::spawn()` for tokio work and executor-agnostic channels like `flume` for communication.

### Error Handling
- Never discard Results with `let _ =` - log or act on errors
- Log when async loops exit to detect unresponsive features
- Recover from mutex poison rather than panicking
- Reap child processes to avoid zombies

## Wayland Layer Shell

The overlay window uses Wayland layer-shell for proper integration:
- Dynamic resizing with `window.set_default_size(width, -1)` when content changes
- Constrain height with `ScrolledWindow.set_max_content_height()`
- Handle animated content resize via revealer callbacks

## Dependencies

- GTK4 + libadwaita for UI
- gtk4-layer-shell for Wayland layer-shell protocol
- Tokio for async runtime
- zbus 5.0 for DBus
- flume for executor-agnostic channels
- libloading for dynamic plugin loading
- For `plugin::darkman`: [darkman](https://darkman.whynothugo.nl/) running as a DBus service
- For `plugin::sunsetr`: [sunsetr](https://github.com/just-paja/sunsetr) CLI tool
- For `plugin::notifications`: Replaces your current notification daemon

## Documentation

- **Plugin Development:** [docs/PLUGIN_DEVELOPMENT.md](docs/PLUGIN_DEVELOPMENT.md) - How to create dynamic plugins
- **Architecture & Rules:** [AGENTS.md](AGENTS.md) - Comprehensive development guide for AI agents and contributors

## License

See LICENSE file.
