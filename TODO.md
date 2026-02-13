# Tethering support in networkmanager

When I connect my phone to the pc over USB or Bluetooth, the network manager should provide this as a tethering NetworkAdapter. The overlay should render these as a separate feature toggle with similar logic to Wi-Fi, just isolated. Clicking on a connection row in tethering feature toggle menu will connect or disconnect it.

# Notification sounds

Play a sound when a notification pops up. Configure sounds=disabled/enabled, sound based on urgency, sound based on notification matching. Sounds are off in Do Not Disturb mode.

# Auxiliary notification group splits

Sometimes apps have workspaces. It would be useful to split notifications to groups per app workspace. Good example is Slack: running multiple workspaces seems to prefix the notification title with `[{workspace_name}]` and that could be used for grouping. The workspace name (if detected) should appear in the notification group header.

See `docs/notification-grouping-research.md` for research.

# WiFi: Support connecting to new (unsaved) networks

Currently WiFi only shows networks with saved connection profiles. Connecting to new networks requires a password prompt flow using `AddAndActivateConnection()` on the NetworkManager D-Bus Settings interface.

# Notification toast bubbles

Replace traditional toast notifications with bubble-style notifications. Needs design work.

# Notification toast position

Support configurable toast position (top, bottom, corners). Fix toast ordering to match position (newest-on-top vs newest-on-bottom).

# SNI (Status Notifier Items)

Systray compatibility for applications that use the StatusNotifierItem protocol.
