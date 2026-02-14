# Display Settings Page Implementation Plan

**Status:** Implemented

**Goal:** Add a Display settings page to waft-settings with brightness sliders, dark mode toggle, and night light (sunsetr) controls.

**Architecture:** Three independent smart containers (`BrightnessSection`, `DarkModeSection`, `NightLightSection`) composed by a thin `DisplayPage`. Each section subscribes to its own entity type via `EntityStore`, reconciles its widgets, and routes user actions back to the daemon. Follows the same dumb-widget + smart-container pattern as Bluetooth/WiFi/Wired pages.

**Tech Stack:** GTK4, libadwaita (AdwPreferencesGroup, AdwSwitchRow, AdwComboRow, AdwActionRow), waft-client (EntityStore, EntityActionCallback), waft-protocol entity types.

---

## Reference: Entity Types & Actions

### Display (brightness)
- **Entity type:** `"display"` (`entity::display::DISPLAY_ENTITY_TYPE`)
- **Struct:** `Display { name: String, brightness: f64, kind: DisplayKind }`
- **URN pattern:** `brightness/display/{device-id}` (0..N entities)
- **Action:** `"set-brightness"` with params `{"value": f64}` (0.0–1.0)

### Dark Mode
- **Entity type:** `"dark-mode"` (`entity::display::DARK_MODE_ENTITY_TYPE`)
- **Struct:** `DarkMode { active: bool }`
- **URN:** `darkman/dark-mode/default` (0..1 entity)
- **Action:** `"toggle"` with `Value::Null`

### Night Light (sunsetr)
- **Entity type:** `"night-light"` (`entity::display::NIGHT_LIGHT_ENTITY_TYPE`)
- **Struct:** `NightLight { active: bool, period: Option<String>, next_transition: Option<String>, presets: Vec<String>, active_preset: Option<String> }`
- **URN:** `sunsetr/night-light/default` (0..1 entity)
- **Actions:**
  - `"toggle"` with `Value::Null`
  - `"select_preset"` with `Value::String(preset_name)` (**raw string**, not object)

## Reference: File Patterns

All dumb widgets follow the pattern in `crates/settings/src/bluetooth/adapter_group.rs`:
- `Props` struct for input data
- `Output` enum for events
- `OutputCallback = Rc<RefCell<Option<Box<dyn Fn(Output)>>>>`
- `new(props)` → create widget, wire signals, call `apply_props`
- `apply_props(props)` → update widget state with `updating` guard
- `connect_output(callback)` → register output handler

Smart containers follow the pattern in `crates/settings/src/pages/bluetooth.rs`:
- `pub root: gtk::Box`
- Internal state in `Rc<RefCell<State>>`
- `subscribe_type()` per entity type
- Reconciliation: HashSet-based create/update/remove cycle

---

## Task 1: Create `display/mod.rs` module declaration

**Files:**
- Create: `crates/settings/src/display/mod.rs`

**Step 1: Create the module file**

```rust
pub mod brightness_section;
pub mod dark_mode_section;
pub mod night_light_section;
```

**Step 2: Register module in `main.rs`**

Modify `crates/settings/src/main.rs` — add `mod display;` after line 4 (`mod bluetooth;`):

```rust
mod app;
mod bluetooth;
mod display;
mod pages;
mod wifi;
mod wired;
mod sidebar;
mod window;
```

**Step 3: Verify it compiles**

Run: `cargo check -p waft-settings 2>&1 | head -20`
Expected: Errors about missing files (brightness_section.rs, etc.) — that's fine for now. No syntax errors in mod.rs itself.

---

## Task 2: Create `NightLightSection` smart container

This is the primary new component. It subscribes to `night-light` entities and provides toggle, preset selector, and status display.

**Files:**
- Create: `crates/settings/src/display/night_light_section.rs`

**Step 1: Write the night light section**

