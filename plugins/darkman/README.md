# Darkman Plugin

Toggles dark mode via the [darkman](https://darkman.whynothugo.nl/) D-Bus service. Monitors for external mode changes so the UI stays in sync when dark mode is toggled from outside Waft.

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `dark-mode` | `darkman/dark-mode/default` | Whether dark mode is active |
| `dark-mode-automation-config` | `darkman/dark-mode-automation-config/default` | Configuration settings for darkman |

## Actions

| Action | Parameters | Description |
|---|---|---|
| `toggle` | none | Switches between dark and light mode |
| `update_field` | `field` (string), `value` (json) | Updates a configuration field |

### Configuration Fields

| Field | Type | Range | Description |
|---|---|---|---|
| `latitude` | float | -90 to 90 | Manual latitude for sunrise/sunset calculation |
| `longitude` | float | -180 to 180 | Manual longitude for sunrise/sunset calculation |
| `auto_location` | bool | | Auto-detect location via geoclue |
| `dbus_api` | bool | | Enable D-Bus API (required for waft) |
| `portal_api` | bool | | Enable XDG portal support |

Configuration is stored in `~/.config/darkman/config.yaml`. After changes, the plugin attempts to restart darkman via `systemctl --user restart darkman.service` (best-effort).

## D-Bus Interfaces

| Bus | Service | Path | Usage |
|---|---|---|---|
| Session | `nl.whynothugo.darkman` | `/nl/whynothugo/darkman` | Read/write `Mode` property, listen for `ModeChanged` signal |

The plugin reads and writes the `Mode` property (values: `"dark"`, `"light"`) via `org.freedesktop.DBus.Properties`. It subscribes to the `ModeChanged` signal for instant updates when the mode changes externally.

## Configuration

```toml
[[plugins]]
id = "darkman"
```

No plugin-specific configuration options.

## Dependencies

- **darkman** daemon running on the session bus
