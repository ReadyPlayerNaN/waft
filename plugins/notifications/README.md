# Notifications Plugin

Freedesktop.org notification server that owns `org.freedesktop.Notifications` on the session D-Bus. Receives notifications from all applications and translates them into entities for the waft daemon.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `notification` | `notifications/notification/{id}` | One entity per active notification |
| `dnd` | `notifications/dnd/default` | Do Not Disturb toggle state |

### `notification` entity

- `title` - Notification summary
- `description` - Notification body (may contain markup)
- `app_name` / `app_id` - Source application
- `urgency` - Low, Normal, or Critical
- `actions` - Available action buttons (key/label pairs)
- `icon_hints` - Icon sources (themed name, file path, or raw bytes)
- `created_at_ms` - Creation timestamp
- `resident` - Whether the notification persists after action invocation
- `workspace` - Extracted workspace name (Slack multi-workspace support)

### `dnd` entity

- `active` - Whether Do Not Disturb is enabled

## Actions

| Entity Type | Action | Params | Description |
|---|---|---|---|
| `dnd` | `toggle` | - | Toggle Do Not Disturb on/off |
| `notification` | `dismiss` | - | Dismiss a notification (emits `NotificationClosed` signal) |
| `notification` | `invoke-action` | `{"key": "action_key"}` | Invoke a notification action (emits `ActionInvoked` + `NotificationClosed` signals) |

## D-Bus Interfaces

### Owned

- **`org.freedesktop.Notifications`** on session bus at `/org/freedesktop/Notifications`
  - `Notify` - Receive notifications from applications
  - `CloseNotification` - Close a notification by ID
  - `GetCapabilities` - Returns: `actions`, `body`, `body-markup`, `body-hyperlinks`
  - `GetServerInformation` - Returns server name and version
  - Signals: `ActionInvoked`, `NotificationClosed`

### Bus Name Acquisition

Requests the name with `REPLACE_EXISTING` and `ALLOW_REPLACEMENT` flags. If another notification server holds the name and does not allow replacement, the plugin fails to start.

## Features

- **TTL expiration**: Sleep-to-deadline timer removes notifications after their `expire_timeout` elapses (no polling)
- **Notification grouping**: Groups notifications by application identifier
- **Deprioritization**: Category-based and app-name-based rules reduce noise from transient system notifications (screenshot tools, clipboard managers, power/battery apps, software updates)
- **Slack workspace extraction**: Detects `[workspace] title` pattern in Slack notifications and groups per workspace
- **Replacement**: Supports `replaces_id` to update existing notifications
- **Spec version**: Implements freedesktop.org Desktop Notifications Specification 1.2

## Configuration

```toml
[[plugins]]
id = "plugin::notifications"
```

No plugin-specific configuration options.

## Lifecycle

`can_stop` returns `false` -- the plugin must remain running to receive D-Bus notifications even when no apps are subscribed.

## Dependencies

- D-Bus session bus (zbus)
