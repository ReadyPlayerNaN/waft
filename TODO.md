# 1. Audio slider menus random close

Selecting default input or output device works only once per open menu.

## Reproduction scenario

1. Open audio output menu
2. See output 1 (default) and output 2
3. Click output 2
4. Output 2 is now default as expected
5. Click output 1

Expected: Output 1 is default, menu is still open and visible, the menu chevron is marked as open

Actual: Output 1 is default, menu disappears (as if it had no outputs), the menu chevron is marked as open
Workaround: Close the menu, Re-open the menu. It is now visible.

# 2. Labels not translated

- Do Not Disturb
- Wired
- Caffeine (In Czech should be "Nezamykat", also translate this to english)
- Night Light
- Cloudy (and possibly all other weather conditions)

# 3. Audio device menu row appearence

Extract the `AudioDeviceRow` component into `waft-ui-gtk/src/audio/device_row.rs`.

Re-export the component from bluetooth mod.rs.

The ConnectionIcon is going to show bluetooth icon for bluetooth devices, otherwise nothing.

The layout should be:

```
<Row>
    <Box>
        <DeviceTypeIcon />
        <ConnectionIcon />
    </Box>
    <Box>{label}</Box>
    <Box>
        <CheckMark />
    </Box>
</Row>
```

# 4. Caffeine icon broken

The feature toggle should use an unlocked lock icon

# 5. Syncthing plugin

The plugin should detect if the syncthing is available and configured. It should provide entity `BackupMethod` with name=syncthing.

The overlay UI should display feature toggle "Backup" with menu, that lists all backup methods. The backup method row component is going to be looking like bluetooth device row item and extracted into `waft-ui-gtk/src/backup`. The layout of row item will be:

```
<Row>
    <Box>
        <MethodIcon />
    </Box>
    <Box>{label}</Box>
    <Box>
        <gtk::Switch />
    </Box>
</Row>
```

The Method icon will be either the app icon (syncthing in our case) or some default

# Calendar widget

The EDS plugin must be able to supply events both for agenda widget and for a calendar. The consumers must be able to add a filter to their subscription. For example: Overview is only interested in agenda events (that means today and tomorrow). Calendar widget is going to be interested in entire month of events.

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
