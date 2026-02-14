# Brightness Plugin

Display brightness control for laptop backlights and external monitors.

## Purpose

Discovers controllable displays via two backends -- `brightnessctl` for laptop/internal backlights and `ddcutil` for external monitors over DDC/CI. Exposes one entity per display with a brightness slider.

## Entity Types

### `display`

One entity per controllable display.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Human-readable display name (e.g. "Built-in Display", monitor model) |
| `brightness` | `f64` | Current brightness level, 0.0 to 1.0 |
| `kind` | `DisplayKind` | `Backlight` (internal) or `External` (DDC/CI) |

### Actions

| Action | Params | Description |
|--------|--------|-------------|
| `set-brightness` | `{ "value": 0.5 }` | Set brightness (0.0 to 1.0) |

### URN Format

```
brightness/display/{device-id}
```

Device IDs are prefixed by backend:
- `backlight:intel_backlight` -- brightnessctl device
- `ddc:1` -- ddcutil display number

## System Interface

### brightnessctl (backlight backend)

- `brightnessctl -l -m -c backlight` -- discover backlight devices
- `brightnessctl -d {device} set {percent}%` -- set brightness

### ddcutil (external monitor backend)

- `ddcutil detect --brief` -- discover DDC/CI monitors
- `ddcutil getvcp 10 --brief -d {display}` -- read brightness (VCP code 0x10)
- `ddcutil setvcp 10 {value} -d {display}` -- set brightness

## Dependencies

- **brightnessctl** (optional) -- backlight control for laptops
- **ddcutil** (optional) -- DDC/CI control for external monitors

At least one backend tool must be available for the plugin to provide entities. Both are probed at startup; missing tools are skipped gracefully.

## Configuration

```toml
[[plugins]]
id = "brightness"
```

No plugin-specific configuration options. Displays are sorted with backlights first, then external monitors alphabetically.
