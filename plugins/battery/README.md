# Battery Plugin

Battery status display from UPower.

## Purpose

Provides battery percentage, charging state, and estimated time remaining by reading the UPower DisplayDevice over the system D-Bus. Updates are pushed instantly via D-Bus PropertiesChanged signals -- no polling.

## Entity Types

### `battery`

Single entity representing the composite battery state (UPower DisplayDevice).

| Field | Type | Description |
|-------|------|-------------|
| `present` | `bool` | Whether a battery is present |
| `percentage` | `f64` | Charge level, 0.0 to 100.0 |
| `state` | `BatteryState` | `Unknown`, `Charging`, `Discharging`, `Empty`, `FullyCharged`, `PendingCharge`, `PendingDischarge` |
| `icon_name` | `String` | UPower-provided icon name (e.g. `battery-level-80-charging-symbolic`) |
| `time_to_empty` | `i64` | Seconds until empty (when discharging) |
| `time_to_full` | `i64` | Seconds until full (when charging) |

### Actions

None. Battery is display-only.

### URN Format

```
battery/battery/BAT0
```

## D-Bus Interfaces

| Bus | Destination | Path | Interface | Usage |
|-----|-------------|------|-----------|-------|
| System | `org.freedesktop.UPower` | `/org/freedesktop/UPower/devices/DisplayDevice` | `org.freedesktop.UPower.Device` | Read battery properties |
| System | `org.freedesktop.DBus` | `/org/freedesktop/UPower/devices/DisplayDevice` | `org.freedesktop.DBus.Properties` | PropertiesChanged signals |

## Dependencies

- **UPower** -- system battery monitoring service (usually pre-installed on Linux desktops)

## Configuration

```toml
[[plugins]]
id = "battery"
```

No plugin-specific configuration options. Returns no entities if no battery is present.
