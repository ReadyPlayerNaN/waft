## 1. Module Structure Setup

- [x] 1.1 Create `src/features/systemd_actions/` directory
- [x] 1.2 Create `src/features/systemd_actions/mod.rs` with plugin skeleton
- [x] 1.3 Add `pub mod systemd_actions;` to `src/features/mod.rs`
- [x] 1.4 Register `SystemdActionsPlugin` in `src/main.rs` plugin initialization

## 2. D-Bus Integration

- [x] 2.1 Create `src/features/systemd_actions/dbus.rs`
- [x] 2.2 Implement `SystemAction` enum with variants: `LockSession`, `Terminate`, `Reboot { interactive: bool }`, `PowerOff { interactive: bool }`, `Suspend { interactive: bool }`
- [x] 2.3 Implement `SystemdDbusClient` struct wrapping `Arc<DbusHandle>`
- [x] 2.4 Implement session path resolution (check `XDG_SESSION_ID`, fallback to `/session/auto`)
- [x] 2.5 Implement `SystemdDbusClient::new(dbus: Arc<DbusHandle>) -> Option<Self>` with graceful failure
- [x] 2.6 Implement `execute_action(&self, action: SystemAction) -> Result<()>` with pattern matching
- [x] 2.7 Implement `Lock()` D-Bus call on session path via `org.freedesktop.login1.Session`
- [x] 2.8 Implement `Terminate()` D-Bus call on session path via `org.freedesktop.login1.Session`
- [x] 2.9 Implement `Reboot(boolean)` D-Bus call on `/org/freedesktop/login1` via `org.freedesktop.login1.Manager`
- [x] 2.10 Implement `PowerOff(boolean)` D-Bus call on `/org/freedesktop/login1` via `org.freedesktop.login1.Manager`
- [x] 2.11 Implement `Suspend(boolean)` D-Bus call on `/org/freedesktop/login1` via `org.freedesktop.login1.Manager`
- [x] 2.12 Add error context using `anyhow::Context` for each operation
- [x] 2.13 Add unit tests for session path resolution logic

## 3. Action Menu Widget

- [x] 3.1 Create `src/features/systemd_actions/action_menu.rs`
- [x] 3.2 Implement `ActionMenuWidget` struct with `root: gtk::Box` (vertical)
- [x] 3.3 Implement `ActionMenuOutput` enum with `ActionSelected(SystemAction)` variant
- [x] 3.4 Create menu item creation helper using `MenuItemWidget` with icon and label
- [x] 3.5 Implement session menu with Lock and Logout actions
- [x] 3.6 Implement power menu with Reboot, Shutdown, and Suspend actions
- [x] 3.7 Add CSS classes: `system-action-menu` for container, `system-action-row` for items
- [x] 3.8 Connect click handlers to emit `ActionMenuOutput` events
- [x] 3.9 Use system icons: `system-lock-screen-symbolic`, `system-log-out-symbolic`, `system-reboot-symbolic`, `system-shutdown-symbolic`, `media-playback-pause-symbolic`

## 4. Action Group Widget

- [x] 4.1 Create `src/features/systemd_actions/widget.rs`
- [x] 4.2 Implement `ActionGroupWidget` struct with horizontal layout
- [x] 4.3 Add main button area with icon and optional label
- [x] 4.4 Add expand button with chevron using `MenuChevronWidget`
- [x] 4.5 Add `gtk::Revealer` with `RevealerTransitionType::SlideDown`
- [x] 4.6 Implement `ActionGroupWidget::new()` accepting icon, menu, menu_id, and MenuStore
- [x] 4.7 Connect expand button to emit `MenuOp::OpenMenu(menu_id)` on click
- [x] 4.8 Subscribe to MenuStore updates to sync revealer and chevron state
- [x] 4.9 Implement menu visibility logic: reveal when `active_menu_id == Some(menu_id)`
- [x] 4.10 Add CSS classes: `system-action-group`, `system-action-button`, `expanded` state
- [x] 4.11 Implement `ActionGroupOutput` enum with `ActionTriggered(SystemAction)` variant
- [x] 4.12 Forward `ActionMenuOutput` events to `ActionGroupOutput`

