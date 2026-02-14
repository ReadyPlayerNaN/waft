# Notification sounds

Play a sound when a notification pops up. Configure sounds=disabled/enabled, sound based on urgency, sound based on notification matching. Sounds are off in Do Not Disturb mode.

# Auxiliary notification group splits

Sometimes apps have workspaces. It would be useful to split notifications to groups per app workspace. Good example is Slack: running multiple workspaces seems to prefix the notification title with `[{workspace_name}]` and that could be used for grouping. The workspace name (if detected) should appear in the notification group header.

See `docs/notification-grouping-research.md` for research.

# WiFi: Support connecting to new (unsaved) networks

Currently WiFi only shows networks with saved connection profiles. Connecting to new networks requires a password prompt flow using `AddAndActivateConnection()` on the NetworkManager D-Bus Settings interface.

# Notification toast bubbles (PARTIALLY DONE)

Basic toast application (`waft-toasts`) is now implemented with:
- ✅ 3-toast limit with queue overflow
- ✅ DND awareness (critical notifications bypass)
- ✅ 5-second TTL with automatic expiry
- ✅ Interactive (left-click action, right-click dismiss)
- ✅ Top-center positioning on Layer::Top

Still TODO:
- [ ] Bubble-style visual design (currently uses card style)
- [ ] Configurable toast limit and timeout
- [ ] Per-app toast filtering

# Notification toast position

Support configurable toast position (top, bottom, corners). Fix toast ordering to match position (newest-on-top vs newest-on-bottom).

Current implementation: Fixed top-center position.

# SNI (Status Notifier Items)

Systray compatibility for applications that use the StatusNotifierItem protocol.

# Rename blueman plugin to bluez

The `blueman` plugin is misleadingly named - it talks directly to BlueZ via D-Bus, not to the Blueman application. Rename the plugin from `blueman` to `bluez` to accurately reflect what it does.

This involves:

- Renaming `plugins/blueman/` directory to `plugins/bluez/`
- Renaming binary from `waft-blueman-daemon` to `waft-bluez-daemon`
- Updating plugin name in `PluginRuntime::new("blueman", ...)` to `"bluez"`
- Updating URN prefixes from `blueman/` to `bluez/`
- Updating all documentation references
- Updating Cargo.toml workspace members
