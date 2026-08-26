# Power Plugin

Battery status display and power profile management.

## Purpose

Provides the existing `battery` entity from the UPower DisplayDevice and a `power-profile` entity from `power-profiles-daemon`. Updates are pushed instantly from D-Bus property changes -- no polling.

## Entity Types

### `battery`

Single entity representing composite battery state.

| Field | Type | Description |
|-------|------|-------------|
| `present` | `bool` | Whether a battery is present |
| `percentage` | `f64` | Charge level, 0.0 to 100.0 |
| `state` | `BatteryState` | `Unknown`, `Charging`, `Discharging`, `Empty`, `FullyCharged`, `PendingCharge`, `PendingDischarge` |
| `icon_name` | `String` | UPower-provided icon name |
| `time_to_empty` | `i64` | Seconds until empty |
| `time_to_full` | `i64` | Seconds until full |

URN:

```text
power/battery/BAT0
```

### `power-profile`

Singleton entity representing system power profile state.

| Field | Type | Description |
|-------|------|-------------|
| `active_profile` | `String` | Active backend-native profile name |
| `profiles` | `Vec<String>` | Available backend-native profile names |
| `performance_degraded` | `Option<String>` | Optional degraded-performance reason |

URN:

```text
power/power-profile/default
```

### Actions

#### `set-profile`

```json
{ "profile": "balanced" }
```

Uses backend-native values such as `power-saver`, `balanced`, and `performance`. UI labels are localized separately.

## D-Bus Interfaces

| Bus | Destination | Path | Interface | Usage |
|-----|-------------|------|-----------|-------|
| System | `org.freedesktop.UPower` | `/org/freedesktop/UPower/devices/DisplayDevice` | `org.freedesktop.UPower.Device` | Read battery properties |
| System | `org.freedesktop.UPower.PowerProfiles` | `/org/freedesktop/UPower/PowerProfiles` | `org.freedesktop.UPower.PowerProfiles` | Read and set power profiles |
| System | `org.freedesktop.DBus.Properties` | backend paths above | `org.freedesktop.DBus.Properties` | Receive `PropertiesChanged` signals |

## Dependencies

- **UPower** -- battery monitoring
- **power-profiles-daemon** -- power profile management

## Configuration

```toml
[[plugins]]
id = "power"
```

One-release compatibility alias:

```toml
[[plugins]]
id = "battery"
```

No plugin-specific configuration options. Battery and power profile entities are emitted independently based on backend availability.
