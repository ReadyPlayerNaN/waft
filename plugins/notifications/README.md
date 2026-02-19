# Notifications Plugin

Freedesktop.org notification server that owns `org.freedesktop.Notifications` on the session D-Bus. Receives notifications from all applications and translates them into entities for the waft daemon.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `notification` | `notifications/notification/{id}` | One entity per active notification |
| `dnd` | `notifications/dnd/default` | Do Not Disturb toggle state |
| `notification-group` | `notifications/notification-group/{id}` | Notification filter group (pattern matcher) |
| `notification-profile` | `notifications/notification-profile/{id}` | Notification filter profile (set of group rules) |
| `active-profile` | `notifications/active-profile/default` | Currently active filter profile |
| `sound-config` | `notifications/sound-config/default` | Sound enabled toggle + per-urgency default sounds |
| `notification-sound` | `notifications/notification-sound/{filename}` | Custom sound file in gallery |

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
| `notification-group` | `create-group` | `{group object}` | Create a new filter group |
| `notification-group` | `update-group` | `{group object}` | Update an existing filter group |
| `notification-group` | `delete-group` | - | Delete a filter group |
| `notification-profile` | `create-profile` | `{profile object}` | Create a new filter profile |
| `notification-profile` | `update-profile` | `{profile object}` | Update an existing filter profile |
| `notification-profile` | `delete-profile` | - | Delete a filter profile |
| `active-profile` | `set-active-profile` | `{"profile_id": "..."}` | Switch the active filter profile |
| `sound-config` | `update-sound-config` | `{sound config object}` | Update sound enabled state and per-urgency defaults |
| `sound-config` | `preview-sound` | `{"reference": "..."}` | Play a sound preview |
| `notification-sound` | `add-sound` | `{"filename": "...", "data": "base64..."}` | Upload a custom sound file |
| `notification-sound` | `remove-sound` | - | Remove a custom sound file |

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
- **Notification filtering**: Pattern-based groups with AND/OR combinators (8 match operators: equals, contains, starts_with, ends_with, regex, not_equals, not_contains, not_regex). Configurable profiles with hide/no_toast/no_sound rules per group.
- **Sound management**: Master toggle, per-urgency default sounds (XDG sound names or custom files), sound gallery with upload/preview/remove. Custom sounds stored in `~/.config/waft/sounds/`.

## Configuration

```toml
[[plugins]]
id = "plugin::notifications"
toast_limit = 3
disable_toasts = false

[plugins.sound]
enabled = true
default_low = "message-new-email"
default_normal = "message-new-instant"
default_critical = "dialog-warning"

[[plugins.groups]]
id = "my-group"
name = "My Filter Group"
combinator = "and"
# ... pattern matchers

[[plugins.profiles]]
id = "my-profile"
name = "My Profile"
# ... group rules (hide/no_toast/no_sound)
```

See `~/.config/waft/config.toml` under `plugin::notifications` for full configuration.

## Lifecycle

`can_stop` returns `false` -- the plugin must remain running to receive D-Bus notifications even when no apps are subscribed.

## Dependencies

- D-Bus session bus (zbus)
