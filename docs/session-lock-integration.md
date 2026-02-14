# Session Lock Integration

Session lock detection allows waft-overview to respond to screen lock/unlock events from the compositor. When the session locks, the overlay hides immediately and plugins are notified to pause work. On unlock, state resets to a clean baseline.

## Architecture

Session lock is an **internal feature** of the overview app, not a daemon plugin. It lives in `crates/overview/src/features/session/` and consists of two files:

- `mod.rs` -- Module root, re-exports `SessionEvent` and `SessionMonitor`
- `dbus.rs` -- D-Bus signal listener implementation

The feature integrates with three consumers:

1. **App shell** (`app.rs`) -- Coordinates the lock response: hides the window, pauses animations, notifies plugins
2. **MainWindowWidget** (`ui/main_window.rs`) -- Provides `on_session_lock()` / `on_session_unlock()` methods for direct window control
3. **PluginRegistry** (`plugin_registry.rs`) -- Broadcasts lock/unlock events to all registered overview plugins via the `Plugin` trait

## SessionMonitor D-Bus Integration

`SessionMonitor` connects to the **system bus** and listens for `Lock` and `Unlock` signals on the `org.freedesktop.login1.Session` interface.

### Session Path Resolution

The monitor determines the current session's D-Bus object path using this strategy:

1. Check `XDG_SESSION_ID` environment variable -- if set, use `/org/freedesktop/login1/session/{id}`
2. Fall back to `/org/freedesktop/login1/session/auto` -- logind resolves this to the caller's session

### Signal Subscription

The monitor adds D-Bus match rules for both signals on the resolved session path:

```
type='signal',interface='org.freedesktop.login1.Session',member='Lock',path='/org/freedesktop/login1/session/...'
type='signal',interface='org.freedesktop.login1.Session',member='Unlock',path='/org/freedesktop/login1/session/...'
```

Each signal gets its own `tokio::spawn`-ed listener task that filters `zbus::MessageStream` messages by interface and member name.

### Graceful Degradation

`SessionMonitor::new()` returns `Option<Self>`. If the system bus is unavailable or the match rule fails, it logs a warning and returns `None`. The app continues without session lock detection.

## Event Flow

The full event propagation path from D-Bus signal to UI response:

```
logind D-Bus signal (system bus)
  -> SessionMonitor listener task (tokio::spawn)
    -> broadcast::Sender<SessionEvent>
      -> bridge task (tokio::spawn, subscribes to broadcast)
        -> async_channel::Sender<SessionEvent>
          -> glib::spawn_future_local receiver loop (GTK main thread)
            -> app.rs handler
```

The bridge between tokio and GTK uses `async_channel` (executor-agnostic) so the GTK main thread can receive events without running tokio futures in glib context.

### SessionEvent Enum

```rust
pub enum SessionEvent {
    Lock,
    Unlock,
}
```

## Lock/Unlock Behavior

### On Lock (app.rs, lines 332-338)

When a `SessionEvent::Lock` arrives on the GTK main thread:

1. **Pause animation** -- `animation.pause()` stops any in-progress show/hide animation immediately
2. **Clear animation flag** -- `animating_hide.set(false)` resets the hide-in-progress state
3. **Hide window** -- `window.set_visible(false)` removes the overlay from screen without animation
4. **Notify plugins** -- `registry.notify_session_locked()` calls `on_session_lock()` on every registered plugin

This is an **immediate** hide with no animation. The compositor stops sending frame events during lock, so animating would be pointless and could leave the window in a partially-visible state.

### On Unlock (app.rs, lines 339-344)

When a `SessionEvent::Unlock` arrives:

1. **Reset animation progress** -- `progress.set(0.0)` returns to fully-hidden baseline
2. **Clear animation flag** -- `animating_hide.set(false)` ensures clean state
3. **Notify plugins** -- `registry.notify_session_unlocked()` calls `on_session_unlock()` on every registered plugin

The window stays hidden after unlock. Users must explicitly show the overlay via IPC command (toggle/show).

### MainWindowWidget Methods

`MainWindowWidget` also exposes `on_session_lock()` and `on_session_unlock()` methods (currently marked `#[allow(dead_code)]` as the app.rs handler manipulates the window directly):

- `on_session_lock()` -- Pauses animation, forces window hidden, logs the event
- `on_session_unlock()` -- Resets animation progress to 0.0, clears animating_hide, keeps window hidden

## Plugin Hook API

The overview `Plugin` trait (not the daemon plugin trait) provides two optional hooks:

```rust
pub trait Plugin {
    /// Called when the session is about to lock (screen locker activating).
    /// Plugins should pause animations and hide any visible windows.
    fn on_session_lock(&self) {}

    /// Called when the session unlocks (screen locker deactivated).
    /// Plugins should resume normal operation.
    fn on_session_unlock(&self) {}
}
```

`PluginRegistry` iterates all registered plugins and calls the appropriate method, using `try_borrow()` to avoid panics if a plugin is currently borrowed for another operation.

### Current Plugin Usage

As of the current codebase, no overview plugins override `on_session_lock()` or `on_session_unlock()`. The hooks exist as extension points for future use (e.g., pausing notification toast timers, stopping clock animation ticks).

## Power Consumption Benefits

The session lock integration reduces resource usage during lock screen:

- **No animation frames** -- Pausing `adw::TimedAnimation` stops frame callbacks
- **Window hidden** -- GTK skips rendering for invisible windows; the compositor reclaims the surface
- **Plugin notification** -- Plugins can stop timers, polling, or D-Bus watches they only need while the overlay is potentially visible

Since the overlay window is force-hidden immediately (no fade-out animation), there is zero GPU work from the overlay during the entire lock period.

## Key Files

| File | Role |
|------|------|
| `crates/overview/src/features/session/mod.rs` | Module root, re-exports |
| `crates/overview/src/features/session/dbus.rs` | SessionMonitor, D-Bus signal listeners |
| `crates/overview/src/app.rs` (lines 159-176, 321-349) | Monitor setup, event bridge, lock/unlock handler |
| `crates/overview/src/ui/main_window.rs` (lines 300-324) | Window-level lock/unlock methods |
| `crates/overview/src/plugin.rs` (lines 170-177) | Plugin trait hooks |
| `crates/overview/src/plugin_registry.rs` (lines 310-343) | Plugin broadcast for lock/unlock |
