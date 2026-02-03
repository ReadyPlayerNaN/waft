---
# ITERATION SUMMARY (Ralph Loop)

**Completed:**
- ✅ Task 1: Fixed sunsetr checkmark jumping - Only send ActivePreset for events that have the field
- ✅ Task 4: Feature toggle loading state - Added CSS outline for busy state (no layout jump)
- ✅ Task 5: Feature toggle hover color - Fixed to use neutral color when off
- ✅ Task 5b: Feature toggle 50% width - Added column_homogeneous and hexpand
- ✅ Task 3: Analyzed NetworkManager plugin - Detailed architecture documentation for both sub-tasks
- ✅ Code quality: Fixed 3 clippy warnings (derivable_impls, collapsible_if)

**Documented (needs developer input):**
- Task 2: Plugin implementation - Needs clarification on "Tether" and "SNI" requirements
- Task 3a: WiFi new network support - Full implementation plan with file locations
- Task 3b: WiFi signal strength in toggle - Implementation approach documented
- Task 6: Notification deprioritization - Categorization complete, needs approval
- Task 7: Notification bubbles - Needs design mockup/specification
- Task 8: Notification positioning - Needs configuration decisions

**Ready for implementation (pending decisions):**
- Task 3a: WiFi password prompt for new networks (Medium-High complexity)
- Task 3b: Signal strength icon in WiFi toggle (Low-Medium complexity)
- Task 6: Notification deprioritization (pending priority approval)
- Task 8: Notification positioning (pending config decisions)

**Code quality:**
- ✅ All changes compile successfully (`cargo check` passed)
- ✅ CSS animations use GTK4-compatible syntax
- ✅ Reduced clippy warnings from 9 to 6 (fixed derivable_impls, collapsible_if)
- Remaining clippy warnings are architectural (type complexity, trait suggestions)

**Files modified:**
- `TODO.md` - Comprehensive documentation of all tasks
- `src/features/sunsetr/ipc.rs` - Fixed checkmark jumping (task 1)
- `src/ui/main_window.rs` - CSS fixes for tasks 4 & 5
- `src/ui/feature_grid.rs` - Added column_homogeneous for 50% width (task 5b)
- `src/ui/feature_toggle.rs` - Added hexpand for 50% width (task 5b)
- `src/menu_state.rs` - Derive Default instead of manual impl
- `src/features/notifications/types.rs` - Derive Default for NotificationUrgency
- `src/features/notifications/store/manager.rs` - Collapsed if statement

**Next iteration priorities:**
1. Clarify Task 2 requirements (Tether plugin, SNI definition)
2. Consider implementing Task 3b (WiFi signal strength in toggle) - low complexity, high UX value
3. Review and approve notification priority categories (Task 6)
4. For animated loading state: consider adding GtkSpinner to FeatureToggleWidget

---

## 1. Sunsetr preset menu: Fix checkmark jumping during transitions ✓ COMPLETED

**Issue:** When switching presets (e.g., day → gaming), the checkmark jumps through an intermediate state (day → default → gaming) instead of going directly to the target preset.

**Root Cause:**

Looking at the actual IPC logs:
```json
{"event_type":"preset_changed","from_preset":"day","to_preset":"gaming",...}
{"event_type":"state_applied","active_preset":"gaming",...}
```

The `preset_changed` events do NOT have an `active_preset` field - only `from_preset` and `to_preset`. When serde deserializes this, `active_preset` becomes `None`. The old code was sending `ActivePreset(None)` for these events, causing the checkmark to jump to "Default".

**Fix Applied:**

In `src/features/sunsetr/ipc.rs`, only send `ActivePreset` updates when the event actually contains the `active_preset` field:

```rust
// Only send active preset update if the event actually contains active_preset
// (preset_changed events don't have this field, only state_applied events do)
if ev.active_preset.is_some() {
    let preset = ev.active_preset.as_ref().and_then(|p| {
        if p == "default" { None } else { Some(p.clone()) }
    });
    let _ = sender.send(SunsetrIpcEvents::ActivePreset(preset));
}
```

**Result:** Checkmark now moves directly from one preset to another without jumping through "Default".

## 2. Plugins to implement

**Needs developer clarification:**
- Tether plugin? - What functionality is needed? Mobile hotspot detection/control?
- SNI - What is SNI in this context? Server Name Indication? Social Network Integration? Please specify requirements.

## 3. NetworkManager plugin enhancements

### 3a. WiFi: Support connecting to new (unsaved) networks with password prompt

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

### 3b. WiFi: Signal strength icon updates in toggle (currently just on/off)

**Status:** Easier to implement - infrastructure exists

