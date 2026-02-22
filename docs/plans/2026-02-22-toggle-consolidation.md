# Toggle Consolidation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate ~170 lines of boilerplate across the four simple feature toggles (caffeine, dark_mode, dnd, night_light) and the duplicated settings-app tracking in bluetooth/network, while fixing the inconsistent waft-settings URN lookup in bluetooth.

**Architecture:** Introduce `SimpleToggle<E>` (a generic single-entity toggle builder in `simple_toggle.rs`) and `SettingsAppTracker` (a settings app availability tracker in `settings_app_tracker.rs`). The four simple toggle files shrink to constructor functions; bluetooth and network drop their duplicated subscription/tracking blocks.

**Tech Stack:** Rust, GTK4 (waft-ui-gtk), waft-client (EntityStore, EntityActionCallback), waft-protocol (entity types, Urn)

**Design doc:** `docs/plans/2026-02-22-toggle-consolidation-design.md`

---

### Task 1: Create `simple_toggle.rs`

**Files:**
- Create: `crates/overview/src/components/toggles/simple_toggle.rs`
- Modify: `crates/overview/src/components/toggles/mod.rs` (add `pub mod simple_toggle;`)

**Step 1: Write `simple_toggle.rs`**

```rust
//! Generic single-entity feature toggle.
//!
//! `SimpleToggle` covers the common case of a feature toggle backed by
//! exactly one entity type, a "toggle" action, and no expandable menu.
//! Hidden until entity data arrives from the daemon.

use std::cell::Cell;
use std::rc::Rc;

use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::layout::types::WidgetFeatureToggle;
use waft_client::{EntityActionCallback, EntityStore};

/// Widget state update derived from a received entity.
pub struct ToggleUpdate {
    pub active: bool,
    /// None keeps existing details text; Some(_) replaces it.
    pub details: Option<String>,
    /// None keeps the current icon; Some("name") replaces it.
    pub icon: Option<&'static str>,
}

/// Configuration for a `SimpleToggle`.
pub struct SimpleToggleConfig<E> {
    /// Entity type constant (e.g. `entity::session::SLEEP_INHIBITOR_ENTITY_TYPE`).
    pub entity_type: &'static str,
    /// URN to dispatch the "toggle" action to.
    pub urn: Urn,
    /// Initial icon name (from the icon theme).
    pub icon: &'static str,
    /// Localized display title.
    pub title: String,
    /// Stable widget ID used in `WidgetFeatureToggle`.
    pub widget_id: &'static str,
    /// Sort weight in the feature grid (lower = further left).
    pub weight: i32,
    /// Maps a received entity to widget state updates.
    pub on_update: fn(&E) -> ToggleUpdate,
}

/// A single-entity, no-menu feature toggle.
///
/// Reports zero toggles until the first entity arrives from the daemon,
/// then exactly one. Dispatches `"toggle"` action on click.
pub struct SimpleToggle {
    toggle: Rc<FeatureToggleWidget>,
    available: Rc<Cell<bool>>,
    widget_id: &'static str,
    weight: i32,
}

impl SimpleToggle {
    pub fn new<E>(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        rebuild_callback: Rc<dyn Fn()>,
        config: SimpleToggleConfig<E>,
    ) -> Self
    where
        E: serde::de::DeserializeOwned + 'static,
    {
        let available = Rc::new(Cell::new(false));

        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: false,
                icon: config.icon.to_string(),
                title: config.title,
                menu_id: None,
            },
            None,
        ));

        let cb = action_callback.clone();
        let urn = config.urn;
        toggle.connect_output(move |_output| {
            cb(urn.clone(), "toggle".to_string(), serde_json::Value::Null);
        });

        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        let available_ref = available.clone();
        let entity_type = config.entity_type;
        let on_update = config.on_update;

        store.subscribe_type(entity_type, move || {
            let entities: Vec<(Urn, E)> = store_ref.get_entities_typed(entity_type);

            let was_available = available_ref.get();
            let now_available = !entities.is_empty();

            if let Some((_urn, entity)) = entities.first() {
                let update = on_update(entity);
                toggle_ref.set_active(update.active);
                toggle_ref.set_details(update.details);
                if let Some(icon) = update.icon {
                    toggle_ref.set_icon(icon);
                }
            }

            if was_available != now_available {
                available_ref.set(now_available);
                rebuild_callback();
            }
        });

        Self {
            toggle,
            available,
            widget_id: config.widget_id,
            weight: config.weight,
        }
    }

    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        if !self.available.get() {
            return Vec::new();
        }
        vec![Rc::new(WidgetFeatureToggle {
            id: self.widget_id.to_string(),
            weight: self.weight,
            toggle: (*self.toggle).clone(),
            menu: None,
        })]
    }
}
```

