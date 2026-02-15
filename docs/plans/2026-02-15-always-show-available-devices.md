# Always Show "Available Devices" in Bluetooth Settings

## Problem

The "Available Devices" section (`DiscoveredDevicesGroup`) is only visible while a Bluetooth adapter is actively scanning (`discovering = true`). When scanning stops, the entire section disappears — including any discovered devices that were listed. This is jarring and inconsistent with the "Paired Devices" section which is always visible.

## Current Behavior

In `crates/settings/src/bluetooth/discovered_devices_group.rs`, the `reconcile()` method (lines 97-110):

```rust
if any_discovering {
    self.root.set_visible(true);
    // ...
} else {
    self.root.set_visible(false);  // ← hides entire section
    // ...
}
```

The section starts hidden (`visible(false)` at construction) and only appears when an adapter has `discovering = true`.

## Desired Behavior

The "Available Devices" section should always be visible, regardless of scanning state. The scanning state should only affect:
- Whether the spinner is shown (spinning during scan)
- The description text (contextual guidance)

## Changes

### 1. `crates/settings/src/bluetooth/discovered_devices_group.rs`

**Construction:** Remove `visible(false)` from the `AdwPreferencesGroup` builder (line 27). The group should be visible from the start.

**`reconcile()` method:** Change visibility logic to always keep the group visible. Update description text based on state:

| State | Spinner | Description |
|---|---|---|
| Scanning, no devices yet | Spinning | "Searching for devices..." |
| Scanning, has devices | Spinning | _(none)_ |
| Not scanning, has devices | Stopped | _(none)_ |
| Not scanning, no devices | Stopped | "Start scanning to discover nearby devices" |

The `self.root.set_visible(false)` call in the else-branch should be removed entirely.

### 2. No other files need changes

The smart container (`pages/bluetooth.rs`) already passes the `any_discovering` flag and discovered device list correctly. No changes needed upstream.

## Summary

Single-file change in `discovered_devices_group.rs`: remove initial `visible(false)`, remove `set_visible(false)` in the non-scanning branch, and adjust description text for the idle state.
