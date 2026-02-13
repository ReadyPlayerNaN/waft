# Drop nmrs in favor of pure D-Bus for NetworkManager

The `nmrs` crate produces `!Send` futures, which forces the networkmanager plugin to spawn a dedicated OS thread with a single-threaded tokio runtime + `LocalSet` just for WiFi scanning. The rest of the plugin already uses pure `zbus` D-Bus calls. Removing `nmrs` eliminates the threading workaround, simplifies the architecture, and unblocks support for additional device types (tethering, mobile broadband) that nmrs does not expose.

## Migration phases

### Phase 1: Replace device listing with pure D-Bus

- Replace device enumeration with `GetDevices()` on `org.freedesktop.NetworkManager` via zbus and read device properties with the existing `get_property()` helper
- This phase alone unblocks new device types: tethering, mobile broadband, etc.

### Phase 2: Replace WiFi scanning with pure D-Bus

- Replace WiFi scan with `RequestScan` method call on `org.freedesktop.NetworkManager.Device.Wireless`
- Replace network listing with `GetAllAccessPoints` and read each access point's properties
- Remove the dedicated OS thread + single-threaded runtime workaround

### Phase 3: Remove nmrs dependency

- Remove `nmrs = "2.0"` from `plugins/networkmanager/Cargo.toml`
- Plugin depends only on `zbus` for all NetworkManager communication

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
