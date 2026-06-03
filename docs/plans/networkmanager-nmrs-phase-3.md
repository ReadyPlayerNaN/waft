# NetworkManager → nmrs Migration Plan: Phase 3

## Goal
Prune leftover manual D-Bus code, keep only Waft-specific logic that still adds value, and decide which features remain custom vs. fully delegated to `nmrs`.

## Scope
- Final cleanup of obsolete modules/helpers
- Decide which custom features stay
- Optionally unlock `nmrs` features currently unsupported by Waft

## Deliverables
- Clear split between:
  - `nmrs`-owned networking logic
  - Waft-owned entity/state/UI glue
- Reduced maintenance burden in `plugins/networkmanager/`
- Documented rationale for any remaining custom DBus code
- Remaining custom islands aligned with `docs/plans/networkmanager-nmrs-compatibility-matrix.md`

## Work items

### 1. Keep only Waft-specific custom logic
Likely to keep:
1. **Wi‑Fi QR code generation**
   - formatting QR payloads is a Waft feature
2. **Wi‑Fi PSK/secrets retrieval**
   - `nmrs` saved-profile APIs do not expose secrets directly
3. **Public IP fetch**
   - not a NetworkManager concern
4. **Waft entity shaping / state caching**
   - protocol and UI integration are Waft-specific
5. **BlueZ-specific tethering visibility heuristics**
   - required for current paired/actually-connected semantics
6. **Ethernet per-profile activation on a chosen adapter**
   - public `nmrs` does not appear to expose this behavior cleanly
7. **Fresh WEP creation/connect fallback**
   - public `nmrs` does not appear to expose WEP connection creation

### 2. Remove or shrink obsolete manual modules
Candidates to delete or heavily reduce:
- `plugins/networkmanager/src/dbus_property.rs`
- `plugins/networkmanager/src/wifi.rs`
- `plugins/networkmanager/src/vpn.rs`
- `plugins/networkmanager/src/ethernet.rs`
- `plugins/networkmanager/src/tethering.rs`
- `plugins/networkmanager/src/device_discovery.rs`
- `plugins/networkmanager/src/signal_monitor.rs`
- custom security helpers in `plugins/networkmanager/src/lib.rs`

Approach:
- remove only after the replacement path is proven
- prefer incremental deletion over one-shot cleanup
- keep compatibility shims only briefly

### 3. Revisit monitoring architecture
Once read-side and action-side are stable:
- simplify `signal_monitor.rs`
- determine whether `nmrs` monitoring APIs are sufficient for refresh triggers
- reduce duplicate signal handling if `nmrs` already covers it

Goal:
- monitoring should trigger refreshes, not reimplement full DBus semantics

### 4. Reevaluate unsupported or duplicated behavior
Specific items to revisit:
- current enterprise Wi‑Fi rejection
- manual security inference logic
- duplicated VPN metadata extraction
- duplicated bluetooth profile matching
- whether any raw Ethernet activation code can be isolated into a very small compatibility layer

Potential improvements:
- support enterprise Wi‑Fi via `nmrs`
- richer VPN metadata in entities/settings UI
- better multi-radio behavior using `WifiScope`

### 5. Optional feature unlocks
Only after parity and cleanup:
1. enterprise Wi‑Fi support
2. WPA3-Enterprise support
3. richer VPN display for OpenVPN/OpenConnect/strongSwan/PPTP/L2TP
4. `.ovpn` import if useful for product scope
5. airplane mode / radio state integration through `nmrs`

## Validation
- `cargo +nightly-2026-02-28 check --workspace`
- `cargo +nightly-2026-02-28 test --workspace --no-run`
- targeted manual smoke checks for:
  - Wi‑Fi listing/connect/disconnect
  - wired listing/connect/disconnect
  - VPN listing/connect/disconnect
  - tethering visibility/connect/disconnect
  - QR/share flow

## Acceptance criteria
- Manual NetworkManager D-Bus code is minimized and justified
- Remaining custom code is clearly Waft-specific or explicitly required by gaps in public `nmrs`
- Dead compatibility helpers are removed
- Entity behavior remains stable
- Any preserved raw DBus paths are documented by purpose (e.g. secrets, BlueZ visibility, Ethernet profile activation, WEP fallback)

## Risks
- Over-pruning before parity could regress behavior
- Secrets/QR flows may still require hybrid design
- Bluetooth/tethering may need persistent custom glue

## Non-goals
- No forced adoption of every `nmrs` feature
- No UI redesign
- No protocol/entity schema changes unless separately approved
