# 1. Clickable InfoCard

Add `on_click` to the InfoCard widget. When `on_click` is not `None`, the InfoCard renders as a clickable button an on click it performs the `on_click`. When `on_click` is `None`, the InfoCard renders the way it does now (not a button).

# 2. Clock InfoCard

Migrate the clock code, so it uses InfoCard with optional on click when the click command is set.

# 3. Horizontal layout box

Provide `<Row>` component for the layout. This is a regular `<Box>`, but instead of being vertical, it is `horizontal`. Provide `<Col>` component for the layout. This is a vertical `<Box>`. Regular box is going to be layout neutral.

## Improve default layout

The boxes in Header are going to become Rows

```
<Row>
    <Widget id="clock:main" />
    <Widget id="battery:main" />
    <Widget id="weather:main" />
</Row>
<Row halign="end">
    <Widget id="keyboard-layout:indicator" />
    <Widget id="systemd-actions:*" />
</Row>
```

# 4. Icon Widget

Move the ui-gtk icon from utils to widgets. Inline the tests into the icon.rs file.

# 5. Destroy utils

It is forbidden to use ballast filler words like `utils`, `common`, `helpers` on this project. This leads to creating trash can for ballast code. Extract ui-gtk utils modules to the `src` level. Write it to the instructions file CLAUDE.md never to name anything utils, common, helpers or other ballast words.

# 6. Refactor blueman plugin for readability

The plugin code is all cramped together. `build_widgets` can be split by adapter and have `build_adapter_widget`. Each device row code can be put into `build_device_row_widget`. Split the code into separate file, group it by microdomain and write tests.

# 7. Battery widget InfoCard

Use InfoCard widget in battery plugin instead of MenuRow.

# 8. Fix audio widgets

The audio plugin provides input and output sliders. The input slider widgets work and the output sliders widgets are broken. Output slider widgets look different and they have no menu items.

# 9. StatusCycleButton

Add `StatusCycleButton` widget to the `ui-gtk` and the protocol. It will receive `value: string`, `icon: IconHints`, `options: { label: string, id: string }[]`. It will display the icon and the label of selected option, where `value = options.[x].id`. If `value` is not in options list, it will display label `---`. The `on_click` will send "the next option" as the first argument. If there is no next option after the current selected option, return the first. If there is less than two options, the cycle button is not clickable.

# 10. Keyboard widget StatusCycleButton

Use the `StatusCycleButton` widget instead of `MenuRow`.

# 11. Stateful `on_toggle`

The feature toggles currently have `on_toggle` callback. Problem is that is does not say what is the desired state, so clicking it sends ambiguous requests to the waft bus. Modify `on_toggle` so it sends `activate: bool` as the first argument. This will directly translate the desired user action - "user is clicking disabled feature toggle = he wants to enable it = we are sending `activate=true`" versus "user is clicking enabled feature toggle = he wants to disable it = wa are sending `activate=false`". The plugin daemons will have to handle it, so they do not send duplicate activation requests when the activation is in progress.

# 12. Refactor networkmanager for readability

The code is cramped together. Refactor it in the same way as blueman plugin.

# 13. Migrate sunsetr to daemon architecture

Please migrate the sunsetr to the daemon architecture

# 14. Extract eds-agenda plugin widgets

## ListRow

The AgendaCard widget container can be integrated as ListRow. It will be just the simple container box that receives children.

## IconList

This will be the generic `AttendeeList`. `IconListRow` will receive `icon: IconHints` and `label: string`. `IconList` will receive `IconListRow[]` as children.

## ListButton

This will be generalized button for lists, that can be put into `ListRow`. It will looks just like the MeetingButton from `eds-agenda`.

# 15. Box, Row, Col and Container

The `ui-gtk` provides `Container`. The layout provides `Box`, `Row` and `Col`. Please unify the ui-gtk so it uses `Box`, `Row` and `Col` instead of the `Container`. This should remove the need to pass `orientation` to what is a container at the moment.

# 16. Extract widgets from `primitives.rs`

Each widget should have its own file, despite being primitive.

---

## Syncthing plugin

Provides overlay feature toggle, that enables/disables syncthing

## Notification sounds

Play a sound when a notification pops up
Configure sounds=disabled/enabled
Configure sound based on urgency
Configure sound based on notification matching
Sounds are off in Do Not Disturb mode

## Tethering

Add to networkmanager plugin?

Whenever tethering device is detected, display it as a feature toggle

## Auxiliary notification group splits

Sometimes apps have workspaces. It would be useful to split notifications to groups per app workspace. We should investigate if there is a generic way to achieve this. Good example is Slack. Running multiple workspaces seems to be prefixing the notification title with `[{workspace_name}]` and that could be used to group notifications more productively. The Workspace name (if detected) MUST appear in thenotification group header. Optionally we can even load the workspace icon and display it in the notification group header as a secondary icon to provide more visual hints.

## Plugins to implement

**Needs developer clarification:**

- SNI - What is SNI in this context? Server Name Indication? Social Network Integration? Please specify requirements.

## NetworkManager plugin enhancements

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

## Notification toast bubbles

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

## 8. Notification toast window position

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

## Simplify clock plugin

It does not need external ping
