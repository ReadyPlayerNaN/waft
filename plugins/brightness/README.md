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
| `connector` | `Option<String>` | DRM connector name (e.g. "eDP-1", "DP-3"), if resolved via sysfs |

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

## External Brightness Change Detection

Backlight devices are monitored via inotify on `/sys/class/backlight/{device}/actual_brightness`. When brightness is changed by other tools (e.g. hardware keys, power management), the plugin detects the change within 50ms and updates the entity. This uses the `notify` crate with a debounce window to coalesce rapid sysfs writes.

## Connector Resolution

The plugin resolves DRM connector names for displays via sysfs traversal:

- **Backlight devices**: Follow `/sys/class/backlight/{device}/device` symlink to the PCI device, then search `/sys/class/drm/card*-*/` entries under the same PCI parent to find the connector.
- **DDC monitors**: Parse the I2C bus number from `ddcutil detect` output, then match it against `/sys/class/drm/card*-*/ddc` symlinks that point to the corresponding I2C adapter.

Connector names enable the settings UI to group brightness controls with their corresponding display output settings.

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

## Known Limitations

- **DDC/CI monitors do not detect external brightness changes.** DDC/CI has no notification mechanism, so brightness changes made by other tools (OSD buttons, other DDC utilities) are not reflected until the next manual interaction. Only backlight devices support automatic external change detection via inotify on sysfs.
- **Connector resolution may fail** if sysfs topology is unusual or if the DRM subsystem does not expose the expected symlinks. In that case, the `connector` field is `None` and the display appears as a standalone group in settings.
