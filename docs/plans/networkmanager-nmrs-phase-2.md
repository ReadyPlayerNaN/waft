# NetworkManager → nmrs Migration Plan: Phase 2

## Goal
Migrate the NetworkManager plugin action-side from raw D-Bus operations to `nmrs 3`, after read-side parity is established.

## Scope
- Wi‑Fi actions
- Ethernet actions
- VPN actions
- Bluetooth/tethering actions
- Radio toggles where `nmrs` has stable equivalents

## Deliverables
- Core connect/disconnect/scan/toggle actions routed through `nmrs`
- Substantially less direct `ActivateConnection`, `Disconnect`, `GetSettings`, and property-write code
- Existing Waft action semantics preserved as closely as possible
- Explicit hybrid boundary documented for features that remain custom

## Work items

### 1. Wi‑Fi actions
Migrate:
- scan
- connect saved network
- connect unsaved network
- connect to specific AP/BSSID if needed
- disconnect
- forget saved network

Likely files:
- `plugins/networkmanager/src/wifi.rs`
- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs`

Target `nmrs` APIs:
- `scan_networks(...)`
- `connect(...)`
- `connect_to_bssid(...)`
- `disconnect(...)`
- `WifiScope`
- optionally `try_connect(...)`

Notes:
- Preserve current user-visible distinction between password-required and unsupported cases
- Re-check enterprise Wi‑Fi behavior instead of automatically rejecting it

### 2. Wi‑Fi radio enable/disable
Migrate:
- global Wi‑Fi toggle
- optionally per-interface enable/disable if useful

Likely files:
- `plugins/networkmanager/src/wifi.rs`
- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs`

Target `nmrs` APIs:
- `set_wireless_enabled(bool)`
- `WifiScope::set_enabled(bool)`

Notes:
- Preserve current plugin semantics for adapter busy/enabled/scanning state

### 3. Ethernet actions
Migrate as far as possible:
- connect wired
- disconnect active wired connection where interface-level `nmrs` APIs are sufficient

Keep custom if required:
- activate saved ethernet profile on a specific adapter

Likely files:
- `plugins/networkmanager/src/ethernet.rs`
- `plugins/networkmanager/src/wifi.rs` (if helper paths currently live there)
- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs`

Target `nmrs` APIs:
- `connect_wired()`
- `disconnect(Some(interface))`
- saved-connection APIs as needed for read-side context
- device discovery APIs to resolve interface identity cleanly

Hybrid note:
- Public `nmrs` does not appear to expose per-profile Ethernet activation on a chosen adapter, so Waft’s `ethernet-connection/{uuid}` child action may remain on raw D-Bus

### 4. VPN actions
Migrate:
- activate saved VPN
- disconnect active VPN
- use UUID-based or id-based activation instead of raw object-path activation

Likely files:
- `plugins/networkmanager/src/vpn.rs`
- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs`

Target `nmrs` APIs:
- `connect_vpn_by_uuid(...)`
- `connect_vpn_by_id(...)`
- `disconnect_vpn(...)`
- `disconnect_vpn_by_uuid(...)`

Notes:
- Prefer UUID internally where stable mapping exists
- Preserve current Waft state transitions (`Connecting`, `Disconnecting`, etc.)

### 5. Bluetooth/tethering actions
Migrate as far as possible:
- connect tethering profile / bluetooth PAN
- disconnect tethering where interface/device resolution is possible
- forget bluetooth profile if needed later

Likely files:
- `plugins/networkmanager/src/tethering.rs`
- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs`

Target `nmrs` APIs:
- `connect_bluetooth(...)`
- `forget_bluetooth(...)`
- `list_bluetooth_devices()` for matching devices/roles

Notes:
- May still need Waft-specific selection logic for “smart toggle” behavior
- May still need raw D-Bus fallback for tethering disconnect depending on how reliably profile/device/interface mapping can be derived through public `nmrs`

### 6. Action error mapping
Introduce a consistent translation layer from `nmrs::ConnectionError` into:
- current Waft log messages
- current action failure semantics
- current UI-relevant error strings where required

Likely files:
- new adapter/error module
- daemon action handlers

## Validation
After each subsystem migration:
- `cargo +nightly-2026-02-28 check --workspace`
- `cargo +nightly-2026-02-28 test --workspace --no-run`

Recommended targeted manual checks:
- Wi‑Fi scan/connect/disconnect
- Wi‑Fi saved network reconnect
- Wired connect/disconnect
- VPN connect/disconnect
- Bluetooth tethering connect/disconnect

## Acceptance criteria
- Wi‑Fi actions no longer depend on raw D-Bus method calls for primary flows
- Ethernet actions no longer depend on raw activation plumbing for primary flows
- VPN actions route through `nmrs`
- Tethering actions use `nmrs` where supported
- Existing Waft action names and entity behavior remain stable
- Workspace compiles and test targets compile

## Risks
- Some Waft semantics do not map 1:1 to `nmrs` convenience APIs
- Ethernet per-profile child actions are not cleanly represented in public `nmrs`
- Tethering may still need hybrid logic if device/profile matching is app-specific
- Error wording may change if not normalized explicitly

## Non-goals
- Secret/QR handling remains out of scope here
- Public IP fetch remains out of scope here
- Full monitor simplification is not required for Phase 2 completion
- Do not force-migrate features that the compatibility matrix marks as missing if it would degrade behavior
