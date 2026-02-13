# 1. Bluetooth device row

Extract the `BluetoothDeviceRow` component into `waft-ui-gtk/src/bluetooth`. This is the component, that renders bluetooth device menu row.

# 2. Bluetooth device menu row spinner

When the bluetooth device is connecting or disconnecting, it MUST display spinner left of the switch.

# 3. Bluetooth device menu states

When a bluetooth device starts connecting somewhere, the bluetooth daemon is supposed to send status update. This does not happen and UI does not have an opportunity to respond to it.

When a bluetooth device connection fails, the bluetooth daemon is supposed to send status update. This does not happen and UI does not have an opportunity to respond to it.

# 4. VPN menu row

Extract the VPN menu row component into `waft-ui-gtk`. It is going to look very much like the bluetooth device row. The layout is going to be simplified, see below. It is going to have the same behaviour about status, connecting and spinner..

```
<Row>
    <Box>{label}</Box>
    <Box>
        <Spinner />
        <gtk::Switch />
    </Box>
</Row>
```

# 5. Wired menu profiles

The wired adapter menu must provide list of profiles and allow switching the profiles.

# 6. VPN menu

If at least a single VPN is configured, then the VPN feature toggle will be expandable with a menu. The menu will list all available VPN configurations as MenuRows. Clicking the menu row dis/connects the VPN. Clicking the feature toggle toggle disconnects all VPNs.

The VPN feature toggle menu is still not showing up.

# 7. Agenda divider

The agenda must have a divider between today's events and tomorrow's events. The layout should look like this:

```
<>
    <Revealer>
        <PastEvents />
        <Divider />
    </Revealer>
    <Revealer>
        <FutureEventsGrouppedByDay />
    </Revealer>
</>
```

and `FutureEventsGrouppedByDay` should look like this

```
<>
    <Label>{date}
    <Box>
        <Events />
    </Box>
</>
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
