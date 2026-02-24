---
name: widget-coding
description: Pattern for building GTK UI components. Dumb widgets implement RenderFn returning VNode trees. Smart containers use Reconciler with EntityStore subscriptions.
---

# Widget Coding Pattern (RenderFn + VNode + Reconciler)

## Architecture

Data flows DOWN (Props), events flow UP (Output). This is the unidirectional data flow used throughout waft-settings.

```
EntityStore --> SmartContainer --> Reconciler --> RenderComponent<F> (via VNode)
                SmartContainer <-- RenderComponent<F>                (via Output)
                SmartContainer --> Daemon                            (via EntityActionCallback)
```

## Type Hierarchy

Users implement `RenderFn`. The framework wraps it automatically:

```
RenderFn (you implement)
  -> RenderComponent<F> (auto-wrapper, implements Component)
     -> Component (underlying trait, used by VNode::new / VNode::with_output)
        -> VNode (type-erased, consumed by Reconciler)
```

**Never implement `Component` directly** (only tests do). Always implement `RenderFn` and alias with `pub type X = RenderComponent<XRender>;`.

Source: `crates/waft-ui-gtk/src/vdom/component.rs`, `crates/waft-ui-gtk/src/vdom/render_component.rs`

## Dumb Widget Template (no output)

For purely presentational widgets with no user interaction events.

```rust
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VLabel};

/// Input data -- must be Clone + PartialEq.
#[derive(Clone, PartialEq)]
pub struct PluginRowProps {
    pub name:         String,
    pub state:        PluginState,
    pub entity_types: Vec<String>,
}

pub(crate) struct PluginRowRender;

impl RenderFn for PluginRowRender {
    type Props  = PluginRowProps;
    type Output = ();                    // No events

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let subtitle = props.entity_types.join(", ");
        let state_label = props.state.to_string();

        VNode::action_row(
            VActionRow::new(&props.name)
                .subtitle(&subtitle)
                .suffix(VNode::label(
                    VLabel::new(&state_label).css_class("dim-label"),
                )),
        )
    }
}

/// Public type alias -- this is what smart containers import and use.
pub type PluginRow = RenderComponent<PluginRowRender>;
```

Reference: `crates/settings/src/plugins/plugin_row.rs`

## Dumb Widget Template (with output)

For widgets that emit user interaction events back to the smart container.

```rust
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VCustomButton, VIcon, VLabel};

#[derive(Clone, PartialEq)]
pub struct DeviceRowProps {
    pub name:             String,
    pub connected:        bool,
    // ... other fields
}

pub enum DeviceRowOutput {
    ToggleConnect,
    Remove,
}

pub(crate) struct DeviceRowRender;

impl RenderFn for DeviceRowRender {
    type Props  = DeviceRowProps;
    type Output = DeviceRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let label = if props.connected { "Disconnect" } else { "Connect" };

        // Clone emit for each closure that needs to fire an event
        let emit_toggle = emit.clone();
        let mut row = VActionRow::new(&props.name)
            .suffix(VNode::custom_button(
                VCustomButton::new(VNode::label(VLabel::new(label)))
                    .css_class("flat")
                    .on_click(move || {
                        // The emit idiom: clone, borrow, call if Some
                        if let Some(ref cb) = *emit_toggle.borrow() {
                            cb(DeviceRowOutput::ToggleConnect);
                        }
                    }),
            ));

        let emit_remove = emit.clone();
        row = row.suffix(VNode::custom_button(
            VCustomButton::new(VNode::icon(VIcon::new(
                vec![Icon::Themed("user-trash-symbolic".to_string())],
                16,
            )))
            .css_classes(["flat", "destructive-action"])
            .on_click(move || {
                if let Some(ref cb) = *emit_remove.borrow() {
                    cb(DeviceRowOutput::Remove);
                }
            }),
        ));

        VNode::action_row(row)
    }
}

pub type DeviceRow = RenderComponent<DeviceRowRender>;
```

Reference: `crates/settings/src/bluetooth/device_row.rs`

## RenderCallback Emit Idiom

The `emit` parameter in `render()` has type `RenderCallback<T>` which is `Rc<RefCell<Option<Box<dyn Fn(T)>>>>`. The callback is set by `RenderComponent` after `build()`, so during `render()` it may be `None` on the first call.

To emit an output event from within a closure:

```rust
// 1. Clone the Rc before moving into the closure
let emit_clone = emit.clone();

// 2. Inside the closure: borrow and call if Some
move || {
    if let Some(ref cb) = *emit_clone.borrow() {
        cb(YourOutput::SomeEvent);
    }
}
```

Each closure that emits events needs its own clone of `emit`. Do NOT share a single clone across multiple closures.

## VNode Primitives Catalog

All primitives are constructed via `VNode::constructor_name(VPrimitive::new(...))`.

