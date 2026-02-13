# waft

A Wayland-native overlay panel built with Rust, GTK4, and libadwaita. Plugins run as daemon processes communicating through a central daemon (`waft`) that routes entity data and actions.

## Components

- **`waft`** - Central daemon that discovers, spawns, and supervises plugins. Routes entity updates and actions between plugins and apps via Unix sockets. Registered as `org.waft.Daemon` on D-Bus.
- **`waft-overview`** - GTK4/libadwaita overlay panel using Wayland layer-shell. Connects to the daemon, subscribes to entity types, and renders UI.

## Building

```bash
cargo build --workspace
cargo test --workspace
```

## Running

```bash
# Start the central daemon (or let D-Bus activation start it)
waft

# Start the overlay
waft-overview

# Development: run daemon with plugins from build output
WAFT_DAEMON_DIR=./target/debug waft
```

## Configuration

Configuration is loaded from `~/.config/waft/config.toml`.

```toml
[[plugins]]
id = "plugin::clock"

[[plugins]]
id = "plugin::darkman"

[[plugins]]
id = "plugin::notifications"
toast_limit = 3
disable_toasts = false
```

## Plugins

All plugins are standalone daemon binaries implementing the `Plugin` trait from `waft-plugin`. They provide domain entities to the central daemon, which routes updates to subscribed apps.

| Plugin | Binary | Entity Types | Description |
|--------|--------|--------------|-------------|
| clock | waft-clock-daemon | `clock` | Date and time with locale support |
| darkman | waft-darkman-daemon | `dark-mode` | Dark mode toggle via darkman D-Bus |
| caffeine | waft-caffeine-daemon | `sleep-inhibitor` | Prevent sleep/screensaver via Portal |
| battery | waft-battery-daemon | `battery` | Battery status via UPower D-Bus |
| brightness | waft-brightness-daemon | `display` | Display brightness via brightnessctl/ddcutil |
| keyboard-layout | waft-keyboard-layout-daemon | `keyboard-layout` | Input method display/switch (Niri/Sway/Hyprland/localed) |
| systemd-actions | waft-systemd-actions-daemon | `session` | Lock, logout, reboot, shutdown, suspend |
| blueman | waft-blueman-daemon | `bluetooth-adapter`, `bluetooth-device` | Bluetooth management via BlueZ D-Bus |
| audio | waft-audio-daemon | `audio-device` | Volume sliders and device selection via pactl |
| networkmanager | waft-networkmanager-daemon | `network-adapter`, `wifi-network`, `ethernet-connection`, `vpn` | WiFi/Ethernet/VPN via NetworkManager |
| weather | waft-weather-daemon | `weather` | Weather info via Open-Meteo API |
| notifications | waft-notifications-daemon | `notification`, `dnd` | D-Bus notification server with toasts and DND |
| eds | waft-eds-daemon | `calendar-event` | Calendar integration via Evolution Data Server |
| sunsetr | waft-sunsetr-daemon | `night-light` | Night light control via sunsetr CLI |
| syncthing | waft-syncthing-daemon | `backup-method` | Syncthing service toggle |

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full entity-based architecture design.

### Crate Structure

| Crate | Path | Purpose |
|-------|------|---------|
| `waft-protocol` | `crates/protocol/` | Entity types, messages, URN format, transport |
| `waft-plugin` | `crates/plugin/` | Plugin SDK: `Plugin` trait, `PluginRuntime`, `EntityNotifier` |
| `waft` | `crates/waft/` | Central daemon: routing, lifecycle, discovery |
| `waft-overview` | `crates/overview/` | GTK4 overlay app with `WaftClient` |
| `waft-ui-gtk` | `crates/waft-ui-gtk/` | GTK4 widget library: `WidgetReconciler`, widget implementations |
| `waft-config` | `crates/config/` | TOML configuration loading |
| `waft-i18n` | `crates/i18n/` | Fluent localization |
| `waft-core` | `crates/core/` | Common types: `Callback`, `DbusHandle` |
| `waft-ipc` | `crates/ipc/` | Legacy widget protocol (being phased out) |

### Communication Flow

```
Plugin (daemon)  <-->  waft (central daemon)  <-->  waft-overview (GTK app)
```

- Plugins provide **entities** (typed domain data) to the daemon
- Apps **subscribe** to entity types and receive push updates
- Actions flow from apps through the daemon to the owning plugin
- Transport: length-prefixed JSON over Unix sockets

### Threading Model

- **GTK widgets** live on the main thread (not `Send`/`Sync`)
- **Plugin daemons** are pure tokio (all `Send + Sync`)
- **GTK-to-daemon writes**: `std::sync::mpsc` + OS thread (bypasses tokio)
- **Daemon-to-GTK reads**: tokio task -> flume -> `glib::spawn_future_local`

## Notification Server

The notifications plugin owns `org.freedesktop.Notifications` on D-Bus and supports:
- Notifications with actions, body, and body-markup
- In-memory storage (not persisted across restarts)
- Action invocation and notification replacement
- Do Not Disturb mode

## External Dependencies

- [darkman](https://darkman.whynothugo.nl/) for dark mode toggling
- [sunsetr](https://github.com/just-paja/sunsetr) for night light control
- [brightnessctl](https://github.com/haikarainen/brightnessctl) / [ddcutil](https://www.ddcutil.com/) for display brightness
- [pactl](https://www.freedesktop.org/wiki/Software/PulseAudio/) (PipeWire/PulseAudio) for audio control

## License

See LICENSE file.
