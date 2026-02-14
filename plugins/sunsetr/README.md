# Sunsetr Plugin

Night light control via the [sunsetr](https://github.com/lbgracioso/sunsetr) CLI tool. Provides a toggle to start/stop the sunsetr process and displays the current period (day/night), next transition time, and available presets.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `night-light` | `sunsetr/night-light/default` | Night light state and configuration |

### `night-light` entity

- `active` - Whether sunsetr process is running
- `period` - Current period ("day", "night", or custom)
- `next_transition` - Next period transition time (HH:MM)
- `presets` - Available preset names
- `active_preset` - Currently active preset (None = default)

## Actions

| Action | Params | Description |
|---|---|---|
| `toggle` | - | Start or stop the sunsetr process |
| `select_preset` | `"preset_name"` (string) | Switch to a named preset |

## External Tool Integration

The plugin communicates with sunsetr through its CLI:

- `sunsetr -b` / `sunsetr start` - Start the background process
- `sunsetr stop` / `sunsetr off` - Stop the process
- `sunsetr S --json` - Query current status
- `sunsetr S --json --follow` - Live event stream (spawned in a background thread)
- `sunsetr preset list` - List available presets
- `sunsetr preset <name>` - Switch preset

## Configuration

```toml
[[plugins]]
id = "sunsetr"
```

No plugin-specific configuration options. Sunsetr itself is configured through its own config files.

## Lifecycle

The plugin checks for the `sunsetr` binary on startup and exits gracefully if not found. A background thread runs `sunsetr S --json --follow` to receive live period transition events without polling.

## Dependencies

- [sunsetr](https://github.com/lbgracioso/sunsetr) CLI tool in `$PATH`