| Primitive | Constructor | Description |
|-----------|-------------|-------------|
| `VLabel` | `VNode::label(VLabel::new("text"))` | Text label with CSS classes, alignment, ellipsize, wrap |
| `VBox` | `VNode::vbox(VBox::new(orientation, spacing))` | Container box with children |
| `VButton` | `VNode::button(VButton::new("label"))` | Button with label and on_click |
| `VSwitch` | `VNode::switch(VSwitch::new(active))` | Toggle switch with on_toggle |
| `VToggleButton` | `VNode::toggle_button(VToggleButton::new(child, active))` | Toggle button with child VNode |
| `VSpinner` | `VNode::spinner(VSpinner::new(spinning))` | Loading spinner |
| `VIcon` | `VNode::icon(VIcon::new(hints, pixel_size))` | Icon from themed/file/bytes hints |
| `VCustomButton` | `VNode::custom_button(VCustomButton::new(child))` | Button with arbitrary child VNode |
| `VPreferencesGroup` | `VNode::preferences_group(VPreferencesGroup::new())` | adw::PreferencesGroup with children |
| `VActionRow` | `VNode::action_row(VActionRow::new("title"))` | adw::ActionRow with prefix/suffix |
| `VSwitchRow` | `VNode::switch_row(VSwitchRow::new("title", active))` | adw::SwitchRow with on_toggle |
| `VEntryRow` | `VNode::entry_row(VEntryRow::new("title", "text"))` | adw::EntryRow with on_change |
| `VRevealer` | `VNode::revealer(VRevealer::new(child, reveal))` | Animated show/hide with child |
| `VProgressBar` | `VNode::progress_bar(VProgressBar::new(fraction))` | Progress bar (0.0..1.0) |
| `VScale` | `VNode::scale(VScale::new(value))` | Slider with on_value_change/on_value_commit |

Source: `crates/waft-ui-gtk/src/vdom/primitives.rs`, `crates/waft-ui-gtk/src/vdom/vnode.rs`

## Smart Container Overview (with Reconciler)

Smart containers own `EntityStore` subscriptions, create `VNode` trees for each entity, and hand them to a `Reconciler` for automatic add/update/remove lifecycle management.

```rust
use waft_ui_gtk::vdom::{Reconciler, VNode};

struct MyPageState {
    reconciler: Reconciler,  // manages child widgets in a gtk::Box
}

// In the reconcile function:
state.reconciler.reconcile(
    entities.iter().map(|(urn, entity)| {
        let cb = action_callback.clone();
        let urn_clone = urn.clone();
        VNode::with_output::<MyWidget>(
            MyWidgetProps { /* ... from entity ... */ },
            move |output| {
                // Map output events to entity actions
                cb(urn_clone.clone(), "action".to_string(), serde_json::Value::Null);
            },
        )
        .key(urn.as_str())   // Stable key for diffing
    }),
);
```

The `Reconciler` handles:
- **New key**: builds widget and appends to container
- **Existing key, same type, props changed**: calls `update()` in place
- **Existing key, same type, props unchanged**: skips (no GTK call)
- **Missing key**: removes widget from container

For the full EntityStore subscription + initial reconciliation pattern, see the `entity-store-subscription` skill.

Reference: `crates/settings/src/pages/wired.rs` (Reconciler-based), `crates/settings/src/pages/bluetooth.rs` (multi-entity-type)

## Using Components in Smart Containers

Without Reconciler (direct build/update):

```rust
// Build a component
let row = PluginRow::build(&props);

// Update with new props (skips if unchanged)
row.update(&new_props);

// Get the GTK widget for insertion
let widget = row.widget();

// Connect output events (call once after build)
row.connect_output(|output| { /* handle */ });
```

Reference: `crates/settings/src/pages/plugins.rs`

## Key Conventions

### Props
- Must implement `Clone + PartialEq` (required by `RenderFn` trait)
- Named `*Props` (e.g. `DeviceRowProps`, `PluginRowProps`)

### Output
- Named `*Output` (e.g. `DeviceRowOutput`)
- Skip entirely for purely presentational widgets (use `type Output = ()`)

### Naming
- `*Render` struct implements `RenderFn` (e.g. `DeviceRowRender`)
- `*Render` struct is `pub(crate)` (not part of public API)
- Public type alias: `pub type DeviceRow = RenderComponent<DeviceRowRender>;`
- Smart containers import the alias: `use crate::bluetooth::device_row::{DeviceRow, DeviceRowProps, DeviceRowOutput};`

### Imports
```rust
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VActionRow, VLabel, /* ... */};
```

## Legacy Note

Approximately 12 older widgets still use the manual `OutputCallback = Rc<RefCell<Option<Box<dyn Fn(Output)>>>>` pattern with imperative `connect_output()`, `apply_props()`, and `updating` guard. These work but should not be used as templates for new widgets:

- `crates/settings/src/sidebar.rs`
- `crates/settings/src/audio/device_card.rs`
- `crates/settings/src/wifi/available_networks_group.rs`
- `crates/settings/src/bluetooth/discovered_devices_group.rs`
- `crates/settings/src/wallpaper/mode_section.rs`
- `crates/settings/src/wallpaper/config_section.rs`
- `crates/settings/src/wallpaper/preview_section.rs`
- `crates/settings/src/wallpaper/transition_section.rs`
- `crates/settings/src/search_results.rs`
- `crates/settings/src/notifications/pattern_row.rs`
- `crates/settings/src/notifications/group_form.rs`
- `crates/settings/src/notifications/combinator_editor.rs`
- `crates/settings/src/weather/location_settings_group.rs`
- `crates/settings/src/keyboard/layout_row.rs`

## Reference Implementations

### Dumb Widgets
- `crates/settings/src/plugins/plugin_row.rs` -- no output, simplest example
- `crates/settings/src/bluetooth/device_row.rs` -- with output, multiple buttons
- `crates/settings/src/wifi/network_row.rs` -- with output, icon + signal strength
- `crates/settings/src/wired/connection_row.rs` -- with output, activate/deactivate

### Smart Containers
- `crates/settings/src/pages/plugins.rs` -- simple, direct Component::build/update (no Reconciler)
- `crates/settings/src/pages/wired.rs` -- Reconciler-based with VNode::with_output
- `crates/settings/src/pages/bluetooth.rs` -- multi-entity-type, Reconciler-based
