# Systemd Actions Plugin

System power and session management via systemd-logind D-Bus interface. Provides session information (user name, display) and handles lock, logout, reboot, shutdown, and suspend actions.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `session` | `systemd-actions/session/default` | Current session information |

### `session` entity

- `user_name` - Current user's login name (from `$USER` or `$LOGNAME`)
- `screen_name` - Current display (from `$WAYLAND_DISPLAY` or `$DISPLAY`)

## Actions

| Action | D-Bus Method | Description |
|---|---|---|
| `lock` | `Session.Lock()` | Lock the current session |
| `logout` | `Session.Terminate()` | Terminate the current session |
| `reboot` | `Manager.Reboot(interactive=true)` | Reboot the system |
| `shutdown` | `Manager.PowerOff(interactive=true)` | Power off the system |
| `suspend` | `Manager.Suspend(interactive=true)` | Suspend the system |

## D-Bus Interfaces

### Used (system bus)

- **`org.freedesktop.login1.Session`** - Session-scoped actions (lock, terminate)
  - Object path: `/org/freedesktop/login1/session/{XDG_SESSION_ID}` or `/org/freedesktop/login1/session/auto`
- **`org.freedesktop.login1.Manager`** - System-wide power actions (reboot, shutdown, suspend)
  - Object path: `/org/freedesktop/login1`

Power actions use `interactive=true` to allow polkit authorization prompts.

## Configuration

```toml
[[plugins]]
id = "systemd-actions"
```

No plugin-specific configuration options.

## Lifecycle

Stateless plugin with no background tasks. The session entity is fixed at startup. All actions dispatch D-Bus calls directly when invoked.

## Dependencies

- systemd-logind (D-Bus on system bus via zbus)
