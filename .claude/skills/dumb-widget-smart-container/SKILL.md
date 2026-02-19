---
name: dumb-widget-smart-container
description: Pattern for building GTK UI components in waft-settings. Dumb widgets receive data via Props, emit events via Output. Smart containers own store subscriptions and orchestrate dumb widgets.
---

# Dumb Widget + Smart Container Pattern

## Architecture

Data flows DOWN (Props/setters), events flow UP (Output callbacks). This is the React-ish unidirectional data flow pattern used throughout waft-settings.

```
EntityStore --> SmartContainer --> DumbWidget (via Props)
                SmartContainer <-- DumbWidget (via Output)
                SmartContainer --> Daemon    (via EntityActionCallback)
```

## Dumb Widget Template

Dumb widgets are presentational. They never hold store references or subscribe to stores.

```rust
use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

/// Input data for the widget.
pub struct YourWidgetProps {
    pub name: String,
    pub active: bool,
}

/// Events emitted by the widget.
pub enum YourWidgetOutput {
    ToggleActive,
    Renamed(String),
}

/// Callback type alias.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(YourWidgetOutput)>>>>;

/// A presentational widget.
pub struct YourWidget {
    pub root: adw::PreferencesGroup,
    switch_row: adw::SwitchRow,
    /// Guard against feedback loops when programmatically updating state.
    updating: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
}

impl YourWidget {
    pub fn new(props: &YourWidgetProps) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(&props.name)
            .build();

        let switch_row = adw::SwitchRow::builder()
            .title("Active")
            .active(props.active)
            .build();
        group.add(&switch_row);

        let updating = Rc::new(RefCell::new(false));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Wire switch toggle
        let cb = output_cb.clone();
        let guard = updating.clone();
        switch_row.connect_active_notify(move |_row| {
            if *guard.borrow() {
                return;
            }
            if let Some(ref callback) = *cb.borrow() {
                callback(YourWidgetOutput::ToggleActive);
            }
        });

        Self {
            root: group,
            switch_row,
            updating,
            output_cb,
        }
    }

    /// Update widget state from new props.
    pub fn apply_props(&self, props: &YourWidgetProps) {
        self.root.set_title(&props.name);

        // Use updating guard to prevent feedback loops
        *self.updating.borrow_mut() = true;
        self.switch_row.set_active(props.active);
        *self.updating.borrow_mut() = false;
    }

    /// Register an output event callback.
    pub fn connect_output(&self, callback: impl Fn(YourWidgetOutput) + 'static) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
```

## Smart Container Template

Smart containers own store subscriptions, create/update/remove dumb widgets, and forward actions to the daemon.

See the `entity-store-subscription` skill for the full smart container pattern with EntityStore subscription and initial reconciliation.

## Key Conventions

### Naming
- `*Props` for input structs
- `*Output` for event enums
- `connect_output()` for callback registration
- `apply_props()` for state updates
- `pub root` for the GTK root widget

### Feedback Loop Prevention
Switches and other interactive widgets fire `notify` signals when set programmatically. Use an `updating: Rc<RefCell<bool>>` guard:

```rust
// In signal handler:
if *guard.borrow() { return; }

// When setting props:
*self.updating.borrow_mut() = true;
self.switch_row.set_active(value);
*self.updating.borrow_mut() = false;
```

### When to Skip Output
If a widget is purely presentational with no user interactions, skip the `Output` enum and `connect_output`. Just provide `Props` + `apply_props`.

### Visibility Control
Widgets hide themselves until entity data arrives. Use `root.set_visible(false)` initially and show when reconcile provides data.

## Reference Implementations

### Dumb Widgets
- `crates/settings/src/bluetooth/adapter_group.rs` -- SwitchRows, EntryRow, Button with multiple Output variants
- `crates/settings/src/bluetooth/device_row.rs` -- ActionRow with status suffix
- `crates/settings/src/wifi/network_row.rs` -- ActionRow with signal strength icon
- `crates/settings/src/plugins/plugin_row.rs` -- Simple Props + apply_props without Output (purely presentational)

### Smart Containers
- `crates/settings/src/pages/bluetooth.rs` -- Two entity types, adapter groups + device lists
- `crates/settings/src/pages/wifi.rs` -- WiFi adapters + network lists
- `crates/settings/src/pages/wired.rs` -- Ethernet adapters + connection profiles
- `crates/settings/src/pages/plugins.rs` -- Single entity type with empty state toggle