**Step 2: Add to mod.rs**

In `crates/overview/src/components/toggles/mod.rs`, add before the other modules:
```rust
pub mod simple_toggle;
```

**Step 3: Build**

```bash
cargo build -p waft-overview 2>&1 | head -30
```
Expected: compiles (new module has no errors).

**Step 4: Commit**

```bash
git add crates/overview/src/components/toggles/simple_toggle.rs \
        crates/overview/src/components/toggles/mod.rs
git commit -m "feat(overview): add SimpleToggle generic single-entity toggle builder"
```

---

### Task 2: Replace `caffeine.rs`

**Files:**
- Modify: `crates/overview/src/components/toggles/caffeine.rs`

**Step 1: Replace the entire file**

```rust
//! Caffeine (sleep inhibitor) toggle component.
//!
//! Subscribes to the `sleep-inhibitor` entity type and renders a
//! FeatureToggleWidget that prevents the screen from sleeping.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use super::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn caffeine_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
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
        },
    )
}
```

**Step 2: Build**

```bash
cargo build -p waft-overview 2>&1 | head -30
```
Expected: `CaffeineToggle` is now unused — compile errors in `renderer.rs` referencing it are expected. We fix the renderer in Task 6.

If there are errors only in `renderer.rs`, proceed. If there are errors inside `caffeine.rs` itself, fix them first.

**Step 3: Commit**

```bash
git add crates/overview/src/components/toggles/caffeine.rs
git commit -m "refactor(overview): replace CaffeineToggle struct with caffeine_toggle constructor"
```

---

### Task 3: Replace `dark_mode.rs`

**Files:**
- Modify: `crates/overview/src/components/toggles/dark_mode.rs`

**Step 1: Replace the entire file**

```rust
//! Dark mode toggle component.
//!
//! Subscribes to the `dark-mode` entity type and renders a FeatureToggleWidget
//! that switches between light and dark themes.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use super::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn dark_mode_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::display::DARK_MODE_ENTITY_TYPE,
            urn: Urn::new("darkman", "dark-mode", "default"),
            icon: "weather-clear-night-symbolic",
            title: crate::i18n::t("darkman-title"),
            widget_id: "dark-mode-toggle",
            weight: 200,
            on_update: |d: &entity::display::DarkMode| ToggleUpdate {
                active: d.active,
                details: None,
                icon: None,
            },
        },
    )
}
```

**Step 2: Build — same expectation as Task 2**

```bash
cargo build -p waft-overview 2>&1 | head -30
```

**Step 3: Commit**

```bash
git add crates/overview/src/components/toggles/dark_mode.rs
git commit -m "refactor(overview): replace DarkModeToggle struct with dark_mode_toggle constructor"
```

---

### Task 4: Replace `dnd.rs`

**Files:**
- Modify: `crates/overview/src/components/toggles/dnd.rs`

**Step 1: Replace the entire file**

```rust
//! Do Not Disturb toggle component.
//!
//! Subscribes to the `dnd` entity type and renders a FeatureToggleWidget
//! that silences notification toasts. Also switches icon when active.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use super::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn dnd_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::notification::DND_ENTITY_TYPE,
            urn: Urn::new("notifications", "dnd", "default"),
            icon: "preferences-system-notifications-symbolic",
            title: crate::i18n::t("dnd-title"),
            widget_id: "dnd-toggle",
            weight: 60,
            on_update: |d: &entity::notification::Dnd| ToggleUpdate {
                active: d.active,
                details: d.active.then(|| crate::i18n::t("dnd-silenced")),
                icon: Some(if d.active {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                }),
            },
        },
    )
}
```

**Step 2: Build**