## 5. Plugin Implementation

- [x] 5.1 Implement `Plugin::id()` returning `PluginId::from_static("plugin::systemd-actions")`
- [x] 5.2 Add `dbus_client: Arc<Mutex<Option<SystemdDbusClient>>>` field to plugin struct
- [x] 5.3 Implement `Plugin::init()` to initialize `SystemdDbusClient` with graceful failure
- [x] 5.4 Implement `Plugin::create_elements()` to create two `ActionGroupWidget` instances
- [x] 5.5 Create session action widget with `system-users-symbolic` icon and session menu
- [x] 5.6 Create power action widget with `system-shutdown-symbolic` icon and power menu
- [x] 5.7 Register session widget with ID `systemd-actions:session`, slot `Slot::Header`, weight 100
- [x] 5.8 Register power widget with ID `systemd-actions:power`, slot `Slot::Header`, weight 101
- [x] 5.9 Generate unique menu IDs using `Uuid::new_v4()` or similar
- [x] 5.10 Connect widget output events to D-Bus action execution
- [x] 5.11 Use spawn_on_tokio() to bridge glib/tokio runtime for D-Bus calls
- [x] 5.12 Handle D-Bus errors with user-facing `gtk::MessageDialog`

## 6. Error Handling

- [x] 6.1 Add error dialog helper for displaying D-Bus failures
- [x] 6.2 Distinguish PolicyKit authorization errors from other D-Bus errors
- [x] 6.3 Show "Permission denied" message for PolicyKit denials
- [x] 6.4 Show "System service unavailable" for connection errors
- [x] 6.5 Log all errors with appropriate log levels (warn for degradation, error for failures)

## 7. Testing and Verification

- [x] 7.1 Build project and verify no compilation errors
- [x] 7.2 Run sacrebleui and verify two action buttons appear in header
- [ ] 7.3 Verify session button displays `system-users-symbolic` icon
- [ ] 7.4 Verify power button displays `system-shutdown-symbolic` icon
- [ ] 7.5 Click session expand button and verify menu slides down with Lock/Logout
- [ ] 7.6 Click power expand button and verify menu slides down with Reboot/Shutdown/Suspend
- [ ] 7.7 Verify only one menu opens at a time (MenuStore coordination)
- [ ] 7.8 Click Lock Session action and verify screen locks
- [ ] 7.9 Click Logout action and verify logout confirmation/execution
- [ ] 7.10 Test Reboot action and verify PolicyKit prompt appears (if needed)
- [ ] 7.11 Test Shutdown action and verify PolicyKit prompt appears (if needed)
- [ ] 7.12 Test Suspend action and verify system suspends
- [ ] 7.13 Test error case: Run on system without systemd and verify graceful degradation
- [ ] 7.14 Test error case: Deny PolicyKit prompt and verify error dialog
- [ ] 7.15 Verify widgets appear right-aligned in header (high weight positioning)
- [ ] 7.16 Verify menu animations are smooth (slide-down transition)
- [ ] 7.17 Check CSS styling matches existing UI patterns

## 8. Documentation

- [x] 8.1 Add module-level documentation to `mod.rs` explaining plugin purpose
- [x] 8.2 Document public structs and enums with rustdoc comments
- [x] 8.3 Document D-Bus interface requirements in `dbus.rs`
- [x] 8.4 Add inline comments for PolicyKit interactive flag usage
- [x] 8.5 Create plugin README.md with architecture and troubleshooting

## 9. Cleanup and Polish

- [x] 9.1 Remove any debug print statements
- [x] 9.2 Ensure all imports are used and properly organized
- [x] 9.3 Run `cargo fmt` to format code
- [x] 9.4 Run `cargo clippy` and fix any warnings
- [x] 9.5 Review code for consistency with existing codebase patterns
- [x] 9.6 Verify no unwrap() calls that could panic (use proper error handling)
