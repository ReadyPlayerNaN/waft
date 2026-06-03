# NetworkManager → nmrs Migration Plan: Phase 0 + Phase 1

## Goal
Replace dead or redundant dependency usage and migrate the NetworkManager plugin read-side from hand-rolled D-Bus logic to `nmrs 3`, while keeping Waft entity behavior stable.

## Scope
- Cleanup unused `nmrs` dependency from overview
- Add `nmrs 3` to the NetworkManager plugin
- Introduce adapter/mapping layer from `nmrs` models to Waft entities/state
- Migrate read/discovery/profile enumeration paths
- Keep action-side behavior unchanged for now

## Deliverables
- `crates/overview/Cargo.toml`: remove dead `nmrs`
- `plugins/networkmanager/Cargo.toml`: add `nmrs = "3"`
- New internal adapter module(s) for `nmrs` → Waft translation
- Read-side data sources migrated to `nmrs`
- Existing entity types and UI-facing behavior preserved
- Compatibility matrix captured in `docs/plans/networkmanager-nmrs-compatibility-matrix.md`

## Work items

### Phase 0 — cleanup + prep
1. Remove unused dependency:
   - `crates/overview/Cargo.toml`
2. Add `nmrs` to:
   - `plugins/networkmanager/Cargo.toml`
3. Add a small abstraction layer in `plugins/networkmanager/src/` for:
   - mapping `nmrs::Device`, `nmrs::Network`, `nmrs::AccessPoint`, `nmrs::SavedConnection`, `nmrs::VpnConnection`, `nmrs::BluetoothDevice`
   - converting them into Waft state structs / protocol-facing entity data
4. Keep existing plugin logic working unchanged around the new layer.

### Phase 1 — read-side migration

#### 1. Device discovery
Replace custom read-side discovery for:
- Wi‑Fi devices
- Ethernet devices
- Bluetooth devices

Likely files:
- `plugins/networkmanager/src/device_discovery.rs`
- `plugins/networkmanager/src/bluez_discovery.rs`
- `plugins/networkmanager/src/state.rs`

Target `nmrs` APIs:
- `NetworkManager::list_devices()`
- `NetworkManager::list_wifi_devices()`
- `NetworkManager::list_bluetooth_devices()`

#### 2. Wi‑Fi network enumeration
Replace hand-rolled access point reads, grouping, and security decoding.

Likely files:
- `plugins/networkmanager/src/wifi.rs`
- `plugins/networkmanager/src/lib.rs`

Target `nmrs` APIs:
- `NetworkManager::list_networks(interface)`
- `NetworkManager::list_access_points(interface)`
- `NetworkManager::wifi("...")`

Notes:
- Keep Waft’s own `SecurityType` mapping layer
- Keep any temporary state needed so connected networks remain visible immediately if `nmrs` behavior differs

#### 3. Saved connection/profile reads
Replace raw `ListConnections` / `GetSettings` enumeration logic.

Likely files:
- `plugins/networkmanager/src/wifi.rs`
- `plugins/networkmanager/src/ethernet.rs`
- `plugins/networkmanager/src/vpn.rs`
- `plugins/networkmanager/src/tethering.rs`

Target `nmrs` APIs:
- `list_saved_connections()`
- `list_saved_connections_brief()`
- `get_saved_connection()`

#### 4. VPN enumeration
Replace manual VPN saved-profile and active-state discovery.

Likely files:
- `plugins/networkmanager/src/vpn.rs`
- parts of `plugins/networkmanager/bin/waft-networkmanager-daemon.rs`

Target `nmrs` APIs:
- `list_vpn_connections()`
- `active_vpn_connections()`

#### 5. Bluetooth/tethering read path
Use `nmrs` Bluetooth enumeration as the main source where possible, but preserve Waft’s custom BlueZ-based tethering visibility heuristic.

Likely files:
- `plugins/networkmanager/src/tethering.rs`
- `plugins/networkmanager/src/bluez_discovery.rs`

Target `nmrs` APIs:
- `list_bluetooth_devices()`

Notes:
- Preserve Waft’s visibility semantics if `nmrs` alone is insufficient
- BlueZ paired/connected state remains a justified custom path per the compatibility matrix

#### 6. Monitoring strategy
Do not rewrite monitoring first.

Plan:
- keep current signal-based monitoring initially
- swap read-side refresh functions under the monitor callbacks
- simplify monitor internals only after parity is confirmed

Likely files:
- `plugins/networkmanager/src/signal_monitor.rs`
- `plugins/networkmanager/src/bluez_signal_monitor.rs`

## Validation
Run after each meaningful step:
- `cargo +nightly-2026-02-28 check --workspace`
- `cargo +nightly-2026-02-28 test --workspace --no-run`

## Acceptance criteria
- Overview no longer depends on `nmrs`
- NetworkManager plugin builds with `nmrs 3`
- Same Waft entity types are emitted
- Wi‑Fi / Ethernet / VPN / tethering lists still populate correctly
- Direct D-Bus read logic is reduced substantially
- No action flow changes yet

## Risks
- Mapping mismatches between `nmrs` models and current Waft state
- Bluetooth/tethering visibility requires BlueZ-specific glue and should not be force-migrated away
- Existing signal monitor assumptions may not perfectly match `nmrs` state snapshots
- Saved-profile read migration may still need thin raw-settings glue for Waft-specific field extraction

## Non-goals
- No action-side migration yet
- No QR/secret handling changes yet
- No user-facing feature expansion yet
- No attempt to remove justified custom BlueZ visibility logic in this phase
