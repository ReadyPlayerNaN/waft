## Context

The application uses a hybrid runtime: tokio for async I/O and glib for the GTK main loop. The IPC server runs on a separate thread with its own tokio runtime, while plugins run on the main GTK thread.

When the screen locks (via hyprlock on niri), the Wayland compositor stops sending frame callbacks to hidden surfaces. If animations or frame-clock-dependent operations are in progress, the glib main loop blocks waiting for events that never come. The freeze persists after unlock because the compositor doesn't automatically resume event delivery to layer-shell windows in corrupted states.

Existing patterns:
- `Plugin` trait already has `on_overlay_visible(bool)` hook
- `PluginRegistry` has `notify_overlay_visible()` broadcast method
- D-Bus connections exist for both session and system buses
- Notifications plugin manages toast windows separately from main overlay

## Goals / Non-Goals

**Goals:**
- Detect session lock/unlock before compositor starves the event loop
- Pause all frame-dependent operations (animations, countdown timers) during lock
- Hide toast window during lock to prevent frozen visible state
- Resume clean operation on unlock without requiring restart
- Queue notifications during lock for processing after unlock

**Non-Goals:**
- Saving/restoring exact UI state across lock (main overlay always hidden after unlock)
- Supporting lock screen integrations beyond event detection
- Handling compositor crashes or other edge cases

## Decisions

### Decision 1: Use logind D-Bus signals

Subscribe to `Lock`/`Unlock` signals on `org.freedesktop.login1.Session`.

**Rationale:** logind is the standard interface for session management on systemd-based systems. The signals fire before the lock screen fully activates, giving us time to pause gracefully.

**Alternatives considered:**
- Wayland protocol events (wl_surface visibility): Not reliably exposed to GTK, layer-shell complicates this
- Idle detection: Would require polling, doesn't catch explicit lock actions

### Decision 2: Extend Plugin trait with session hooks

Add `on_session_lock()` and `on_session_unlock()` methods to `Plugin` trait with default no-op implementations.

**Rationale:** Mirrors existing `on_overlay_visible()` pattern. Plugins opt-in to session awareness. Main window can also implement handling directly.

**Alternatives considered:**
- Separate trait (SessionAware): More complex, plugins would need to implement two traits
- Event channel to plugins: More decoupled but adds complexity for simple callbacks

### Decision 3: Session monitor as internal component, not a plugin

The session monitor connects to D-Bus and notifies the registry, but is not a user-visible plugin.

**Rationale:** Session monitoring is infrastructure, not a feature users configure. It should always be active if the system supports it. Implemented as a module initialized in `app.rs` before plugins.

**Alternatives considered:**
- Session monitor as plugin: Would require it to be always-enabled, adds config noise
- Inline in app.rs: Would bloat the main app file

### Decision 4: Main window handles lock directly via callback

Rather than making `MainWindowWidget` implement Plugin, pass a callback from the session monitor.

**Rationale:** Main window is not a plugin and shouldn't become one. A simple callback closure keeps concerns separated.

### Decision 5: Notifications plugin hides toast window, not destroys it

On lock, hide the toast window and pause timers. On unlock, show it again and resume.

**Rationale:** Recreating windows is expensive and loses state. The toast window can be hidden and shown efficiently.

### Decision 6: Graceful degradation when logind unavailable

If system bus or logind is unavailable, log a warning and continue without session detection.

**Rationale:** The app should work on non-systemd systems or when D-Bus fails. The freeze issue only occurs in specific compositor configurations.

## Risks / Trade-offs

**[Risk] Lock signal arrives too late** → The signal should fire before hyprlock fully activates, but timing could vary. Mitigation: Pause operations immediately on signal, don't wait for confirmation.

**[Risk] System bus connection fails** → Mitigation: Graceful degradation, app works without session detection.

**[Risk] Other plugins have frame-dependent code** → Mitigation: Extend pattern as needed. Clock plugin's 1-second timer doesn't depend on frame clock, so it should continue working.

**[Trade-off] Main overlay always hidden after unlock** → Simplifies implementation, user just triggers it again via IPC. Alternative (restore previous state) adds complexity for minimal benefit.
