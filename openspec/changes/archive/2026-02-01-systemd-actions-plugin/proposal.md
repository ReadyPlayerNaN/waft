## Why

Users need quick access to common system power and session management actions (lock, logout, shutdown, reboot, suspend) without leaving the main overlay interface. Currently, these actions require external tools or window manager shortcuts, breaking the unified control experience that sacrebleui provides for other system functions.

## What Changes

- Add a new plugin (`features/systemd_actions`) that provides system action controls in the main overlay header
- Create two grouped action buttons in the top-right section of the header:
  - Session Actions: Lock Session, Logout
  - Power Actions: Reboot, Shutdown, Suspend
- Implement slide-down menu pattern for each group button showing available actions
- Integrate with D-Bus `org.freedesktop.login1.Manager` interface for executing system operations
- Ensure only one action menu is open at a time (using existing MenuStore pattern)

## Capabilities

### New Capabilities
- `systemd-actions-widget`: UI component for grouped system action buttons with slide-down menus in the header slot
- `systemd-dbus-integration`: D-Bus client for `org.freedesktop.login1.Manager` to execute lock/logout/shutdown/reboot/suspend operations

### Modified Capabilities
- `reactive-widget-registry`: Header slot layout needs support for right-aligned widgets (currently widgets flow left-aligned only)

## Impact

**New Code:**
- `src/features/systemd_actions/` - New plugin module
  - `mod.rs` - Plugin trait implementation
  - `dbus.rs` - D-Bus client for login1 Manager interface
  - `widget.rs` - Grouped action button widgets
  - `action_menu.rs` - Slide-down menu component for action lists

**Modified Code:**
- `src/ui/main_window.rs` - Header layout may need right-alignment support for action widgets
- `src/plugin.rs` - Potentially add header alignment hints to Widget struct (if right-alignment needed)

**Dependencies:**
- Reuses existing `DbusHandle` infrastructure
- Reuses existing `MenuStore` for menu coordination
- Follows existing patterns: `SliderControlWidget` (expand button), `MenuItemWidget` (action rows), `gtk::Revealer` (slide-down animation)

**D-Bus Interface:**
- Service: `org.freedesktop.login1`
- Object: `/org/freedesktop/login1`
- Methods: `LockSession()`, `Terminate()`, `Reboot()`, `PowerOff()`, `Suspend()`, `Hibernate()`
- Requires PolicyKit authorization for power operations
