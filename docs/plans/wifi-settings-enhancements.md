# WiFi Settings Enhancements -- Implementation Plan

## Current State Summary

**Protocol** (`crates/protocol/src/entity/network.rs`):
- `WiFiNetwork` entity has fields: `ssid`, `strength`, `secure`, `known`, `connected`, `security_type`, `connecting`
- The registry (`crates/protocol/src/entity/registry.rs`) already declares three actions: `connect`, `disconnect`, and `forget`. However, only `connect` and `disconnect` are implemented in the plugin.

**Plugin** (`plugins/networkmanager/bin/waft-networkmanager-daemon.rs`):
- `handle_wifi_network_action` only handles `connect` and `disconnect`. There is no `forget` handler.
- The plugin uses raw D-Bus calls to NM. It already has `get_connections_for_ssid()` which returns saved connection profile D-Bus paths.
- No mechanism exists to read or modify per-network connection settings (autoconnect, metered, DNS, IP config).
- No mechanism to read WiFi passwords from NM (which requires `GetSecrets` D-Bus call).

**Settings UI** (`crates/settings/src/wifi/`):
- WiFi page is a "smart container" that does NOT receive `navigation_view` -- it cannot push sub-pages.
- `NetworkRow` is a VDOM-based dumb widget with Connect/Disconnect button only. No forget button, no navigation chevron.
- `KnownNetworksGroup` reconciles known networks and wires Connect/Disconnect actions.
- No per-network detail/settings sub-page exists.

**Reference patterns in the codebase:**
- Bluetooth `DeviceRow` shows how to add a Remove/trash button to a row (uses `DeviceRowOutput::Remove`).
- Online Accounts page shows the pattern for: (1) accepting `navigation_view`, (2) creating `SettingsSubPage` per entity, (3) confirmation dialog for destructive actions, (4) wiring detail page output events to action callbacks.
- Appearance page shows sub-page navigation pattern.

---

## Feature 1: Forget a Known Network

### Protocol changes
None needed. The `forget` action is already declared in the registry.

### Plugin changes (`plugins/networkmanager/`)

1. Add `"forget"` arm to `handle_wifi_network_action`. Implementation:
   - Call `get_connections_for_ssid(&self.conn, ssid)` to find saved connection profile D-Bus paths.
   - For each matching connection path, create a proxy to `org.freedesktop.NetworkManager.Settings.Connection` and call `Delete`.
   - If the network is currently connected, disconnect first (call `disconnect_device`).
   - Update state: remove the access point's `known` flag.
   - Call `notifier.notify()` to push updated entities.

2. Add a helper function `delete_connection` in `plugins/networkmanager/src/wifi.rs`:
   ```rust
   pub async fn delete_connection(conn: &Connection, connection_path: &str) -> Result<()>
   ```
   This calls `NM Settings.Connection.Delete` via D-Bus.

### Settings UI changes (`crates/settings/src/wifi/`)

1. Add `Forget` variant to `NetworkRowOutput` in `network_row.rs`.
2. Add a trash icon button (destructive-action style) to known network rows, following the Bluetooth `DeviceRow` pattern. Only show when the network is `known`.
3. In `known_networks_group.rs`, wire `NetworkRowOutput::Forget` to emit the action.
4. In `pages/wifi.rs`, add the `forget` action dispatch -- call `action_callback(urn, "forget", Value::Null)`.
5. Add a confirmation dialog before sending the forget action (following the Online Accounts `RemoveAccount` pattern with `adw::AlertDialog`).
6. Add localization strings: `wifi-forget`, `wifi-forget-confirm-title`, `wifi-forget-confirm-body`, `wifi-forget-cancel`, `wifi-forget-confirm`.

**Complexity:** Low. This is the simplest feature and should be done first.

---

## Feature 2: Per-Network WiFi Settings

### Protocol changes (`crates/protocol/src/entity/network.rs`)

1. Add optional fields to `WiFiNetwork`:
   ```rust
   pub struct WiFiNetwork {
       // existing fields...
       #[serde(default)]
       pub autoconnect: Option<bool>,
       #[serde(default)]
       pub metered: Option<MeteredState>,
       #[serde(default)]
       pub dns_servers: Option<Vec<String>>,
       #[serde(default)]
       pub ip_method: Option<IpMethod>,
   }
   ```

2. Add supporting enums:
   ```rust
   pub enum MeteredState { Unknown, Yes, No, GuessYes, GuessNo }
   pub enum IpMethod { Auto, Manual, LinkLocal, Disabled }
   ```

3. Add new action to the registry: `update-settings` with params for changeable fields.

### Plugin changes (`plugins/networkmanager/`)

1. When building `WiFiNetwork` entities for known networks, read connection profile settings via `GetSettings` D-Bus call:
   - `connection.autoconnect` -> `autoconnect`
   - `connection.metered` -> `metered`
   - `ipv4.method` -> `ip_method`
   - `ipv4.dns` -> `dns_servers`

2. Add `"update-settings"` action handler: find connection profile, read current settings, modify desired keys, call `Update` D-Bus method.

3. Add helper functions in `plugins/networkmanager/src/wifi.rs`:
   - `get_connection_settings(conn, path)` -> reads full settings dict
   - `update_connection_settings(conn, path, updates)` -> modifies and writes settings

### Settings UI changes (`crates/settings/src/wifi/`)