```bash
cargo build -p waft-overview 2>&1 | head -30
```

**Step 3: Commit**

```bash
git add crates/overview/src/components/toggles/dnd.rs
git commit -m "refactor(overview): replace DoNotDisturbToggle struct with dnd_toggle constructor"
```

---

### Task 5: Replace `night_light.rs`

**Files:**
- Modify: `crates/overview/src/components/toggles/night_light.rs`

**Step 1: Replace the entire file**

```rust
//! Night light toggle component.
//!
//! Subscribes to the `night-light` entity type and renders a FeatureToggleWidget
//! that enables/disables blue light filtering. Shows the current period as detail text.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use super::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn night_light_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::display::NIGHT_LIGHT_ENTITY_TYPE,
            urn: Urn::new("sunsetr", "night-light", "default"),
            icon: "night-light-symbolic",
            title: crate::i18n::t("nightlight-title"),
            widget_id: "night-light-toggle",
            weight: 210,
            on_update: |n: &entity::display::NightLight| ToggleUpdate {
                active: n.active,
                details: n.period.clone(),
                icon: None,
            },
        },
    )
}
```

**Step 2: Build**

```bash
cargo build -p waft-overview 2>&1 | head -30
```

**Step 3: Commit**

```bash
git add crates/overview/src/components/toggles/night_light.rs
git commit -m "refactor(overview): replace NightLightToggle struct with night_light_toggle constructor"
```

---

### Task 6: Update `renderer.rs`

The renderer still references the old named structs. This task wires up the new constructors and consolidates `DynamicToggleSource` impls.

**Files:**
- Modify: `crates/overview/src/layout/renderer.rs`

**Step 1: Replace the 4 named toggle imports**

Find these lines near the top of `renderer.rs`:
```rust
use crate::components::toggles::caffeine::CaffeineToggle;
use crate::components::toggles::dark_mode::DarkModeToggle;
use crate::components::toggles::dnd::DoNotDisturbToggle;
// ... (there may be a night_light import too)
use crate::components::toggles::night_light::NightLightToggle;
```

Replace them with:
```rust
use crate::components::toggles::caffeine::caffeine_toggle;
use crate::components::toggles::dark_mode::dark_mode_toggle;
use crate::components::toggles::dnd::dnd_toggle;
use crate::components::toggles::night_light::night_light_toggle;
use crate::components::toggles::simple_toggle::SimpleToggle;
```

**Step 2: Replace the 4 match arms**

Find the match block inside the `FeatureToggleGrid` renderer that handles `"DndToggle"`, `"CaffeineToggle"`, `"DarkModeToggle"`, `"NightLightToggle"`. Replace each arm:

```rust
"DndToggle" => {
    let t = Rc::new(dnd_toggle(
        &ctx.store,
        &ctx.action_callback,
        dynamic_rebuild.clone(),
    ));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
"CaffeineToggle" => {
    let t = Rc::new(caffeine_toggle(
        &ctx.store,
        &ctx.action_callback,
        dynamic_rebuild.clone(),
    ));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
"DarkModeToggle" => {
    let t = Rc::new(dark_mode_toggle(
        &ctx.store,
        &ctx.action_callback,
        dynamic_rebuild.clone(),
    ));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
"NightLightToggle" => {
    let t = Rc::new(night_light_toggle(
        &ctx.store,
        &ctx.action_callback,
        dynamic_rebuild.clone(),
    ));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
```

**Step 3: Replace the 4 individual `DynamicToggleSource` impls with one**

Find and delete these four blocks at the bottom of `renderer.rs`:
```rust
impl DynamicToggleSource for DarkModeToggle { ... }
impl DynamicToggleSource for NightLightToggle { ... }
impl DynamicToggleSource for CaffeineToggle { ... }
impl DynamicToggleSource for DoNotDisturbToggle { ... }
```

Add one impl in their place:
```rust
impl DynamicToggleSource for SimpleToggle {
    fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        SimpleToggle::as_feature_toggles(self)
    }
}
```

**Step 4: Build and test**

```bash
cargo build -p waft-overview 2>&1 | head -30
cargo test -p waft-overview 2>&1
```
Expected: clean build, all tests pass.

**Step 5: Commit**