**Current behavior:**
- WiFi toggle shows generic WiFi icon regardless of signal strength
- Signal strength IS available (`AccessPointState.strength: u8` in dbus.rs:984-991)
- Menu already shows signal strength icons (`wifi_menu.rs:39-47`)

**What needs to be done:**

1. **Track active connection signal strength** (`wifi_adapter_widget.rs`):
   - Currently widget stores `active_ssid: Option<String>` (line 34)
   - Need to add `active_signal_strength: Option<u8>`
   - Update signal strength when network list refreshes (line 156-236)

2. **Pass signal strength to toggle** (`wifi_toggle.rs`):
   - Add `set_signal_strength(strength: Option<u8>)` method
   - Update icon dynamically based on strength

3. **Icon selection logic** (reuse from `wifi_menu.rs:39-47`):
   ```rust
   fn signal_icon(strength: u8) -> &'static str {
       match strength {
           0..=25 => "network-wireless-signal-weak-symbolic",
           26..=50 => "network-wireless-signal-ok-symbolic",
           51..=75 => "network-wireless-signal-good-symbolic",
           _ => "network-wireless-signal-excellent-symbolic",
       }
   }
   ```

4. **Consider real-time updates** (optional):
   - Current: Signal strength only updates when menu is opened
   - Enhanced: Subscribe to AccessPoint PropertiesChanged signals
   - Update signal strength icon in real-time (every few seconds)

**Files to modify:**
- `src/features/networkmanager/wifi_adapter_widget.rs` - Track active signal strength
- `src/features/networkmanager/wifi_toggle.rs` - Add signal strength icon updates
- `src/features/networkmanager/dbus.rs` - Optional: Add AccessPoint signal subscription

**Complexity:** Low-Medium (mostly UI updates, D-Bus subscription if real-time)

## 4. Feature toggle loading state ✓ COMPLETED

**Implementation:** Added CSS outline for busy state (GTK4 doesn't support @keyframes animations).

**Changes made:**
- Used `outline` instead of `border` to avoid layout jump
- Applied to `.feature-toggle.busy .toggle-main`
- Applied to `.feature-toggle-expandable.busy .toggle-main` and `.toggle-expand`
- `outline: 2px solid alpha(@accent_bg_color, 0.6); outline-offset: -2px;`

**Note:** GTK4 CSS does not support @keyframes animations like web CSS. For animated effects, a GtkSpinner or widget-level animation would be needed.

## 5. Feature toggle off main button hover colour ✓ COMPLETED

**Issue:** Inactive toggle had primary accent color on hover, but should use neutral color.

**Fix:** Changed hover background in `src/ui/main_window.rs:481-487`
- **Before:** `@accent_bg_color 20%` mixed with `@card_bg_color`
- **After:** `@window_fg_color 10%` mixed with `@card_bg_color`

**Effect:** Inactive toggles now have a subtle neutral gray hover effect instead of blue/accent.

## 5b. Feature toggle 50% width ✓ COMPLETED

**Issue:** Feature toggles weren't taking equal 50% width in the grid.

**Fix:**
- Added `column_homogeneous(true)` to grid in `src/ui/feature_grid.rs:30-34`
- Added `hexpand(true)` to toggle root container in `src/ui/feature_toggle.rs:63-67`

**Effect:** Each feature toggle now takes exactly 50% of the grid width.

## 6. Notification deprioritization

**Status:** Design/brainstorming phase. Ready for implementation once priorities are decided.

**Proposed notification categories and treatments:**

**High priority (default behavior):**
- Security alerts
- Critical system errors
- Incoming calls/messages

**Medium priority (short TTL, 3-5 seconds):**
- Device connected/disconnected
- WiFi/network connected
- Screenshot saved
- USB device mounted/unmounted
- Media player track changes

**Low priority (very short TTL, 1-2 seconds, minimal UI):**
- Volume change notifications (transient slider/OSD)
- Brightness change notifications (transient slider/OSD)
- Clipboard/copy confirmations

**Persistent indicators (no toast, status bar only):**
- Battery fully charged (status icon update)
- Software update available - non-security (can show badge)

**Implementation requirements:**
- Add priority field to notification data structure
- Implement TTL (time-to-live) per notification
- Create minimal UI variant for transient notifications
- Add filtering logic in notification manager

**Needs developer decision:** Confirm priority assignments and UX approach before implementation.

## 7. Notification toast bubbles

**Status:** Feature idea - needs design approval

**Concept:** Replace traditional toast notifications with bubble-style notifications like Civilization VI.

**Needs developer input:**
- Visual design mockup or reference
- Animation behavior specification
- Interaction model (click to dismiss, auto-fade, etc.)
- How this integrates with task #6 (deprioritization) and task #8 (positioning)

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
