# GSettings Plugin

Configures the GTK accent colour via the `gsettings` CLI. Monitors for external changes through the XDG Desktop Portal `SettingChanged` D-Bus signal so the UI stays in sync when the accent colour is changed from GNOME Settings or the command line.

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `gtk-appearance` | `gsettings/gtk-appearance/default` | Current GTK appearance settings including accent colour |

## Actions

| Action | Parameters | Description |
|---|---|---|
| `set-accent-color` | `color` (string) | Set the system accent colour |

### Valid Accent Colours

`blue`, `teal`, `green`, `yellow`, `orange`, `red`, `pink`, `purple`, `slate`

These are the 9 predefined `AdwAccentColor` values from libadwaita. No custom hex colours are supported.

## D-Bus Interfaces

| Bus | Service | Path | Interface | Usage |
|---|---|---|---|---|
| Session | `org.freedesktop.portal.Desktop` | `/org/freedesktop/portal/desktop` | `org.freedesktop.portal.Settings` | Listen for `SettingChanged` signal (namespace `org.freedesktop.appearance`, key `accent-color`) |

The plugin does not own a D-Bus name. It reads and writes via the `gsettings` CLI and monitors the XDG Desktop Portal for external changes.

## Configuration

```toml
[[plugins]]
id = "gsettings"
```

No plugin-specific configuration options.

## Dependencies

- **gsettings** CLI on PATH (universally available on GNOME systems)
- **org.gnome.desktop.interface** schema with `accent-color` key (GNOME 47+)
- **XDG Desktop Portal** running on the session bus (for external change monitoring)

If the gsettings schema is not available (e.g. on non-GNOME systems), the plugin returns no entities and the settings UI hides the accent colour section automatically.
