## 1. Session Monitor Module

- [x] 1.1 Create `src/features/session/mod.rs` module structure
- [x] 1.2 Create `src/features/session/dbus.rs` with logind Session proxy
- [x] 1.3 Implement session path discovery via `$XDG_SESSION_ID` or logind API
- [x] 1.4 Subscribe to `Lock` and `Unlock` signals on the session object
- [x] 1.5 Add graceful degradation when system bus or logind unavailable

## 2. Plugin Trait Extensions

- [x] 2.1 Add `on_session_lock(&self)` method to Plugin trait with default no-op
- [x] 2.2 Add `on_session_unlock(&self)` method to Plugin trait with default no-op

## 3. Plugin Registry Integration

- [x] 3.1 Add `notify_session_locked()` method to PluginRegistry
- [x] 3.2 Add `notify_session_unlocked()` method to PluginRegistry

## 4. Main Window Lock Handling

- [x] 4.1 Add `on_session_lock()` method to MainWindowWidget
- [x] 4.2 Stop animation immediately when lock signal received
- [x] 4.3 Force window to hidden state without animation
- [x] 4.4 Add `on_session_unlock()` method to MainWindowWidget
- [x] 4.5 Reset animation state to initial values (progress = 0.0)

## 5. App Integration

- [x] 5.1 Initialize session monitor in `app.rs` after D-Bus connection
- [x] 5.2 Wire session monitor to PluginRegistry broadcast methods
- [x] 5.3 Wire session monitor to MainWindowWidget lock/unlock methods
- [x] 5.4 Register session module in `src/features/mod.rs`

## 6. Notifications Plugin Lock Handling

- [x] 6.1 Add `session_locked` flag to NotificationsPlugin state
- [x] 6.2 Implement `on_session_lock()` to hide toast window and pause timers
- [x] 6.3 Implement `on_session_unlock()` to resume toast processing
- [x] 6.4 Skip toast rendering when `session_locked` is true
- [x] 6.5 Process queued notifications on unlock