```rust
//! Night light settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `night-light` entity type.
//! Provides toggle, preset selection, and status display.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::display::{NIGHT_LIGHT_ENTITY_TYPE, NightLight};

/// Smart container for night light settings.
pub struct NightLightSection {
    pub root: adw::PreferencesGroup,
}

impl NightLightSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Night Light")
            .visible(false)
            .build();

        // Toggle row
        let toggle_row = adw::SwitchRow::builder()
            .title("Night Light")
            .build();
        group.add(&toggle_row);

        // Preset combo row (visible only when active and presets available)
        let preset_model = gtk::StringList::new(&[]);
        let preset_row = adw::ComboRow::builder()
            .title("Color Preset")
            .model(&preset_model)
            .visible(false)
            .build();
        group.add(&preset_row);

        // Status row (visible only when active)
        let status_row = adw::ActionRow::builder()
            .title("Status")
            .visible(false)
            .build();
        group.add(&status_row);

        // Feedback loop guard
        let updating = Rc::new(Cell::new(false));

        // Current URN (set on first entity arrival)
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));
        // Current preset list (for index → name mapping)
        let current_presets: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        // Wire toggle
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            toggle_row.connect_active_notify(move |_row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    cb(urn.clone(), "toggle".to_string(), serde_json::Value::Null);
                }
            });
        }

        // Wire preset selection
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            let presets_ref = current_presets.clone();
            preset_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                let idx = row.selected() as usize;
                let presets = presets_ref.borrow();
                // Index 0 = "Default", rest are 1-indexed into presets vec
                let preset_value = if idx == 0 {
                    "default".to_string()
                } else if let Some(name) = presets.get(idx - 1) {
                    name.clone()
                } else {
                    return;
                };
                if let Some(ref urn) = *urn_ref.borrow() {
                    cb(
                        urn.clone(),
                        "select_preset".to_string(),
                        serde_json::Value::String(preset_value),
                    );
                }
            });
        }

        // Subscribe to night-light entities
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let toggle_ref = toggle_row.clone();
            let preset_row_ref = preset_row.clone();
            let preset_model_ref = preset_model.clone();
            let status_row_ref = status_row.clone();
            let urn_ref = current_urn;
            let presets_ref = current_presets;
            let guard = updating;

            entity_store.subscribe_type(NIGHT_LIGHT_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, NightLight)> =
                    store.get_entities_typed(NIGHT_LIGHT_ENTITY_TYPE);

                if let Some((urn, nl)) = entities.first() {
                    guard.set(true);

                    // Store current URN
                    *urn_ref.borrow_mut() = Some(urn.clone());

                    // Show the group
                    group_ref.set_visible(true);

                    // Update toggle
                    toggle_ref.set_active(nl.active);

                    // Update toggle subtitle with period info
                    if let Some(ref period) = nl.period {
                        let label = match period.as_str() {
                            "day" => "Day",
                            "night" => "Night",
                            other => other,
                        };
                        toggle_ref.set_subtitle(label);
                    } else {
                        toggle_ref.set_subtitle("");
                    }

                    // Update presets
                    let has_presets = !nl.presets.is_empty();
                    preset_row_ref.set_visible(nl.active && has_presets);

                    // Rebuild preset model if presets changed
                    let prev_presets = presets_ref.borrow();
                    if *prev_presets != nl.presets {
                        drop(prev_presets);

                        // Clear and rebuild model: "Default" + preset names
                        let count = preset_model_ref.n_items();
                        if count > 0 {
                            preset_model_ref.splice(0, count, &[] as &[&str]);
                        }
                        preset_model_ref.append("Default");
                        for preset in &nl.presets {
                            preset_model_ref.append(preset);
                        }

                        *presets_ref.borrow_mut() = nl.presets.clone();
                    }

                    // Select current preset in combo
                    let selected_idx = match &nl.active_preset {
                        Some(name) => {
                            let presets = presets_ref.borrow();
                            presets
                                .iter()
                                .position(|p| p == name)
                                .map(|i| (i + 1) as u32) // +1 for "Default" entry
                                .unwrap_or(0)
                        }
                        None => 0, // "Default"
                    };
                    preset_row_ref.set_selected(selected_idx);

                    // Update status row
                    if nl.active {
                        if let Some(ref next) = nl.next_transition {
                            status_row_ref.set_subtitle(next);
                            status_row_ref.set_title("Next Transition");
                            status_row_ref.set_visible(true);
                        } else {
                            status_row_ref.set_visible(false);
                        }
                    } else {
                        status_row_ref.set_visible(false);
                    }

                    guard.set(false);
                } else {
                    // No night-light entity — hide everything
                    group_ref.set_visible(false);
                }
            });
        }

        Self { root: group }
    }
}
```

**Step 2: Verify it compiles (after Task 1 is done)**

Run: `cargo check -p waft-settings 2>&1 | head -30`
Expected: May fail on missing sibling modules. Fix by creating stubs if needed.

---

## Task 3: Create `DarkModeSection` smart container

**Files:**
- Create: `crates/settings/src/display/dark_mode_section.rs`

**Step 1: Write the dark mode section**

