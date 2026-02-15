# Plan: Add DnD Toggle to Notifications Settings Page

## Context

The Notifications settings page (`crates/settings/src/pages/notifications.rs`) currently shows
notification filter management (active profile, groups, profiles). The "Do Not Disturb" toggle
is available in the overview overlay but missing from settings. Adding it here gives users a
persistent, discoverable place to control DnD.

## Scope

Add an `adw::SwitchRow` for DnD at the top of the Notifications settings page. Follow the
existing `DarkModeSection` pattern exactly — it's the closest analog (single boolean entity,
toggle action, `updating` guard, hidden until entity arrives).

## Changes

### 1. Subscribe to DnD entity type in `app.rs`

**File:** `crates/settings/src/app.rs`

Add `DND_ENTITY_TYPE` to the `ENTITY_TYPES` array so the settings app receives DnD entity
updates from the daemon.

```rust
use waft_protocol::entity::notification::DND_ENTITY_TYPE;

const ENTITY_TYPES: &[&str] = &[
    // ... existing types ...
    DND_ENTITY_TYPE,
];
```

### 2. Create `DndSection` widget

**File:** `crates/settings/src/notifications/dnd_section.rs`

New smart container following `DarkModeSection` pattern:

- `adw::PreferencesGroup` with title "Do Not Disturb", initially `visible: false`
- Single `adw::SwitchRow` with title "Do Not Disturb"
- Subscribe to `DND_ENTITY_TYPE`, update switch state with `updating` guard
- On toggle: send action `"toggle"` to URN `notifications/dnd/default`
- Initial reconciliation via `idle_add_local_once`

### 3. Register module in `notifications/mod.rs`

**File:** `crates/settings/src/notifications/mod.rs`

Add `pub mod dnd_section;`.

### 4. Add DnD section to Notifications page

**File:** `crates/settings/src/pages/notifications.rs`

Insert `DndSection` as the first child of the page root box (before ActiveProfileSection),
so the toggle appears at the top.

## Non-goals

- No icon changes based on DnD state (settings uses standard adw rows, not FeatureToggleWidget)
- No schedule/timer — just the on/off toggle matching the daemon's current DnD state
