# Systemd Plugin

System power, session management, and user service monitoring via systemd D-Bus interfaces. Provides session information (user name, display), handles lock, logout, reboot, shutdown, and suspend actions, and exposes user-level systemd services.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `session` | `systemd/session/default` | Current session information |
| `user-service` | `systemd/user-service/{unit-name}` | A user-level systemd service |

### `session` entity

- `user_name` - Current user's login name (from `$USER` or `$LOGNAME`)
- `screen_name` - Current display (from `$WAYLAND_DISPLAY` or `$DISPLAY`)

### `user-service` entity

- `unit` - Full unit name (e.g. `pipewire.service`)
- `description` - Human-readable description from unit file
- `active_state` - Current state: `active`, `inactive`, `activating`, `deactivating`, or `failed`
- `enabled` - Whether the unit starts on login (`true` for `enabled` and `enabled-runtime` unit file states, `false` otherwise)
- `sub_state` - Detailed sub-state (e.g. `running`, `dead`, `exited`)

The URN ID is the unit name without the `.service` suffix (e.g. `systemd/user-service/pipewire` for `pipewire.service`). All installed `*.service` units are exposed, including disabled services that are not currently loaded. Template units (names containing `@`) are excluded. Timers, sockets, mounts, and other unit types are excluded.

Static services (those without an `[Install]` section) cannot be enabled or disabled.

## Actions

### Session actions

| Action | D-Bus Method | Description |
|---|---|---|
| `lock` | `Session.Lock()` | Lock the current session |
| `logout` | `Session.Terminate()` | Terminate the current session |
| `reboot` | `Manager.Reboot(interactive=true)` | Reboot the system |
| `shutdown` | `Manager.PowerOff(interactive=true)` | Power off the system |
| `suspend` | `Manager.Suspend(interactive=true)` | Suspend the system |

### User service actions

| Action | D-Bus Method | Description |
|---|---|---|
| `start` | `Manager.StartUnit(unit, "replace")` | Start the service |
| `stop` | `Manager.StopUnit(unit, "replace")` | Stop the service |
| `enable` | `Manager.EnableUnitFiles([unit], false, true)` | Enable the service on login |
| `disable` | `Manager.DisableUnitFiles([unit], false)` | Disable the service on login |

## D-Bus Interfaces

### Used (system bus)

- **`org.freedesktop.login1.Session`** - Session-scoped actions (lock, terminate)
  - Object path: `/org/freedesktop/login1/session/{XDG_SESSION_ID}` or `/org/freedesktop/login1/session/auto`
- **`org.freedesktop.login1.Manager`** - System-wide power actions (reboot, shutdown, suspend)
  - Object path: `/org/freedesktop/login1`

### Used (session bus)

- **`org.freedesktop.systemd1.Manager`** - User service listing, monitoring, and control
  - Object path: `/org/freedesktop/systemd1`
  - `ListUnitsByPatterns(["loaded"], ["*.service"])` - List loaded service units at startup
  - `ListUnitFilesByPatterns([], ["*.service"])` - Discover all installed services (including disabled/unloaded) at startup
  - `GetUnitFileState(unit)` - Query enabled/disabled state for any installed unit (works even when the unit is not loaded)
  - `StartUnit` / `StopUnit` - Start/stop services
  - `EnableUnitFiles` / `DisableUnitFiles` - Persist enable/disable across logins
  - Signals: `PropertiesChanged`, `UnitNew`, `UnitRemoved`, `UnitFilesChanged`

Power actions use `interactive=true` to allow polkit authorization prompts. User service operations require no special privileges (session bus, `--user` scope).

## Configuration

```toml
[[plugins]]
id = "plugin::systemd"
```

No plugin-specific configuration options.

## Lifecycle

Session entity is stateless and fixed at startup.

`user-service` and `user-timer` monitoring is **lazy**:

- If nothing is subscribed to `user-service`, the plugin does not keep a live service snapshot or listen for service-unit changes.
- If nothing is subscribed to `user-timer`, the plugin does not keep a live timer snapshot, listen for timer-unit changes, or watch `~/.config/systemd/user/`.
- When the first subscriber appears for either entity type, the plugin takes an initial snapshot and starts only the monitoring needed for that entity type.
- When the subscriber count returns to zero, the plugin stops that monitoring and drops the cached entities.

When `user-service` has subscribers, entities are populated from two D-Bus sources merged together:

1. `ListUnitsByPatterns(["loaded"], ["*.service"])` provides runtime state (description, active_state, sub_state) for currently loaded units.
2. `ListUnitFilesByPatterns([], ["*.service"])` discovers all installed unit files, adding disabled/unloaded services with `active_state: "inactive"`, `sub_state: "dead"`. If this call fails (e.g. older systemd), the plugin falls back to loaded units only.

While `user-service` has subscribers, the plugin monitors four D-Bus signals on the session bus:

- **`PropertiesChanged`** -- Updates `active_state` and `sub_state` when a service transitions between states (e.g. starting, stopping, failing).
- **`UnitNew`** -- Adds newly loaded services to the internal map with full property lookups.
- **`UnitRemoved`** -- Updates the service to `active_state: "inactive"`, `sub_state: "dead"` in-place. The service is never removed from the map because `UnitRemoved` means systemd unloaded the unit from its runtime, not that the unit file was deleted from disk.
- **`UnitFilesChanged`** -- Re-queries `GetUnitFileState` for all tracked services and updates their `enabled` field. This signal fires on enable, disable, mask, and unmask operations.

While `user-timer` has subscribers, the plugin keeps timer entities fresh by:

- listening for relevant systemd manager / unit signals on the session bus, and
- watching `~/.config/systemd/user/` for `.timer` / `.service` file changes.

This replaces the previous fixed 5-second timer polling loop.

The `refresh_service()` method (called after handle_action) also updates in-place when `GetUnit` fails for an unloaded unit: it sets `active_state: "inactive"`, `sub_state: "dead"`, and queries `GetUnitFileState` for the correct `enabled` value. Services are never removed from the map on transient D-Bus failures.

## Dependencies

- systemd-logind (D-Bus on system bus via zbus)
- systemd user instance (D-Bus on session bus via zbus)
