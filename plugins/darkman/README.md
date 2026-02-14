# Darkman Plugin

Toggles dark mode via the [darkman](https://darkman.whynothugo.nl/) D-Bus service. Monitors for external mode changes so the UI stays in sync when dark mode is toggled from outside Waft.

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `dark-mode` | `darkman/dark-mode/default` | Whether dark mode is active |

## Actions

| Action | Description |
|---|---|
| `toggle` | Switches between dark and light mode |

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
