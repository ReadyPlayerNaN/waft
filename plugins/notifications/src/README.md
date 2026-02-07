# Notifications Plugin

Provides desktop notification handling compliant with the [Desktop Notifications Specification](https://specifications.freedesktop.org/notification-spec/latest/). Acts as a notification daemon, displaying toast notifications and a notification panel.

## Plugin ID

```
plugin::notifications
```

## Configuration

```toml
[[plugins]]
id = "plugin::notifications"
toast_limit = 3
disable_toasts = false
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `toast_limit` | integer | `3` | Maximum number of toast notifications displayed at once. Minimum value is 1. |
| `disable_toasts` | boolean | `false` | When `true`, disables popup toast notifications entirely. Notifications will still appear in the panel. |

## Features

- **Toast notifications**: Popup notifications that appear briefly on screen
- **Notification panel**: Persistent list of notifications in the overlay
- **Do Not Disturb mode**: Suppresses non-critical toasts (critical and resident notifications still appear)
- **Notification grouping**: Groups notifications by application
- **Action buttons**: Supports notification action buttons
- **TTL expiration**: Notifications auto-dismiss based on their expire timeout
- **Replacement support**: Notifications with `replaces_id` replace existing notifications

## Requirements

This plugin registers as a notification daemon on DBus. Only one notification daemon can run at a time, so you may need to disable other notification daemons (like mako, dunst, or your desktop environment's built-in notifications).

## Notification Behavior

### Toast TTL (Time-to-Live)

- **Critical urgency**: Never auto-dismiss
- **Normal urgency**: 10 seconds (or explicit timeout from notification)
- **Low urgency**: 5 seconds (or explicit timeout from notification)
- **Resident notifications**: Never auto-dismiss

### Do Not Disturb Mode

When DnD is enabled:
- Critical notifications still show as toasts
- Resident notifications still show as toasts
- All other notifications are suppressed from toasts but still appear in the panel
