# sacrebleui

A GTK4/libadwaita overlay shell for Linux desktops. Provides a notification center, quick toggles, and status information through a plugin-based architecture.

## Installation

```bash
cargo build --release
```

## Usage

```bash
# Start the overlay (runs as a daemon)
sacrebleui

# Control the overlay from another terminal
sacrebleui show      # Show the overlay
sacrebleui hide      # Hide the overlay
sacrebleui toggle    # Toggle overlay visibility
sacrebleui stop      # Stop the daemon
```

## Configuration

Configuration is loaded from `~/.config/sacrebleui/config.toml`.

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
| `plugin::clock` | Displays current date and time | [README](src/features/clock/README.md) |
| `plugin::darkman` | Dark mode toggle via darkman | [README](src/features/darkman/README.md) |
| `plugin::sunsetr` | Night light toggle via sunsetr | [README](src/features/sunsetr/README.md) |
| `plugin::notifications` | Desktop notification handling | [README](src/features/notifications/README.md) |

## Architecture

sacrebleui uses a plugin-based architecture where each feature is implemented as a self-contained plugin. Plugins can:

- Provide widgets for the overlay UI
- Provide feature toggle buttons
- Integrate with system services via DBus or IPC
- Define their own configuration options

## Dependencies

- GTK4 with libadwaita
- For `plugin::darkman`: [darkman](https://darkman.whynothugo.nl/) running as a DBus service
- For `plugin::sunsetr`: [sunsetr](https://github.com/just-paja/sunsetr) CLI tool
- For `plugin::notifications`: Replaces your current notification daemon

## License

See LICENSE file.