```rust
//! Dark mode settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `dark-mode` entity type.
//! Provides a single toggle switch.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::display::{DARK_MODE_ENTITY_TYPE, DarkMode};

/// Smart container for dark mode settings.
pub struct DarkModeSection {
    pub root: adw::PreferencesGroup,
}

impl DarkModeSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Appearance")
            .visible(false)
            .build();

        let toggle_row = adw::SwitchRow::builder()
            .title("Dark Mode")
            .build();
        group.add(&toggle_row);

        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Wire toggle
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            toggle_row.connect_active_notify(move |_row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    cb(urn.clone(), "toggle".to_string(), serde_json::Value::Null);
                }
            });
        }

        // Subscribe to dark-mode entities
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let toggle_ref = toggle_row;
            let urn_ref = current_urn;
            let guard = updating;

            entity_store.subscribe_type(DARK_MODE_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, DarkMode)> =
                    store.get_entities_typed(DARK_MODE_ENTITY_TYPE);

                if let Some((urn, dm)) = entities.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    toggle_ref.set_active(dm.active);
                    guard.set(false);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        Self { root: group }
    }
}
```

---

## Task 4: Create `BrightnessSection` smart container

**Files:**
- Create: `crates/settings/src/display/brightness_section.rs`

**Step 1: Write the brightness section**

This section uses HashMap-based reconciliation since there can be 0..N displays.

```rust
//! Brightness settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `display` entity type.
//! Renders one preferences group per display with a brightness slider.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::display::{DISPLAY_ENTITY_TYPE, Display, DisplayKind};

/// Smart container for brightness settings.
pub struct BrightnessSection {
    pub root: gtk::Box,
}

/// Tracks a single display's widgets.
struct DisplayWidgets {
    group: adw::PreferencesGroup,
    scale: gtk::Scale,
    updating: Rc<Cell<bool>>,
}

impl BrightnessSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .visible(false)
            .build();

        let displays: Rc<RefCell<HashMap<String, DisplayWidgets>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Subscribe to display entities
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let root_ref = root.clone();
            let displays_ref = displays;

            entity_store.subscribe_type(DISPLAY_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, Display)> =
                    store.get_entities_typed(DISPLAY_ENTITY_TYPE);

                let mut map = displays_ref.borrow_mut();
                let mut seen = HashSet::new();

                for (urn, display) in &entities {
                    let urn_str = urn.as_str().to_string();
                    seen.insert(urn_str.clone());

                    if let Some(existing) = map.get(&urn_str) {
                        // Update existing
                        existing.updating.set(true);
                        existing.group.set_title(&display.name);
                        existing.scale.set_value(display.brightness);
                        let subtitle = match display.kind {
                            DisplayKind::Backlight => "Built-in display",
                            DisplayKind::External => "External display",
                        };
                        existing.group.set_description(Some(subtitle));
                        existing.updating.set(false);
                    } else {
                        // Create new display group
                        let subtitle = match display.kind {
                            DisplayKind::Backlight => "Built-in display",
                            DisplayKind::External => "External display",
                        };
                        let group = adw::PreferencesGroup::builder()
                            .title(&display.name)
                            .description(subtitle)
                            .build();

                        let scale = gtk::Scale::builder()
                            .orientation(gtk::Orientation::Horizontal)
                            .hexpand(true)
                            .draw_value(false)
                            .build();
                        scale.set_range(0.0, 1.0);
                        scale.set_increments(0.05, 0.1);
                        scale.set_value(display.brightness);

                        let row = adw::ActionRow::builder()
                            .title("Brightness")
                            .build();
                        row.add_suffix(&scale);
                        group.add(&row);

                        let updating = Rc::new(Cell::new(false));

                        // Wire slider
                        let urn_clone = urn.clone();
                        let cb_clone = cb.clone();
                        let guard = updating.clone();
                        scale.connect_value_changed(move |s| {
                            if guard.get() {
                                return;
                            }
                            cb_clone(
                                urn_clone.clone(),
                                "set-brightness".to_string(),
                                serde_json::json!({ "value": s.value() }),
                            );
                        });

                        root_ref.append(&group);
                        map.insert(
                            urn_str,
                            DisplayWidgets {
                                group,
                                scale,
                                updating,
                            },
                        );
                    }
                }

                // Remove displays no longer present
                let to_remove: Vec<String> = map
                    .keys()
                    .filter(|k| !seen.contains(*k))
                    .cloned()
                    .collect();

                for key in to_remove {
                    if let Some(widgets) = map.remove(&key) {
                        root_ref.remove(&widgets.group);
                    }
                }

                // Show/hide the whole section
                root_ref.set_visible(!map.is_empty());
            });
        }

        Self { root }
    }
}
```

---

## Task 5: Create `DisplayPage` composer and wire into the app

**Files:**
- Create: `crates/settings/src/pages/display.rs`
- Modify: `crates/settings/src/pages/mod.rs`
- Modify: `crates/settings/src/app.rs` (lines 11-25)
- Modify: `crates/settings/src/window.rs` (lines 10-13, 44-69)
- Modify: `crates/settings/src/sidebar.rs` (line 66)

