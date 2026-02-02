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

## 4. Feature toggle loading state

Can we do CSS animated borders?

## 5. Feature toggle off main button hover colour

It has primary accent on hover, but it should be neutral

## 6. Notification deprioritization

Some notifications may be deprioritized and maybe formatted in a different way. Write down suggestions here:

- Device connected (set lower priority, add ttl?)

## 7. Notification toast bubbles

We could create a more fun way to deal with notifications: Bubbles, like in Civilization VI.

## 8. Notification toast window position

We should support at least bottom. We need to deal with order of the toasts and potentially fix all the animations
