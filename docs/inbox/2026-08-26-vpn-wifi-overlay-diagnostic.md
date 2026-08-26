# VPN / Wi‑Fi overlay diagnostic

## Summary

The issue is not only in the overlay UI. `waft` itself is currently holding stale network state for both Wi‑Fi and VPN, and the overlay also has separate toggle wiring/update bugs.

## Live evidence

System state from NetworkManager:

- `wlan0` is connected to `Cookielab.io 4`
- WireGuard connection `kiwi` is active

Observed via `waft query`:

- `networkmanager/network-adapter/wlan0` reports `connected: false`
- `networkmanager/network-adapter/wlan0/wifi-network/Cookielab.io` reports `connected: false`
- VPN entities report transitional states:
  - `home` = `Connecting`
  - `kiwi` = `Disconnecting`

This shows the stale state already exists in the `networkmanager` plugin / daemon entity cache before the overview renders it.

## UI-side bugs found

### 1. Wi‑Fi top-level toggle is miswired

File: `crates/overview/src/components/toggles/network/wifi.rs`

The click handler sends `"activate"` for both:

- `FeatureToggleOutput::Activate`
- `FeatureToggleOutput::Deactivate`

Effect:

- clicking the Wi‑Fi toggle off does not request deactivation
- the control can appear stuck from the user perspective

### 2. VPN top-level toggle is miswired

File: `crates/overview/src/components/toggles/network/vpn.rs`

The consolidated VPN toggle sends disconnect actions for currently active/connecting VPNs on both:

- `FeatureToggleOutput::Activate`
- `FeatureToggleOutput::Deactivate`

Effect:

- if VPN is off, clicking the top-level VPN toggle does nothing useful
- if VPN is on, clicking the toggle only disconnects
- the consolidated toggle has asymmetric / broken behavior

### 3. Wi‑Fi menu rows are not incrementally updated

File: `crates/overview/src/components/toggles/network/wifi.rs`

Existing rows are intentionally skipped with logic equivalent to:

- if row already exists, do nothing

Effect:

- checkmarks
- connected indicators
- row contents
- per-network visual state

can remain stale even when entity data changes.

## Backend / plugin-side diagnostic

### 1. NetworkManager plugin state is cached and signal-driven

Relevant files:

- `plugins/networkmanager/src/signal_monitor.rs`
- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs`

The plugin keeps internal cached state and relies on D-Bus signals plus refresh helpers to keep entities correct.

### 2. Wi‑Fi connected state can get stuck as disconnected

The plugin clears `active_ssid` when:

- `ActiveAccessPoint` becomes `/`
- or device state is no longer activated

It only repopulates `active_ssid` on later refresh paths.

If that follow-up refresh is missed, waft remains in a disconnected state even while NetworkManager is actually connected.

This matches the current observed system state.

### 3. Wi‑Fi enabled/disabled likely does not track external changes

The plugin appears to update Wi‑Fi enabled state mainly from its own action handlers.

I did not find monitoring for NetworkManager’s `WirelessEnabled` property in the signal handling path.

Implication:

- if Wi‑Fi state changes outside waft
- waft can drift out of sync

### 4. VPN transitional states can stick forever

The plugin sets VPN state optimistically to:

- `Connecting`
- `Disconnecting`

and then relies on later D-Bus refreshes to converge to the real state.

If the refresh path is missed, waft can stay stuck in a transitional state indefinitely.

This matches the current observed daemon state where:

- active `kiwi` appears as `Disconnecting`
- inactive `home` appears as `Connecting`

## Conclusion

This appears to be a combined problem:

1. **Backend state-sync bug** in the `networkmanager` plugin / daemon state tracking
2. **Frontend Wi‑Fi toggle wiring bug**
3. **Frontend VPN consolidated toggle wiring bug**
4. **Frontend Wi‑Fi menu row refresh bug**

So the broken behavior in the overlay is real, but it is not purely visual. The underlying waft network entities are already stale.

## Recommended next step

Do not fix blindly in the overview first.

Suggested fix order:

1. confirm and correct backend state-sync in `plugins/networkmanager`
2. fix Wi‑Fi top-level toggle action wiring
3. fix VPN consolidated toggle behavior
4. make Wi‑Fi menu rows update in place when entity state changes
