# Implementation Plan: Per-Display Resolution Management in Waft Settings

## Context

The user wants to add per-display resolution management to the waft settings application. This will allow users to:
- View connected displays with their manufacturer/model information
- Change resolution and refresh rate for each display
- Toggle Variable Refresh Rate (VRR) when supported by the hardware

The niri plugin already provides all the necessary backend functionality through the `DisplayOutput` entity type. The entity includes display identification (name, make, model), available display modes (resolution + refresh rate combinations), current mode, and VRR support/state. The plugin handles `set-mode` and `toggle-vrr` actions.

The Display settings page currently exists as a "thin composer" pattern (43 lines) containing only brightness, dark mode, and night light sections. This plan adds a fourth section for display output/resolution management.

## Implementation Approach

### Architecture Pattern

Follow the **BrightnessSection smart container pattern**:
- Smart container subscribes to `EntityStore` for `display-output` entity type
- One preferences group per display output (URN-keyed HashMap)
- Inline widget creation (no separate dumb widget files needed)
- Standard reconciliation with add/update/remove logic
- Root container visibility tied to entity presence

### UI Design

Each display will show:
```
[Preference Group: "Samsung Electric Company LS49AG95"]
  Description: "Output: DP-3"

  [Resolution]
    [DropDown: "5120×1440 @ 239.76 Hz (Preferred)"]

  [Variable Refresh Rate]  ← Only if vrr_supported
    [Switch: ON/OFF]
```

**Mode Display Format:** `{width}×{height} @ {refresh_rate} Hz [" (Preferred)"]`
- Use multiplication sign (×, U+00D7) not 'x'
- Format refresh rate to 2 decimal places
- Append " (Preferred)" for the preferred mode

### Critical Files

**New File:**
- `/home/just-paja/Work/shell/sacrebleui/crates/settings/src/display/output_section.rs` (new ~180 lines)

**Modified Files:**
- `/home/just-paja/Work/shell/sacrebleui/crates/settings/src/pages/display.rs` (add OutputSection instantiation)
- `/home/just-paja/Work/shell/sacrebleui/crates/settings/src/display/mod.rs` (export output_section module)

**Reference Files:**
- `/home/just-paja/Work/shell/sacrebleui/crates/protocol/src/entity/display.rs` (DisplayOutput, DisplayMode definitions)
- `/home/just-paja/Work/shell/sacrebleui/crates/settings/src/display/brightness_section.rs` (pattern reference)

## Detailed Implementation Steps

### Step 1: Create `output_section.rs` Structure

Create `/home/just-paja/Work/shell/sacrebleui/crates/settings/src/display/output_section.rs`:

```rust
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::display::{DISPLAY_OUTPUT_ENTITY_TYPE, DisplayOutput, DisplayMode};

pub struct OutputSection {
    pub root: gtk::Box,
}

struct OutputGroupWidgets {
    group: adw::PreferencesGroup,
    mode_dropdown: gtk::DropDown,
    vrr_row: adw::SwitchRow,
    updating: Rc<Cell<bool>>,
}
```

### Step 2: Implement Mode Formatting Helper

Add helper function to format display modes:

```rust
fn format_mode(mode: &DisplayMode) -> String {
    let hz = format!("{:.2}", mode.refresh_rate);
    let preferred = if mode.preferred { " (Preferred)" } else { "" };
    format!("{}×{} @ {} Hz{}", mode.width, mode.height, hz, preferred)
}
```

### Step 3: Implement Constructor and Subscription

The constructor creates the root container and subscribes to `DISPLAY_OUTPUT_ENTITY_TYPE`:

```rust
impl OutputSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .visible(false)
            .build();

        let outputs: Rc<RefCell<HashMap<String, OutputGroupWidgets>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Subscribe to display-output entities
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let root_ref = root.clone();
            let outputs_ref = outputs.clone();

            entity_store.subscribe_type(DISPLAY_OUTPUT_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                Self::reconcile(&outputs_ref, &root_ref, &entities, &cb);
            });
        }

        // Initial reconciliation for cached entities
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let root_ref = root.clone();
            let outputs_ref = outputs;

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!("[output-section] Initial reconciliation: {} outputs", entities.len());
                    Self::reconcile(&outputs_ref, &root_ref, &entities, &cb);
                }
            });
        }

        Self { root }
    }
}
```