```bash
git add crates/overview/src/layout/renderer.rs
git commit -m "refactor(overview): use SimpleToggle in renderer, remove 4 DynamicToggleSource impls"
```

---

### Task 7: Create `settings_app_tracker.rs`

This extracts the shared settings-app tracking logic from bluetooth and network.

**Files:**
- Create: `crates/overview/src/components/toggles/settings_app_tracker.rs`
- Modify: `crates/overview/src/components/toggles/mod.rs` (add `pub mod settings_app_tracker;`)

**Step 1: Write the unit tests first**

The `find_settings_app_urn` function is pure and directly unit-testable. Note that `network/mod.rs` already has identical tests — these will be removed from there in Task 9. Write them in the new file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_app_entry(plugin: &str, id: &str) -> (Urn, entity::app::App) {
        let urn = Urn::new(plugin, entity::app::ENTITY_TYPE, id);
        let app = entity::app::App {
            name: "Test App".to_string(),
            icon: "test-icon".to_string(),
            available: true,
            keywords: vec![],
            description: None,
        };
        (urn, app)
    }

    #[test]
    fn settings_urn_found_when_internal_apps_present() {
        let apps = vec![make_app_entry("internal-apps", "waft-settings")];
        let expected = Urn::new("internal-apps", entity::app::ENTITY_TYPE, "waft-settings");
        assert_eq!(find_settings_app_urn(&apps), Some(expected));
    }

    #[test]
    fn settings_urn_none_when_only_xdg_apps_present() {
        let apps = vec![
            make_app_entry("xdg-apps", "firefox"),
            make_app_entry("xdg-apps", "nautilus"),
        ];
        assert_eq!(find_settings_app_urn(&apps), None);
    }

    #[test]
    fn settings_urn_found_among_mixed_app_entities() {
        let settings_urn = Urn::new("internal-apps", entity::app::ENTITY_TYPE, "waft-settings");
        let apps = vec![
            make_app_entry("xdg-apps", "firefox"),
            (
                settings_urn.clone(),
                entity::app::App {
                    name: "Settings".to_string(),
                    icon: "preferences-system-symbolic".to_string(),
                    available: true,
                    keywords: vec![],
                    description: None,
                },
            ),
        ];
        assert_eq!(find_settings_app_urn(&apps), Some(settings_urn));
    }

    #[test]
    fn settings_urn_none_when_no_apps() {
        assert_eq!(find_settings_app_urn(&[]), None);
    }
}
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p waft-overview settings_app_tracker 2>&1
```
Expected: module not found (tests can't run yet).

**Step 3: Write the full `settings_app_tracker.rs`**

```rust
//! Settings app availability tracking.
//!
//! Both Bluetooth and Network toggles show a "Settings" button when the
//! `waft-settings` app entity is present. This module centralises discovery,
//! subscription with initial reconciliation, and settings button construction.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib;
use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use crate::ui::feature_toggles::menu_settings::{
    FeatureToggleMenuSettingsButton, FeatureToggleMenuSettingsButtonProps,
};

/// Tracks whether the waft-settings app entity is present.
///
/// Subscribes to app entity changes and calls `on_change(is_available)`
/// whenever the settings app appears or disappears. Performs initial
/// reconciliation via `idle_add_local_once` to catch entities already cached.
pub struct SettingsAppTracker {
    available: Rc<Cell<bool>>,
    urn: Rc<RefCell<Option<Urn>>>,
}

