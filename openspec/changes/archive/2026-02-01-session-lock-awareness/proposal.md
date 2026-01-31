## Why

The GTK UI freezes when the screen is locked (via hyprlock on niri compositor). The glib main loop blocks completely because the Wayland compositor stops sending frame callbacks to hidden surfaces, and animations/timers waiting for these callbacks starve the event loop. The frozen state persists after unlock, requiring a process restart.

## What Changes

- Add logind D-Bus integration to detect session lock/unlock events
- Pause all rendering, animations, and timers when session locks
- Queue incoming notifications during lock instead of rendering toasts
- Resume normal operation on unlock with clean state (main overlay hidden, toasts visible)
- Extend Plugin trait with optional `on_session_lock`/`on_session_unlock` lifecycle hooks

## Capabilities

### New Capabilities

- `session-lock-detection`: Detect session lock/unlock via logind D-Bus signals and broadcast to plugins

### Modified Capabilities

- `notifications`: Pause toast rendering during lock, queue notifications, resume on unlock

## Impact

- **New files**: `src/features/session/` module (plugin + D-Bus proxy)
- **Modified files**:
  - `src/plugin.rs` - add lifecycle hooks
  - `src/plugin_registry.rs` - add broadcast methods
  - `src/app.rs` - register session plugin, wire to main window
  - `src/features/notifications/mod.rs` - implement lock/unlock handlers
  - `src/ui/main_window.rs` - add lock/unlock handling
- **Dependencies**: Uses existing zbus/D-Bus infrastructure, connects to system bus for logind
