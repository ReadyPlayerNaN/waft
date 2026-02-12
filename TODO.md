# 1. Consistent `waft-ui-gtk`

The widgets in `waft-ui-gtk` are defined as functions. This is difficult to update. Instead, the library must be refactored, so each of the widgets is a struct, that provides update methods.

Some properties must be shared (may be converted to trait):

- `set_css_classes`
- `set_visible`
- ...see if any other apply

# 2. Remove `waft-plugin-api`

It is a leftover of previous architectures. Some packages reuse re-exported things from `waft-ui-gtk` - relink it to `waft-ui-gtk` directly.

# 3. Bluetooth menu menu row

Bluetooth menu should use MenuRow widget. Clicking the row should intiiate connect or disconnect of the device.

# 4. Wired menu info

The wired adapter menu must display the network details, like:

- Local IP address
- Public IP address (if available)

# 5. Wired menu profiles

The wired adapter menu must provide list of profiles and allow switching the profiles.

# 6. VPN menu

If at least a single VPN is configured, then the VPN feature toggle will be displayed with a menu. The menu will list all available VPN configurations as MenuRows. Clicking the menu row dis/connects the VPN. Clicking the feature toggle toggle disconnects all VPNs

# 7. Sunsetr plugin persistence

Sunsetr plugin must always return night light entity even when sunsetr is not running. The only time the sunsetr plugin returns nothing is when the `sunsetr` binary is not found.

# 8. Agenda is empty

For unknown reason, the agenda plugin displays nothing. I definitely have at least one event in one of my calendars for tomorrow and for today.

# 9. D-Bus errors from `nmrs`

This looks like a problem.

```
[2026-02-12T21:53:31Z DEBUG nmrs::core::device] Permanent hardware address not available for device lo: org.freedesktop.DBus.Error.InvalidArgs: No such property “PermHwAddress”
```

# Calendar widget

The EDS plugin must be able to supply events both for agenda widget and for a calendar. The consumers must be able to add a filter to their subscription. For example: Overview is only interested in agenda events (that means today and tomorrow). Calendar widget is going to be interested in entire month of events.

# Syncthing plugin

Provides overlay feature toggle, that enables/pauses user's Syncthing.

# Notification sounds

Play a sound when a notification pops up
Configure sounds=disabled/enabled
Configure sound based on urgency
Configure sound based on notification matching
Sounds are off in Do Not Disturb mode

# Tethering

Add to networkmanager plugin?

Whenever tethering device is detected, display it as a feature toggle

# Auxiliary notification group splits

Sometimes apps have workspaces. It would be useful to split notifications to groups per app workspace. We should investigate if there is a generic way to achieve this. Good example is Slack. Running multiple workspaces seems to be prefixing the notification title with `[{workspace_name}]` and that could be used to group notifications more productively. The Workspace name (if detected) MUST appear in thenotification group header. Optionally we can even load the workspace icon and display it in the notification group header as a secondary icon to provide more visual hints.

# Plugins to implement

**Needs developer clarification:**

- SNI - What is SNI in this context? Server Name Indication? Social Network Integration? Please specify requirements.

# NetworkManager plugin enhancements

### WiFi: Support connecting to new (unsaved) networks with password prompt

**Status:** Requires implementation - significant work needed

**Current limitation:**

- WiFi menu only shows networks with saved connection profiles (`wifi_adapter_widget.rs:214-220`)
- Networks are filtered: `let profiles = dbus::get_connections_for_ssid(&dbus, &ap.ssid).await?;`
- If `profiles.is_empty()`, the network is excluded from the menu

**What needs to be built:**

1. **D-Bus connection creation** (`dbus.rs`):

   - Add `create_wireless_connection()` function to create NM connection profiles dynamically
   - Use D-Bus `AddAndActivateConnection()` method on Settings interface
   - Handle WPA2/WPA3 security types and credentials
   - Current `activate_connection()` (line 390) requires existing connection_path

2. **Password dialog UI** (new file or widget):

   - Create GTK dialog for password entry
   - Support different security types (WPA2-PSK, WPA3-SAE, etc.)
   - Show network name (SSID) in dialog
   - Optional "Save this network" checkbox

3. **WiFi menu updates** (`wifi_menu.rs` and `wifi_adapter_widget.rs`):
   - Show ALL networks (remove filter at line 214-220)
   - Add visual indicator for unsaved vs saved networks (lock icon?)
   - Handle `WiFiMenuOutput::Connect(ssid)` differently for unsaved networks
   - Trigger password dialog when connecting to unsaved network

**Files to modify:**

- `src/features/networkmanager/dbus.rs` - Add D-Bus connection creation
- `src/features/networkmanager/wifi_adapter_widget.rs` - Remove filter, add password dialog logic
- `src/features/networkmanager/wifi_menu.rs` - Add visual indicators for unsaved networks
- New file: `src/features/networkmanager/wifi_password_dialog.rs` (or similar)

**Complexity:** Medium-High (D-Bus API knowledge required, security handling)

# Notification toast bubbles

**Status:** Feature idea - needs design approval

**Concept:** Replace traditional toast notifications with bubble-style notifications like Civilization VI.

**Needs developer input:**

- Visual design mockup or reference
- Animation behavior specification
- Interaction model (click to dismiss, auto-fade, etc.)
- How this integrates with task #8 (positioning)

**Questions to answer:**

- Should all notifications use bubbles, or only certain types?
- Where do bubbles appear (corners, edges, center)?
- How do multiple bubbles stack or cluster?

# Notification toast window position

**Status:** Feature request - can be implemented

**Current:** Notifications appear at top (assumed based on typical behavior)

**Requested:** Support bottom position (and potentially other positions)

**Implementation considerations:**

- Add position configuration (top, bottom, top-left, top-right, bottom-left, bottom-right)
- Fix toast ordering when position changes:
  - Top position: newest on top (stack grows downward)
  - Bottom position: newest on bottom (stack grows upward)
- Update animations to respect position:
  - Slide-in direction should match position
  - Exit animations should feel natural
- Consider interaction with task #7 (bubble style) if both are implemented

**Files to investigate:**

- Notification toast window implementation
- Animation/transition code
- Configuration/settings storage

**Needs developer input:**

- Should this be user-configurable or hardcoded?
- Which positions should be supported initially?

# Simplify clock plugin

It does not need external ping