impl SettingsAppTracker {
    pub fn new(store: &Rc<EntityStore>, on_change: impl Fn(bool) + 'static) -> Self {
        let available = Rc::new(Cell::new(false));
        let urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));
        let on_change = Rc::new(on_change);

        let reconcile = {
            let available = available.clone();
            let urn = urn.clone();
            let store = store.clone();
            let on_change = on_change.clone();

            move || {
                let apps: Vec<(Urn, entity::app::App)> =
                    store.get_entities_typed(entity::app::ENTITY_TYPE);

                let settings_urn = find_settings_app_urn(&apps);
                let now_available = settings_urn.is_some();
                let was_available = available.get();

                *urn.borrow_mut() = settings_urn;
                available.set(now_available);

                if was_available != now_available {
                    on_change(now_available);
                }
            }
        };

        store.subscribe_type(entity::app::ENTITY_TYPE, reconcile.clone());
        glib::idle_add_local_once(reconcile);

        Self { available, urn }
    }

    /// Whether the waft-settings app is currently available.
    pub fn is_available(&self) -> bool {
        self.available.get()
    }

    /// Current waft-settings URN, if available.
    pub fn urn(&self) -> Option<Urn> {
        self.urn.borrow().clone()
    }

    /// Build a settings button that dispatches `open-page` to waft-settings.
    ///
    /// The button uses the tracker's stored URN at click time, so it remains
    /// correct if the URN changes between construction and click.
    pub fn build_settings_button(
        &self,
        action_callback: &EntityActionCallback,
        page: &'static str,
        label: String,
    ) -> FeatureToggleMenuSettingsButton {
        let button = FeatureToggleMenuSettingsButton::new(FeatureToggleMenuSettingsButtonProps {
            label,
        });

        let urn_ref = self.urn.clone();
        let cb = action_callback.clone();
        button.on_click(move |_| {
            if let Some(ref urn) = *urn_ref.borrow() {
                cb(
                    urn.clone(),
                    "open-page".to_string(),
                    serde_json::json!({ "page": page }),
                );
            }
        });

        button
    }
}

/// Find the waft-settings app entity URN.
///
/// Returns `Some(urn)` only for `internal-apps/app/waft-settings`.
pub fn find_settings_app_urn(apps: &[(Urn, entity::app::App)]) -> Option<Urn> {
    apps.iter()
        .find(|(urn, _)| urn.plugin() == "internal-apps" && urn.id() == "waft-settings")
        .map(|(urn, _)| urn.clone())
}

// ... tests block from Step 1 goes here
```

**Step 4: Add to mod.rs**

```rust
pub mod settings_app_tracker;
```

**Step 5: Run tests**

```bash
cargo test -p waft-overview settings_app_tracker 2>&1
```
Expected: 4 tests pass.

**Step 6: Commit**

```bash
git add crates/overview/src/components/toggles/settings_app_tracker.rs \
        crates/overview/src/components/toggles/mod.rs
git commit -m "feat(overview): add SettingsAppTracker for settings app availability"
```

---

### Task 8: Migrate `bluetooth.rs` to use `SettingsAppTracker`

**Files:**
- Modify: `crates/overview/src/components/toggles/bluetooth.rs`

**Step 1: Add import**

At the top of `bluetooth.rs`, add alongside existing imports:
```rust
use crate::components::toggles::settings_app_tracker::SettingsAppTracker;
```

**Step 2: Remove the local `build_settings_button` free function**

Delete the entire `fn build_settings_button(...)` function (lines ~63–83).

**Step 3: Replace settings tracking in `BluetoothToggles::new`**

Find and remove:
- `let settings_available: Rc<Cell<bool>> = Rc::new(Cell::new(false));`
- `let settings_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));`
- The entire `// Subscribe to app entity type for settings availability` block (roughly lines 217–277), including both `subscribe_type` and `idle_add_local_once`.

Also remove the `Rc<RefCell<Option<Urn>>>` and related imports (`Cell`, `RefCell`) if they're no longer used elsewhere in the file.

Replace the removed settings availability block with a `SettingsAppTracker`:

```rust
// Track settings app availability and update all adapter toggles when it changes.
let _settings_tracker = {
    let entries_ref = entries.clone();
    SettingsAppTracker::new(store, move |is_available| {
        let entries_borrowed = entries_ref.borrow();
        for entry in entries_borrowed.iter() {
            entry.settings_separator.set_visible(is_available);
            entry.settings_button.set_visible(is_available);
            let has_devices = !entry.device_rows.borrow().is_empty();
            entry.toggle.set_expandable(has_devices || is_available);
        }
    })
};
```

> Note: `_settings_tracker` keeps the tracker alive for the lifetime of `BluetoothToggles::new`.
> You do NOT need to store it in the struct — the subscriptions are registered on the `EntityStore`
> which is already kept alive by the struct.

**Step 4: Replace uses of `settings_urn_for_adapter` in `build_settings_button` calls**

In the adapter subscription closure, find:
```rust
let settings_button = build_settings_button(&cb, &settings_urn_for_adapter);
```

