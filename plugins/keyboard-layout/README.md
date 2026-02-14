# Keyboard Layout Plugin

Displays the current keyboard layout and allows cycling through configured layouts. Automatically detects the Wayland compositor and uses the appropriate backend.

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `keyboard-layout` | `keyboard-layout/keyboard-layout/default` | Current layout abbreviation and list of available layouts |

## Actions

| Action | Description |
|---|---|
| `cycle` | Switch to the next keyboard layout (wraps around) |

## Backends

The plugin detects backends in priority order:

| Backend | Detection | Query | Switch | Live Events |
|---|---|---|---|---|
| **Niri** | `NIRI_SOCKET` env var | `niri msg --json keyboard-layouts` | `niri msg action switch-layout next` | `niri msg --json event-stream` |
| **Sway** | `SWAYSOCK` env var | `swaymsg -t get_inputs` | `swaymsg input type:keyboard xkb_switch_layout next` | `swaymsg -t subscribe -m '["input"]'` |
| **Hyprland** | `HYPRLAND_INSTANCE_SIGNATURE` env var | `hyprctl devices -j` | `hyprctl switchxkblayout all next` | Unix socket at `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock` |
| **systemd-localed** | D-Bus `org.freedesktop.locale1` available | `X11Layout` property | `SetX11Keyboard` method | `PropertiesChanged` signal (config changes only) |

All backends subscribe to live layout change events so the UI updates instantly when the layout is switched externally (e.g., via keyboard shortcut). The localed backend is limited to detecting configuration changes only, not runtime switches.

## Layout Abbreviation

Layout names are converted to short abbreviations:
- Parenthesized codes: "English (US)" -> "US"
- Language lookup: "Czech" -> "CZ", "German" -> "DE"
- XKB codes: "us" -> "US"

## D-Bus Interfaces

| Bus | Service | Path | Usage |
|---|---|---|---|
| System | `org.freedesktop.locale1` | `/org/freedesktop/locale1` | Fallback: read `X11Layout`, call `SetX11Keyboard` |

## Configuration

```toml
[[plugins]]
id = "keyboard-layout"
```

No plugin-specific configuration options.

## Dependencies

One of the following compositor tools (auto-detected):
- **niri** (Niri compositor)
- **swaymsg** (Sway compositor)
- **hyprctl** (Hyprland compositor)
- **systemd-localed** via D-Bus (fallback)
