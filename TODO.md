# 1. Reconnecting to socket

When `waft` crashes, but the `waft-overview` is still running, it needs to:

- Let user know that the UI is disconnected by disabling all buttons
- Attempt reconnection once per second

# 6. Drop nmrs in favor of pure D-Bus for NetworkManager

The `nmrs` crate produces `!Send` futures, which forces the networkmanager plugin to spawn a dedicated OS thread with a single-threaded tokio runtime + `LocalSet` just for WiFi scanning. The rest of the plugin already uses pure `zbus` D-Bus calls. Removing `nmrs` eliminates the threading workaround, simplifies the architecture, and unblocks support for additional device types (tethering, mobile broadband) that nmrs does not expose.

## Current nmrs usage

### `src/device_discovery.rs` — `discover_devices(nm: &nmrs::NetworkManager)`

- Calls `nm.list_devices()` to enumerate network devices
- Maps `nmrs::DeviceType::{Ethernet, Wifi}` to u32 device type constants
- Maps `nmrs::DeviceState::{Unmanaged, Unavailable, Disconnected, Prepare, Config, Activated, Deactivating, Failed, Other(code)}` to u32 state codes
- Reads `device.interface`, `device.managed`, `device.path`, `device.state`
- **Already has a pure D-Bus alternative:** `get_device_info_dbus()` in the same file reads the same properties via `zbus`

### `src/wifi.rs` — `scan_and_list_known_networks(nm: &nmrs::NetworkManager, conn: &Connection)`

- Calls `nm.scan_networks()` to trigger a WiFi scan
- Calls `nm.list_networks()` to read scan results
- Reads `network.ssid`, `network.strength`, `network.secured`
- This is the **only** function that produces `!Send` futures

### `src/wifi_scan.rs` — `wifi_scan_task(scan_rx, nm: nmrs::NetworkManager, conn, state, notifier)`

- Background task that receives scan requests via channel
- Passes the `nmrs::NetworkManager` instance to `scan_and_list_known_networks()`

### `bin/waft-networkmanager-daemon.rs` — `NetworkManagerPlugin::new()`

- Creates `nmrs::NetworkManager::new()` at startup
- Passes `nm` to `discover_devices()` for initial device enumeration
- Passes `nm` to the WiFi scan thread (lines 949-966)
- **The !Send workaround (lines 949-966):** spawns `std::thread::Builder::new("nm-wifi-scan")` with `tokio::runtime::Builder::new_current_thread()` + `LocalSet::block_on()` solely because nmrs futures are `!Send`

## Migration phases

### Phase 1: Replace device listing with pure D-Bus

- Replace `discover_devices(nm)` with a new function that calls `GetDevices()` on `org.freedesktop.NetworkManager` via zbus and reads device properties with the existing `get_property()` helper (the pattern already exists in `get_device_info_dbus()`)
- Remove `nmrs::NetworkManager` parameter from `NetworkManagerPlugin::new()` return type
- This phase alone unblocks new device types: tethering (DeviceType 10/11), mobile broadband (DeviceType 8), etc.

### Phase 2: Replace WiFi scanning with pure D-Bus

- Replace `nm.scan_networks()` with `RequestScan` method call on `org.freedesktop.NetworkManager.Device.Wireless` interface
- Replace `nm.list_networks()` with `GetAllAccessPoints` method call on the same interface, then read each access point's `Ssid`, `Strength`, `Flags`, `WpaFlags`, `RsnFlags` properties
- Rewrite `scan_and_list_known_networks()` to use only `zbus::Connection`
- Rewrite `wifi_scan_task()` to accept `Connection` instead of `nmrs::NetworkManager`
- Remove the dedicated OS thread + single-threaded runtime workaround in `bin/waft-networkmanager-daemon.rs` — WiFi scanning can run as a normal `tokio::spawn` task on the multi-threaded runtime

### Phase 3: Remove nmrs dependency

- Remove `nmrs = "2.0"` from `plugins/networkmanager/Cargo.toml`
- Verify all `nmrs::` references are gone
- The plugin now depends only on `zbus` for all NetworkManager communication

### Phase 4 (future): Add tethering device type support

- With pure D-Bus device listing, add DeviceType constants for tethering-relevant types (USB=Ethernet with tethering flag, Bluetooth NAP, etc.)
- Expose tethering adapters as `NetworkAdapter` entities with `AdapterKind::Tethering` (or similar)
- Render as a separate feature toggle in the overlay with connect/disconnect actions

## Key benefit

Removing `nmrs` eliminates the dedicated `nm-wifi-scan` OS thread and its single-threaded tokio runtime. All NetworkManager communication runs on the shared multi-threaded tokio runtime via `zbus`, matching every other D-Bus plugin in the project.

# Notification sounds

Play a sound when a notification pops up
Configure sounds=disabled/enabled
Configure sound based on urgency
Configure sound based on notification matching
Sounds are off in Do Not Disturb mode

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