1. Change `WiFiPage::new` to accept `navigation_view: &adw::NavigationView`.
2. Update the page factory in `window.rs` to pass `navigation_view` (move WiFi from macro to explicit pattern).
3. Create `crates/settings/src/wifi/network_detail.rs` -- a detail sub-page with:
   - `AdwSwitchRow` for autoconnect toggle
   - `AdwComboRow` for metered state
   - DNS server entries (optional, could be deferred)
   - IP configuration (optional, could be deferred)
   - Forget button (destructive, at bottom)
   - Share QR code button (ties into Feature 3)
4. Modify `NetworkRow` to add a navigation chevron for known networks.
5. Modify `KnownNetworksGroup` to create `SettingsSubPage` per network and wire navigation.
6. Add localization strings for all new UI elements.

**Complexity:** Medium-High.

---

## Feature 3: Share Network via QR Code

### Protocol changes

Add a `share_uri` field (optional) to `WiFiNetwork` entity, populated only for known networks. The QR string format is standardized: `WIFI:T:<security>;S:<ssid>;P:<password>;;`

Alternatively, extend the action response protocol to carry response payloads. See Questions below.

### Plugin changes (`plugins/networkmanager/`)

1. For known networks, call `GetSecrets` on `org.freedesktop.NetworkManager.Settings.Connection` to retrieve stored password.
2. Construct WiFi QR code string: `WIFI:T:WPA;S:<ssid>;P:<password>;;`
3. Populate `share_uri` field on the entity (only for known networks).

### Settings UI changes (`crates/settings/src/wifi/`)

1. Add `qrcode` crate dependency to `crates/settings/Cargo.toml`.
2. Create `crates/settings/src/wifi/share_dialog.rs` -- an `adw::Dialog` that:
   - Displays the QR code as a `gtk::DrawingArea` or `Picture` widget
   - Shows the network name
   - Has a "Close" button
3. QR rendering: use the `qrcode` crate to generate a `QrCode`, paint onto `cairo::ImageSurface`, set as `gdk::Texture` on `gtk::Picture`.
4. Add "Share" button to the network detail sub-page.
5. Add localization strings: `wifi-share`, `wifi-share-title`, `wifi-share-qr-description`.

**Complexity:** Medium.

---

## Dependencies Between Features

```
Feature 3 (QR Share)
    |
    v (needs password retrieval from NM)
Feature 2 (Per-Network Settings)  <-- provides detail sub-page where Share button lives
    |                                  and the navigation_view plumbing
    v (navigation_view plumbing)
Feature 1 (Forget Network)  <-- simplest, no dependencies
```

Feature 1 is independent. Feature 2 provides the infrastructure that Feature 3 benefits from. Feature 3 could be done without Feature 2 (share button on row), but it's cleaner on the detail sub-page.

---

## Suggested Implementation Order

### Phase 1: Forget Network (smallest scope, immediate user value)
1. Add `delete_connection` helper to `plugins/networkmanager/src/wifi.rs`
2. Add `"forget"` handler in `handle_wifi_network_action`
3. Add `Forget` output to `NetworkRowOutput`
4. Add trash button to known network rows
5. Wire confirmation dialog and action dispatch
6. Add localization strings (en-US, cs-CZ)

### Phase 2: Per-Network Settings (most complex, provides infrastructure for Phase 3)
1. Add optional settings fields to `WiFiNetwork` entity
2. Add NM settings reading to the plugin
3. Add `"update-settings"` action handler
4. Plumb `navigation_view` through to WiFi page
5. Create `NetworkDetailPage` with autoconnect toggle and metered dropdown
6. Add navigation to known network rows
7. Wire detail output events to actions
8. Add localization strings

### Phase 3: QR Code Sharing (builds on Phase 2 infrastructure)
1. Add password/share data retrieval from NM (`GetSecrets`)
2. Add `share_uri` field to protocol
3. Add `qrcode` dependency to settings Cargo.toml
4. Create `share_dialog.rs` with QR rendering
5. Add Share button to network detail sub-page
6. Add localization strings

---

## Questions and Uncertainties Requiring User Input

1. **Per-network settings scope:** Which NM connection settings should be configurable? Minimal: `autoconnect` and `metered`. Fuller: DNS servers, IP method, static IP, proxy. How much should be exposed?

2. **Password exposure in entity stream:** For QR code generation, the WiFi password needs to reach the settings app:
   - (a) Include the password/QR string as optional field on entity -- simpler but flows through daemon socket on every update
   - (b) Use on-demand action/response -- more secure but requires extending `ActionSuccess` to carry payloads
   Which approach?

3. **Forget button placement:** Should it appear:
   - (a) On the known network row (like Bluetooth's Remove button) -- quick access
   - (b) Only on the per-network settings sub-page -- cleaner row UI, requires Feature 2 first
   - (c) Both locations

4. **Navigation for known networks:** Should clicking a known network row navigate to settings, or should there be an explicit chevron button?

5. **Backward compatibility:** New optional fields use `#[serde(default)]`. Is `Option<T>` sufficient, or should there be an explicit protocol version check?

---

## Critical Files

- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs` - Action handlers
- `crates/protocol/src/entity/network.rs` - WiFiNetwork entity
- `crates/settings/src/wifi/network_row.rs` - Network row widget
- `crates/settings/src/pages/wifi.rs` - WiFi page smart container
- `plugins/networkmanager/src/wifi.rs` - WiFi D-Bus helpers