This now needs to be replaced. But at this point in the code, we don't have a `SettingsAppTracker` reference in the closure. The simplest approach: create the tracker *before* the adapter subscription and capture a clone of its `urn` field. However, `SettingsAppTracker` doesn't expose `urn` as a public field.

Instead, move the tracker creation to before the adapter subscription block and pass a reference via `Rc`:

```rust
// Create tracker before adapter subscription so adapter closures can use the URN.
let settings_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));
let settings_available: Rc<Cell<bool>> = Rc::new(Cell::new(false));
```

Then build the settings button inside the adapter loop using these Rcs directly (same as before), and use `SettingsAppTracker::new` only for the subscription/reconciliation:

```rust
let _settings_tracker = {
    let entries_ref = entries.clone();
    let settings_available_ref = settings_available.clone();
    let settings_urn_ref = settings_urn.clone();
    SettingsAppTracker::new(store, move |is_available| {
        // Update stored values for button dispatch
        // NOTE: tracker internally stores the urn; we sync our local copy here
        // by re-querying... actually we need a different approach.
    })
};
```

> **Simpler approach:** Keep the `settings_urn: Rc<RefCell<Option<Urn>>>` and `settings_available: Rc<Cell<bool>>` Rcs as local state. The `SettingsAppTracker::new` callback updates them. Replace only the duplicate subscription+reconciliation block (not the button construction).

Concretely, replace the `// Subscribe to app entity type for settings availability` block with:

```rust
{
    let entries_ref = entries.clone();
    let settings_available_ref = settings_available.clone();
    let settings_urn_ref = settings_urn.clone();

    let _tracker = SettingsAppTracker::new(store, move |is_available| {
        settings_available_ref.set(is_available);
        // Note: urn update happens inside the tracker; replicate for local button dispatch:
        // We need the URN here too. See below.
        let entries_borrowed = entries_ref.borrow();
        for entry in entries_borrowed.iter() {
            entry.settings_separator.set_visible(is_available);
            entry.settings_button.set_visible(is_available);
            let has_devices = !entry.device_rows.borrow().is_empty();
            entry.toggle.set_expandable(has_devices || is_available);
        }
    });
}
```

However, the local `settings_urn` Rc still needs to be updated when the app entity changes, because it is used in the `build_settings_button` closure at button-click time. The tracker handles this internally for buttons it creates — but since bluetooth builds its own buttons using the local `settings_urn`, we need to keep updating it.

**Alternative (cleaner):** Switch bluetooth button construction to use `tracker.build_settings_button(...)`. For this, create the tracker *before* the adapter loop and pass it in. Since the adapter loop runs inside a `subscribe_type` closure (not at construction time), the tracker can be captured by `Rc`:

```rust
let settings_tracker: Rc<SettingsAppTracker> = {
    let entries_ref = entries.clone();
    Rc::new(SettingsAppTracker::new(store, move |is_available| {
        let entries_borrowed = entries_ref.borrow();
        for entry in entries_borrowed.iter() {
            entry.settings_separator.set_visible(is_available);
            entry.settings_button.set_visible(is_available);
            let has_devices = !entry.device_rows.borrow().is_empty();
            entry.toggle.set_expandable(has_devices || is_available);
        }
    }))
};
```

Then, in the adapter subscription closure, replace `build_settings_button(&cb, &settings_urn_for_adapter)` with:
```rust
let settings_button = settings_tracker.build_settings_button(
    &cb,
    "bluetooth",
    crate::i18n::t("bluetooth-settings-button"),
);
```

And use `settings_tracker.is_available()` wherever `settings_available_for_adapter.get()` was used.

Remove: `settings_available`, `settings_urn`, all `settings_*_for_*` clones, the old subscribe+reconcile block.

**Step 5: Build**

```bash
cargo build -p waft-overview 2>&1 | head -40
```
Expected: clean build. If there are unused import warnings for `Cell`, `RefCell`, or `Urn`-related imports, remove them.

**Step 6: Run tests**

```bash
cargo test -p waft-overview 2>&1
```
Expected: all tests pass.

**Step 7: Commit**