### Step 4: Implement Reconciliation Logic

The reconciliation method handles add/update/remove of display output groups:

```rust
fn reconcile(
    outputs_map: &Rc<RefCell<HashMap<String, OutputGroupWidgets>>>,
    root: &gtk::Box,
    entities: &[(Urn, DisplayOutput)],
    action_callback: &EntityActionCallback,
) {
    let mut map = outputs_map.borrow_mut();
    let mut seen = HashSet::new();

    for (urn, output) in entities {
        let urn_str = urn.as_str().to_string();
        seen.insert(urn_str.clone());

        if let Some(existing) = map.get(&urn_str) {
            // Update existing widgets
            existing.updating.set(true);

            // Update group title/description
            let title = if output.make.is_empty() && output.model.is_empty() {
                output.name.clone()
            } else if output.make.is_empty() {
                output.model.clone()
            } else if output.model.is_empty() {
                output.make.clone()
            } else {
                format!("{} {}", output.make, output.model)
            };
            existing.group.set_title(&title);
            existing.group.set_description(Some(&format!("Output: {}", output.name)));

            // Find current mode index
            let current_idx = output.available_modes.iter()
                .position(|m| m == &output.current_mode)
                .unwrap_or(0);

            // Update dropdown
            let string_list = gtk::StringList::new(&[]);
            for mode in &output.available_modes {
                string_list.append(&format_mode(mode));
            }
            existing.mode_dropdown.set_model(Some(&string_list));
            existing.mode_dropdown.set_selected(current_idx as u32);

            // Update VRR
            existing.vrr_row.set_visible(output.vrr_supported);
            existing.vrr_row.set_active(output.vrr_enabled);

            existing.updating.set(false);
        } else {
            // Create new group
            let widgets = Self::create_output_group(urn, output, action_callback);
            root.append(&widgets.group);
            map.insert(urn_str, widgets);
        }
    }

    // Remove stale groups
    let to_remove: Vec<String> = map.keys()
        .filter(|k| !seen.contains(*k))
        .cloned()
        .collect();
    for key in to_remove {
        if let Some(widgets) = map.remove(&key) {
            root.remove(&widgets.group);
        }
    }

    // Update root visibility
    root.set_visible(!map.is_empty());
}
```

### Step 5: Implement Widget Creation

Create the preferences group, dropdown, and VRR switch for each output:

```rust
fn create_output_group(
    urn: &Urn,
    output: &DisplayOutput,
    action_callback: &EntityActionCallback,
) -> OutputGroupWidgets {
    let title = if output.make.is_empty() && output.model.is_empty() {
        output.name.clone()
    } else if output.make.is_empty() {
        output.model.clone()
    } else if output.model.is_empty() {
        output.make.clone()
    } else {
        format!("{} {}", output.make, output.model)
    };

    let group = adw::PreferencesGroup::builder()
        .title(&title)
        .description(&format!("Output: {}", output.name))
        .build();

    // Find current mode index
    let current_idx = output.available_modes.iter()
        .position(|m| m == &output.current_mode)
        .unwrap_or(0);

    // Create mode dropdown
    let string_list = gtk::StringList::new(&[]);
    for mode in &output.available_modes {
        string_list.append(&format_mode(mode));
    }

    let mode_dropdown = gtk::DropDown::builder()
        .model(&string_list)
        .selected(current_idx as u32)
        .build();

    let mode_row = adw::ActionRow::builder()
        .title("Resolution")
        .build();
    mode_row.add_suffix(&mode_dropdown);
    group.add(&mode_row);

    // Wire mode dropdown callback
    let updating = Rc::new(Cell::new(false));
    {
        let urn_clone = urn.clone();
        let cb = action_callback.clone();
        let guard = updating.clone();
        mode_dropdown.connect_selected_notify(move |dropdown| {
            if guard.get() { return; }
            let idx = dropdown.selected() as usize;
            cb(
                urn_clone.clone(),
                "set-mode".to_string(),
                serde_json::json!({ "mode_index": idx }),
            );
        });
    }

    // Create VRR switch
    let vrr_row = adw::SwitchRow::builder()
        .title("Variable Refresh Rate")
        .visible(output.vrr_supported)
        .active(output.vrr_enabled)
        .build();
    group.add(&vrr_row);

    // Wire VRR callback
    {
        let urn_clone = urn.clone();
        let cb = action_callback.clone();
        let guard = updating.clone();
        vrr_row.connect_active_notify(move |_row| {
            if guard.get() { return; }
            cb(
                urn_clone.clone(),
                "toggle-vrr".to_string(),
                serde_json::Value::Null,
            );
        });
    }

    OutputGroupWidgets {
        group,
        mode_dropdown,
        vrr_row,
        updating,
    }
}
```