**Step 1: Create the display page composer**

Create `crates/settings/src/pages/display.rs`:

```rust
//! Display settings page -- thin composer.
//!
//! Composes three independent smart containers: brightness, dark mode,
//! and night light sections into a single scrollable page.

use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::brightness_section::BrightnessSection;
use crate::display::dark_mode_section::DarkModeSection;
use crate::display::night_light_section::NightLightSection;

/// Display settings page composed of independent sections.
pub struct DisplayPage {
    pub root: gtk::Box,
}

impl DisplayPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let brightness = BrightnessSection::new(entity_store, action_callback);
        root.append(&brightness.root);

        let dark_mode = DarkModeSection::new(entity_store, action_callback);
        root.append(&dark_mode.root);

        let night_light = NightLightSection::new(entity_store, action_callback);
        root.append(&night_light.root);

        Self { root }
    }
}
```

**Step 2: Register in `pages/mod.rs`**

Add to `crates/settings/src/pages/mod.rs`:

```rust
pub mod bluetooth;
pub mod display;
pub mod wifi;
pub mod wired;
```

**Step 3: Add entity types to `app.rs`**

In `crates/settings/src/app.rs`, add display entity imports and extend `ENTITY_TYPES`:

Add import (after line 14):
```rust
use waft_protocol::entity::display::{
    DARK_MODE_ENTITY_TYPE, DISPLAY_ENTITY_TYPE, NIGHT_LIGHT_ENTITY_TYPE,
};
```

Extend `ENTITY_TYPES` (lines 19-25):
```rust
const ENTITY_TYPES: &[&str] = &[
    BluetoothAdapter::ENTITY_TYPE,
    BluetoothDevice::ENTITY_TYPE,
    ADAPTER_ENTITY_TYPE,
    WiFiNetwork::ENTITY_TYPE,
    EthernetConnection::ENTITY_TYPE,
    DISPLAY_ENTITY_TYPE,
    DARK_MODE_ENTITY_TYPE,
    NIGHT_LIGHT_ENTITY_TYPE,
];
```

**Step 4: Wire display page in `window.rs`**

Add import (after line 13):
```rust
use crate::pages::display::DisplayPage;
```

After the `wired_page` creation (line 46), add:
```rust
let display_page = DisplayPage::new(entity_store, action_callback);
```

After the `wired_clamp` (line 60), add:
```rust
let display_clamp = adw::Clamp::builder()
    .maximum_size(600)
    .child(&display_page.root)
    .build();
```

After `stack.add_named(&wired_clamp, Some("Wired"));` (line 68), add:
```rust
stack.add_named(&display_clamp, Some("Display"));
```

**Step 5: Enable Display sidebar row**

In `crates/settings/src/sidebar.rs`, change line 66 from:
```rust
            .sensitive(false)
```
to:
```rust
            .sensitive(true)
```

**Step 6: Build and verify**

Run: `cargo build -p waft-settings 2>&1 | tail -5`
Expected: Build succeeds.

**Step 7: Commit**

```bash
git add crates/settings/src/display/ crates/settings/src/pages/display.rs
git add crates/settings/src/pages/mod.rs crates/settings/src/main.rs
git add crates/settings/src/app.rs crates/settings/src/window.rs
git add crates/settings/src/sidebar.rs
git commit -m "feat(settings): add Display page with brightness, dark mode, and night light sections"
```

---

## Task 6: Verify full workspace builds and tests pass

**Step 1: Build the workspace**

Run: `cargo build --workspace 2>&1 | tail -5`
Expected: Build succeeds.

**Step 2: Run tests**

Run: `cargo test --workspace 2>&1 | tail -10`
Expected: All tests pass.

**Step 3: Run clippy**

Run: `cargo clippy -p waft-settings 2>&1 | tail -20`
Expected: No warnings.

---

## Summary of all files

### New files (5) — all created:
- `crates/settings/src/display/mod.rs` ✅
- `crates/settings/src/display/brightness_section.rs` ✅
- `crates/settings/src/display/dark_mode_section.rs` ✅
- `crates/settings/src/display/night_light_section.rs` ✅
- `crates/settings/src/pages/display.rs` ✅

### Modified files (5) — all done:
- `crates/settings/src/main.rs` — added `mod display;` ✅
- `crates/settings/src/pages/mod.rs` — added `pub mod display;` ✅
- `crates/settings/src/app.rs` — added display entity type imports and extended `ENTITY_TYPES` ✅
- `crates/settings/src/window.rs` — created `DisplayPage`, wrap in clamp, add to stack ✅
- `crates/settings/src/sidebar.rs` — Display row active (was already `sensitive(true)` in new sidebar) ✅
