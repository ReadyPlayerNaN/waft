# Toggle Consolidation Design

**Date:** 2026-02-22
**Branch:** larger-larger-picture

## Problem

Four simple feature toggles (`caffeine`, `dark_mode`, `dnd`, `night_light`) share ~90% identical code:
same struct shape, constructor signature, action dispatch, subscription/availability pattern, and
`as_feature_toggles()` structure. Only the entity type, URN, icon, i18n key, widget ID, weight, and
per-entity update logic differ.

Additionally, `bluetooth.rs` and `network/mod.rs` duplicate the settings app tracking pattern:
finding the `waft-settings` URN, maintaining availability state, subscribing to app entity changes
with initial reconciliation, and building settings buttons.

## Goals

1. Eliminate boilerplate across the 4 simple toggles using a `SimpleToggle` builder.
2. Extract shared settings app tracking into a `SettingsAppTracker` type used by bluetooth and network.
3. Fix the inconsistent `waft-settings` URN lookup in `bluetooth.rs` (currently icon-based; should
   match the `internal-apps/app/waft-settings` URN used in network).

## Non-Goals

- Unifying `BluetoothToggles` and `NetworkManagerToggles` architecturally (they are too different).
- Unifying the `ToggleEntry` structs (fields diverge significantly beyond the first three).
- Touching `backup.rs`, `bluetooth.rs` device row logic, or network menu logic beyond the tracker.

---

## Part 1: `SimpleToggle`

### New file: `crates/overview/src/components/toggles/simple_toggle.rs`

```rust
/// Describes the widget state update after an entity arrives.
pub struct ToggleUpdate {
    pub active: bool,
    pub details: Option<String>,
    /// None = leave icon unchanged; Some = set icon to this name.
    pub icon: Option<&'static str>,
}

/// Configuration for a single-entity, no-menu feature toggle.
pub struct SimpleToggleConfig<E> {
    pub entity_type: &'static str,
    pub urn: Urn,
    pub icon: &'static str,
    pub title: String,
    pub widget_id: &'static str,
    pub weight: i32,
    pub on_update: fn(&E) -> ToggleUpdate,
}

/// A single-entity feature toggle that hides itself until entity data arrives.
pub struct SimpleToggle {
    toggle: Rc<FeatureToggleWidget>,
    available: Rc<Cell<bool>>,
    widget_id: &'static str,
    weight: i32,
}
```

`SimpleToggle::new<E>(store, action_callback, rebuild_callback, config)` wires the widget,
action dispatch, and subscription internally. All four simple toggles reduce to a single call.

`SimpleToggle` implements `DynamicToggleSource`, removing the four individual impls in
`renderer.rs`.

### Updated toggle files

Each file shrinks to one public constructor function returning `SimpleToggle`:

```rust
// caffeine.rs
pub fn caffeine_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(store, action_callback, rebuild_callback, SimpleToggleConfig {
        entity_type: entity::session::SLEEP_INHIBITOR_ENTITY_TYPE,
        urn: Urn::new("caffeine", "sleep-inhibitor", "default"),
        icon: "changes-allow-symbolic",
        title: crate::i18n::t("caffeine-title"),
        widget_id: "caffeine-toggle",
        weight: 300,
        on_update: |i: &entity::session::SleepInhibitor| ToggleUpdate {
            active: i.active,
            details: i.active.then(|| crate::i18n::t("caffeine-active")),
            icon: None,
        },
    })
}
```

Same pattern for `dark_mode`, `dnd`, `night_light`.

### Renderer changes

- Remove 4 named struct imports (`CaffeineToggle`, etc.)
- Import constructor functions from each toggle module
- Match arms call the function and push `Rc<SimpleToggle>` into `dynamic_sources` / `keep`
- Remove 4 `DynamicToggleSource` impls (replaced by one impl on `SimpleToggle`)

---

## Part 2: `SettingsAppTracker`

### New file: `crates/overview/src/components/toggles/settings_app_tracker.rs`

```rust
pub struct SettingsAppTracker {
    available: Rc<Cell<bool>>,
    urn: Rc<RefCell<Option<Urn>>>,
}

impl SettingsAppTracker {
    /// Subscribes to app entity changes and performs initial reconciliation.
    /// `on_change(is_available)` is called whenever the settings app appears or disappears.
    pub fn new(
        store: &Rc<EntityStore>,
        on_change: impl Fn(bool) + 'static,
    ) -> Self;

    pub fn is_available(&self) -> bool;
    pub fn urn(&self) -> Option<Urn>;

    /// Build a settings button that dispatches `open-page` to the settings app.
    pub fn build_settings_button(
        &self,
        action_callback: &EntityActionCallback,
        page: &'static str,
        label: String,
    ) -> FeatureToggleMenuSettingsButton;
}
```

URN lookup uses the canonical `internal-apps/app/waft-settings` heuristic (matching the existing
`find_settings_app_urn` in `network/mod.rs`). This fixes the icon-based lookup currently used in
`bluetooth.rs`.

### Changes to `bluetooth.rs`

- Remove local `settings_available`, `settings_urn`, duplicate `subscribe_type(app::ENTITY_TYPE)`,
  and `idle_add_local_once` blocks.
- Replace with `SettingsAppTracker::new(store, on_change)`.
- Replace local `build_settings_button` with `tracker.build_settings_button(...)`.

### Changes to `network/mod.rs`

- Remove local `settings_available`, `settings_urn`, `find_settings_app_urn`, duplicate
  `subscribe_type(app::ENTITY_TYPE)`, reconcile closure, and `idle_add_local_once`.
- Replace with `SettingsAppTracker::new(store, on_change)`.
- Replace `build_settings_button` with `tracker.build_settings_button(...)`.

---

## File Changes Summary

| File | Action |
|------|--------|
| `components/toggles/simple_toggle.rs` | **New** |
| `components/toggles/settings_app_tracker.rs` | **New** |
| `components/toggles/caffeine.rs` | Shrinks to constructor fn |
| `components/toggles/dark_mode.rs` | Shrinks to constructor fn |
| `components/toggles/dnd.rs` | Shrinks to constructor fn |
| `components/toggles/night_light.rs` | Shrinks to constructor fn |
| `components/toggles/bluetooth.rs` | Use `SettingsAppTracker` |
| `components/toggles/network/mod.rs` | Use `SettingsAppTracker` |
| `layout/renderer.rs` | Use `SimpleToggle`, remove 4 impls |

## Estimated Line Changes

- `simple_toggle.rs`: ~80 lines added
- `settings_app_tracker.rs`: ~70 lines added
- 4 toggle files: ~55 lines each → ~12 lines each (−172 lines total)
- `bluetooth.rs`: −40 lines
- `network/mod.rs`: −50 lines
- `renderer.rs`: −20 lines

**Net reduction: ~130 lines, with ~150 lines of new focused abstractions.**