### Step 6: Update Display Module Exports

Edit `/home/just-paja/Work/shell/sacrebleui/crates/settings/src/display/mod.rs`:

```rust
pub mod brightness_section;
pub mod dark_mode_section;
pub mod night_light_section;
pub mod output_section;  // NEW
```

### Step 7: Integrate into Display Page

Edit `/home/just-paja/Work/shell/sacrebleui/crates/settings/src/pages/display.rs`:

Add import:
```rust
use crate::display::output_section::OutputSection;
```

Add section instantiation (after brightness, before dark_mode):
```rust
let output = OutputSection::new(entity_store, action_callback);
root.append(&output.root);
```

## Edge Cases and Considerations

1. **Empty make/model fields:** Use fallback logic to display output name if make/model are empty
2. **Current mode not found in available_modes:** Use index 0 as fallback (shouldn't happen with valid plugin data)
3. **Empty available_modes:** Guard with early return in reconciliation (plugin should never send this)
4. **Display disconnect:** Entity removed → reconciliation removes group
5. **Feedback loops:** `updating` guard prevents action callbacks during programmatic updates
6. **Initial cached data:** `idle_add_local_once()` ensures UI reconciles with data that arrived before subscription

## Verification Steps

### Build Verification
```bash
cargo build --workspace
cargo test --workspace
```

### Runtime Testing

1. **Start the daemon and settings app:**
   ```bash
   # In terminal 1
   cargo run --bin waft

   # In terminal 2 (Niri session)
   cargo run --bin waft-settings
   ```

2. **Navigate to Display page in settings sidebar**

3. **Verify display output section appears with:**
   - Display name/make/model in group title
   - Output name in description
   - Dropdown showing current resolution + refresh rate
   - VRR toggle (if supported by hardware)

4. **Test mode selection:**
   - Select a different resolution from dropdown
   - Verify display mode changes
   - Verify dropdown updates to show new current mode after entity update

5. **Test VRR toggle (if supported):**
   - Toggle VRR switch
   - Verify display VRR state changes
   - Verify switch updates to reflect new state after entity update

6. **Test display disconnect/reconnect:**
   - Disconnect a display (or simulate with niri config reload)
   - Verify group disappears from UI
   - Reconnect display
   - Verify group reappears

7. **Test initial reconciliation:**
   - With displays connected, restart settings app
   - Verify resolution controls appear immediately (no delay)

8. **Test with no displays:**
   - Verify section remains hidden when no `display-output` entities present

### Code Quality Checks

- No compiler warnings
- Consistent with existing settings patterns
- Uses `Cell<bool>` guards to prevent feedback loops
- Proper URN-based HashMap reconciliation
- Root visibility tied to entity presence
- Initial reconciliation for cached data

## Success Criteria

- ✅ Display settings page shows per-display resolution controls
- ✅ Mode dropdown populated with all available modes
- ✅ Current mode correctly selected in dropdown
- ✅ Mode selection triggers `set-mode` action with correct index
- ✅ VRR toggle visible only when supported
- ✅ VRR toggle triggers `toggle-vrr` action
- ✅ No feedback loops during updates
- ✅ Display connect/disconnect handled gracefully
- ✅ Initial reconciliation works with cached entities
- ✅ Clean build with no warnings
