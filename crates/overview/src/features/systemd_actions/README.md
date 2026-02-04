# Systemd Actions Plugin

Quick access to system power and session management actions via the main overlay header.

## Features

This plugin adds two action group buttons to the header:

### Session Actions
- **Lock Session** - Lock the screen
- **Logout** - Terminate the current user session

### Power Actions
- **Reboot** - Restart the system
- **Shutdown** - Power off the system
- **Suspend** - Suspend the system to RAM

## Configuration

Enable the plugin in your `~/.config/waft-overview/config.toml`:

```toml
[[plugins]]
id = "plugin::systemd-actions"
```

No additional configuration is required. The plugin will automatically detect and use systemd-logind.

## Requirements

- **systemd-logind** - The plugin uses D-Bus to communicate with systemd's login1 Manager interface
- **PolicyKit** - Power actions (reboot, shutdown, suspend) require PolicyKit authorization

## Architecture

### D-Bus Integration

The plugin connects to two D-Bus interfaces:

1. **org.freedesktop.login1.Session** - For session-specific actions (lock, logout)
   - Path: `/org/freedesktop/login1/session/{session_id}` or `/org/freedesktop/login1/session/auto`
   - Methods: `Lock()`, `Terminate()`

2. **org.freedesktop.login1.Manager** - For system-wide power operations
   - Path: `/org/freedesktop/login1`
   - Methods: `Reboot(boolean)`, `PowerOff(boolean)`, `Suspend(boolean)`

### UI Components

- **ActionGroupWidget** - Expandable button with menu
  - Main button with icon (non-interactive)
  - Expand button with animated chevron
  - Slide-down menu using `gtk::Revealer`

- **ActionMenuWidget** - Vertical list of action items
  - Uses `MenuItemWidget` for consistent styling
  - System icons for visual clarity

### Widget Positioning

Widgets are registered with high weights (100, 101) to appear on the right side of the header:
- Session actions: weight 100
- Power actions: weight 101

### Menu Coordination

Uses the global `MenuStore` to ensure only one menu is open at a time across all plugins.

## Error Handling

The plugin handles errors gracefully:

- **D-Bus unavailable** - Plugin skips widget creation, logs warning
- **PolicyKit denial** - Shows "Permission Denied" dialog
- **Connection errors** - Shows "System Service Unavailable" dialog

All D-Bus operations are asynchronous and non-blocking to the GTK main thread.

## Files

- `mod.rs` - Plugin trait implementation and widget registration
- `dbus.rs` - D-Bus client for systemd-logind integration
- `action_menu.rs` - Action menu widget (Lock/Logout or Reboot/Shutdown/Suspend)
- `widget.rs` - Action group widget with expandable menu

## Testing

The plugin includes unit tests for session path resolution:

```bash
cargo test systemd_actions
```

Manual testing requires a running systemd-logind service and proper PolicyKit configuration.

## Troubleshooting

### Widgets don't appear
- Check that the plugin is enabled in config
- Verify systemd-logind is running: `systemctl status systemd-logind`
- Check logs for D-Bus connection errors

### Actions fail with "Permission Denied"
- PolicyKit rules may be too restrictive
- Check PolicyKit configuration in `/usr/share/polkit-1/actions/`
- Ensure your user is in appropriate groups (e.g., `wheel`, `sudo`)

### Actions fail with "System Service Unavailable"
- Systemd-logind is not running or not accessible
- D-Bus session bus may be misconfigured
- Check D-Bus service status: `dbus-daemon --session --print-address`

## Security Considerations

- Lock and Logout actions typically don't require elevated privileges
- Power operations (reboot, shutdown, suspend) require PolicyKit authorization
- The `interactive: true` flag allows PolicyKit to prompt for credentials
- No credentials are stored or cached by the plugin