```bash
git add crates/overview/src/components/toggles/bluetooth.rs
git commit -m "refactor(overview/bluetooth): use SettingsAppTracker, fix settings URN lookup"
```

---

### Task 9: Migrate `network/mod.rs` to use `SettingsAppTracker`

**Files:**
- Modify: `crates/overview/src/components/toggles/network/mod.rs`

**Step 1: Add import**

```rust
use crate::components::toggles::settings_app_tracker::SettingsAppTracker;
```

**Step 2: Remove duplicate items**

Delete:
- The `find_settings_app_urn` free function (bottom of file, lines ~597–605)
- The 4 unit tests in `mod tests` for `find_settings_app_urn` (they now live in `settings_app_tracker`)

**Step 3: Replace the settings app subscription block**

Find the `// Subscribe to app entity changes (for settings button visibility).` block (~lines 431–466). It contains a `reconcile` closure, `subscribe_type`, and `idle_add_local_once`.

Replace the entire block with:

```rust
// Track settings app availability for settings button visibility.
let settings_tracker: Rc<SettingsAppTracker> = {
    let entries_ref = entries.clone();
    let settings_available_ref = settings_available.clone();
    let settings_urn_ref = settings_urn.clone();

    Rc::new(SettingsAppTracker::new(store, move |is_available| {
        settings_available_ref.set(is_available);
        let entries = entries_ref.borrow();
        for entry in entries.iter() {
            if let Some(ref btn) = entry.settings_button {
                btn.set_visible(is_available);
                let has_info = !entry.info_rows.borrow().is_empty();
                let has_children = !entry.network_rows.borrow().is_empty();
                entry
                    .toggle
                    .set_expandable(has_info || has_children || is_available);
            }
        }
    }))
};
```

**Step 4: Replace `build_settings_button` calls**

In `network/mod.rs`, the `build_settings_button` free function is called to create per-adapter buttons. Replace the two calls (wired and wireless cases) with tracker calls:

Find (wired case):
```rust
let btn = build_settings_button(
    &settings_urn_ref,
    &cb,
    "wired",
    "wired-settings-button",
);
```
Replace with:
```rust
let btn = settings_tracker.build_settings_button(
    &cb,
    "wired",
    crate::i18n::t("wired-settings-button"),
);
```

Find (wireless case):
```rust
let btn = build_settings_button(
    &settings_urn_ref,
    &cb,
    "wifi",
    "wifi-settings-button",
);
```
Replace with:
```rust
let btn = settings_tracker.build_settings_button(
    &cb,
    "wifi",
    crate::i18n::t("wifi-settings-button"),
);
```

**Step 5: Remove the old `build_settings_button` free function**

Delete `fn build_settings_button(...)` (bottom of file, ~lines 611–635).

Remove now-unused variables: `settings_urn_ref` (used only in the old `build_settings_button` calls), and any dead-code `settings_urn` let-bindings if they're no longer referenced. Check if `settings_urn` and `settings_available` Rcs are still needed for the `on_change` closure — they are if you kept them above. If the tracker fully replaces them, remove them and their clones.

**Step 6: Build and test**

```bash
cargo build -p waft-overview 2>&1 | head -40
cargo test -p waft-overview 2>&1
```
Expected: clean build, all tests pass. The `find_settings_app_urn` tests now run from `settings_app_tracker`.

**Step 7: Commit**

```bash
git add crates/overview/src/components/toggles/network/mod.rs
git commit -m "refactor(overview/network): use SettingsAppTracker, remove duplicated app tracking"
```

---

### Task 10: Final verification

**Step 1: Full workspace build**

```bash
cargo build --workspace 2>&1 | tail -5
```
Expected: `Finished` with no errors or warnings about unused code.

**Step 2: Full test suite**

```bash
cargo test --workspace 2>&1 | tail -20
```
Expected: all tests pass, no regressions.

**Step 3: Check for leftover dead code**

```bash
cargo build --workspace 2>&1 | grep -E "warning.*unused|warning.*dead_code"
```
Expected: no new warnings (pre-existing `#[allow(dead_code)]` annotations in bluetooth/network are fine).

**Step 4: Final commit if any cleanup needed**

```bash
git add -p   # review any remaining changes
git commit -m "chore(overview): remove unused imports after toggle consolidation"
```
