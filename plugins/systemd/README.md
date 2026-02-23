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

The URN ID is the unit name without the `.service` suffix (e.g. `systemd/user-service/pipewire` for `pipewire.service`). Only `*.service` units in the `loaded` state are exposed. Timers, sockets, mounts, and other unit types are excluded.

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
  - `ListUnitsByPatterns(["loaded"], ["*.service"])` - List loaded service units
  - `StartUnit` / `StopUnit` - Start/stop services
  - `EnableUnitFiles` / `DisableUnitFiles` - Persist enable/disable across logins
  - Signals: `PropertiesChanged`, `UnitNew`, `UnitRemoved`

Power actions use `interactive=true` to allow polkit authorization prompts. User service operations require no special privileges (session bus, `--user` scope).

## Configuration

```toml
[[plugins]]
id = "plugin::systemd"
```

No plugin-specific configuration options.

## Lifecycle

Session entity is stateless and fixed at startup. User service entities are populated from D-Bus at startup and updated via signal monitoring (PropertiesChanged, UnitNew, UnitRemoved).

## Dependencies

- systemd-logind (D-Bus on system bus via zbus)
- systemd user instance (D-Bus on session bus via zbus)
