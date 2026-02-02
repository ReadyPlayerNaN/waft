## 1. Sunsetr preset menu: Fix checkmark jumping during transitions

**Issue:** When switching presets (e.g., day → gaming), the checkmark jumps through an intermediate state (day → default → gaming) instead of going directly to the target preset.

**Current behavior:**

- User clicks "gaming" preset
- Checkmark moves to "Default"
- Checkmark moves to "gaming"

**Expected behavior:**

- Checkmark should move directly from "day" to "gaming"

**Investigation needed:**

- IPC event timing/sequencing during preset transitions
- Potential race condition between `set_preset()` call and `spawn_following()` events
- Verify what events sunsetr emits during preset changes

**Files involved:**

- `src/features/sunsetr/ipc.rs` - IPC event handling
- `src/features/sunsetr/mod.rs` - Event processing
- `src/features/sunsetr/store.rs` - State management

## 2. Plugins to implement

- Tether plugin?
- SNI

## 3. NetworkManager plugin enhancements

- WiFi: Support connecting to new (unsaved) networks with password prompt
- WiFi: Signal strength icon updates in toggle (currently just on/off)
