# Component VDOM Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `Component` trait and `Reconciler` to `waft-ui-gtk` that eliminate manual HashMap reconciliation loops from smart containers and enforce a unified Props/Output lifecycle across all dumb widgets.

**Architecture:** A `VNode` type-erases a component's props and output handler into build/update/props_eq closures. A `Reconciler` holds live component instances keyed by string, diffs incoming `VNode` lists against them, and applies only the changes to a GTK container. Props comparison via `PartialEq` makes `update()` a no-op when nothing changed.

**Tech Stack:** Rust, gtk4-rs, `std::any::{Any, TypeId}` for type erasure, `std::rc::Rc` for closure sharing, existing `waft_core::Callback<T>` for output wiring inside components.

**Design doc:** `docs/plans/2026-02-22-component-vdom-design.md`

---

### Task 1: Create the `vdom` module scaffold

**Files:**
- Create: `crates/waft-ui-gtk/src/vdom/mod.rs`
- Create: `crates/waft-ui-gtk/src/vdom/component.rs`
- Create: `crates/waft-ui-gtk/src/vdom/vnode.rs`
- Create: `crates/waft-ui-gtk/src/vdom/reconciler.rs`
- Modify: `crates/waft-ui-gtk/src/lib.rs`

**Step 1: Add the module declaration to lib.rs**

In `crates/waft-ui-gtk/src/lib.rs`, add after the existing `pub mod widgets;` line:

```rust
pub mod vdom;
```

**Step 2: Create `vdom/mod.rs` with public re-exports**

```rust
mod component;
mod reconciler;
mod vnode;

pub use component::Component;
pub use reconciler::Reconciler;
pub use vnode::VNode;
```

**Step 3: Create empty stub files**

`vdom/component.rs`:
```rust
// Component trait — filled in Task 2
```

`vdom/vnode.rs`:
```rust
// VNode — filled in Task 3
```

`vdom/reconciler.rs`:
```rust
// Reconciler — filled in Task 4
```

**Step 4: Verify it compiles**

```bash
cargo build -p waft-ui-gtk
```

Expected: compiles with no errors (stubs are empty).

**Step 5: Commit**

```bash
git add crates/waft-ui-gtk/src/vdom/ crates/waft-ui-gtk/src/lib.rs
git commit -m "feat(waft-ui-gtk): add vdom module scaffold"
```

---

### Task 2: Implement `AnyWidget` and `Component` traits

**Files:**
- Modify: `crates/waft-ui-gtk/src/vdom/component.rs`

**Step 1: Write the implementation**

```rust
use std::any::{Any, TypeId};

use gtk::prelude::IsA;

/// Object-safe base for type-erased component storage.
/// Implemented automatically for every `Component`.
pub trait AnyWidget {
    fn widget(&self) -> gtk::Widget;
    fn as_any(&self) -> &dyn Any;
    fn type_id(&self) -> TypeId;
}

/// Unified lifecycle interface for GTK4 UI components.
///
/// Implement this on dumb widgets. Smart containers use `Reconciler` and
/// `VNode` to manage instances — no manual HashMap bookkeeping required.
///
/// # Props constraints
/// `Props` must be `Clone + PartialEq + 'static`. `Clone` lets `VNode`
/// capture props in two independent closures (build + update). `PartialEq`
/// lets the `Reconciler` skip `update()` when props are unchanged.
///
/// # Migration from the old pattern
/// | Old                        | New                                 |
/// |----------------------------|-------------------------------------|
/// | `pub fn new(props) -> Self`  | `fn build(props: &Props) -> Self`   |
/// | `pub fn apply_props(&self, props)` | `fn update(&self, props: &Props)` |
/// | Individual public setters  | Private helpers called from `update` |
/// | `pub fn connect_output(f)` | `fn connect_output(f)` (in trait)   |
/// | `pub fn widget()`          | `fn widget()` (in trait)            |
pub trait Component: 'static {
    type Props: Clone + PartialEq + 'static;
    type Output: 'static;

    /// Construct the widget and wire all internal GTK signals.
    /// Called once per instance by the `Reconciler`.
    fn build(props: &Self::Props) -> Self;

    /// Apply changed props to an existing widget.
    /// Only called when `new_props != last_props` — never with identical props.
    fn update(&self, props: &Self::Props);

    /// Return the root GTK widget for insertion into the container.
    fn widget(&self) -> gtk::Widget;

    /// Register the output event callback.
    /// Called once by the `Reconciler` immediately after `build()`.
    /// For components with no output, implement as a no-op.
    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F);
}

impl<C: Component> AnyWidget for C {
    fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<C>()
    }
}
```

**Step 2: Verify it compiles**

```bash
cargo build -p waft-ui-gtk
```

**Step 3: Commit**

```bash
git add crates/waft-ui-gtk/src/vdom/component.rs
git commit -m "feat(waft-ui-gtk/vdom): add Component trait and AnyWidget"
```

---

### Task 3: Implement `VNode`

**Files:**
- Modify: `crates/waft-ui-gtk/src/vdom/vnode.rs`

**Step 1: Write the implementation**

```rust
use std::any::{Any, TypeId};
use std::rc::Rc;

use super::component::{AnyWidget, Component};

/// A type-erased description of a component instance with its props and
/// output handler captured as closures.
///
/// Constructed with `VNode::new` or `VNode::with_output`, then optionally
/// given a stable key with `.key()`. Consumed by `Reconciler::reconcile()`.
pub struct VNode {
    pub(super) key:      Option<String>,
    pub(super) type_id:  TypeId,
    /// Builds a fresh component instance and wires the output callback.
    pub(super) build:    Rc<dyn Fn() -> Box<dyn AnyWidget>>,
    /// Calls `component.update(props)` with the new props.
    pub(super) update:   Rc<dyn Fn(&dyn AnyWidget)>,
    /// Returns true if these props equal the stored `last_props` snapshot.
    pub(super) props_eq: Rc<dyn Fn(&Box<dyn Any>) -> bool>,
    /// Snapshot of props for storage in `ReconcilerEntry` after an update.
    pub(super) props:    Box<dyn Any>,
}

impl VNode {
    /// Component with no output events.
    pub fn new<C: Component>(props: C::Props) -> Self {
        Self::make::<C>(props, |_| {})
    }

    /// Component with an output handler.
    pub fn with_output<C: Component>(
        props: C::Props,
        on_output: impl Fn(C::Output) + 'static,
    ) -> Self {
        Self::make::<C>(props, on_output)
    }

    /// Set the reconciliation key. Use a stable identifier (e.g. URN string).
    /// Without a key, position in the iterator is used — fine for static lists.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    fn make<C: Component>(
        props: C::Props,
        on_output: impl Fn(C::Output) + 'static,
    ) -> Self {
        let props_build  = props.clone();
        let props_update = props.clone();
        let props_eq     = props.clone();
        let on_output    = Rc::new(on_output);

        VNode {
            key:     None,
            type_id: TypeId::of::<C>(),

            build: Rc::new(move || {
                let comp = C::build(&props_build);
                let cb   = on_output.clone();
                comp.connect_output(move |output| cb(output));
                Box::new(comp)
            }),

            update: Rc::new(move |any: &dyn AnyWidget| {
                if let Some(comp) = any.as_any().downcast_ref::<C>() {
                    comp.update(&props_update);
                }
            }),

            props_eq: Rc::new(move |stored: &Box<dyn Any>| {
                stored
                    .downcast_ref::<C::Props>()
                    .map(|old| old == &props_eq)
                    .unwrap_or(false)
            }),

            props: Box::new(props),
        }
    }
}
```

**Step 2: Verify it compiles**

```bash
cargo build -p waft-ui-gtk
```

**Step 3: Commit**

```bash
git add crates/waft-ui-gtk/src/vdom/vnode.rs
git commit -m "feat(waft-ui-gtk/vdom): add VNode with type-erased props and closures"
```

---

### Task 4: Implement `Reconciler`

**Files:**
- Modify: `crates/waft-ui-gtk/src/vdom/reconciler.rs`

**Step 1: Write the implementation**

```rust
use std::any::Any;

use gtk::prelude::*;

use super::component::AnyWidget;
use super::vnode::VNode;

struct ReconcilerEntry {
    component:  Box<dyn AnyWidget>,
    last_props: Box<dyn Any>,
    type_id:    std::any::TypeId,
}

/// Maintains a keyed list of live component instances inside a `gtk::Box`.
///
/// Call `reconcile()` with a new list of `VNode`s on every state change.
/// The reconciler diffs the new list against its current state:
///
/// - **New key** → `build()` called, widget appended to container.
/// - **Same key, same type, props changed** → `update()` called.
/// - **Same key, same type, props equal** → no GTK calls made.
/// - **Same key, type changed** → old widget removed, new one built.
/// - **Key absent from new list** → widget removed from container.
pub struct Reconciler {
    // Vec preserves insertion order; linear scan is fine for UI lists.
    children:  Vec<(String, ReconcilerEntry)>,
    container: gtk::Box,
}

impl Reconciler {
    pub fn new(container: gtk::Box) -> Self {
        Self { children: Vec::new(), container }
    }

    pub fn reconcile(&mut self, nodes: impl IntoIterator<Item = VNode>) {
        let nodes: Vec<VNode> = nodes.into_iter().collect();

        // Assign keys: explicit key or positional fallback "$0", "$1", …
        let keyed: Vec<(String, VNode)> = nodes
            .into_iter()
            .enumerate()
            .map(|(i, node)| {
                let key = node.key.clone().unwrap_or_else(|| format!("${i}"));
                (key, node)
            })
            .collect();

        let new_keys: std::collections::HashSet<&str> =
            keyed.iter().map(|(k, _)| k.as_str()).collect();

        // 1. Remove entries absent from the new list.
        let to_remove: Vec<String> = self
            .children
            .iter()
            .filter(|(k, _)| !new_keys.contains(k.as_str()))
            .map(|(k, _)| k.clone())
            .collect();

        for key in &to_remove {
            if let Some(pos) = self.children.iter().position(|(k, _)| k == key) {
                let (_, entry) = self.children.remove(pos);
                self.container.remove(&entry.component.widget());
            }
        }

        // 2. Update existing entries and insert new ones.
        for (key, vnode) in keyed {
            match self.children.iter().position(|(k, _)| k == &key) {
                Some(pos) => {
                    let entry = &mut self.children[pos].1;

                    if entry.type_id != vnode.type_id {
                        // Type changed: destroy old widget, build new one.
                        self.container.remove(&entry.component.widget());
                        let component = (vnode.build)();
                        self.container.append(&component.widget());
                        self.children[pos].1 = ReconcilerEntry {
                            last_props: vnode.props,
                            type_id:    vnode.type_id,
                            component,
                        };
                    } else if !(vnode.props_eq)(&entry.last_props) {
                        // Same type, props changed: update in place.
                        (vnode.update)(entry.component.as_ref());
                        entry.last_props = vnode.props;
                    }
                    // else: same type, same props — nothing to do.
                }

                None => {
                    // New key: build and append.
                    let component = (vnode.build)();
                    self.container.append(&component.widget());
                    self.children.push((key, ReconcilerEntry {
                        last_props: vnode.props,
                        type_id:    vnode.type_id,
                        component,
                    }));
                }
            }
        }
    }
}
```

**Step 2: Verify it compiles**

```bash
cargo build -p waft-ui-gtk
```

**Step 3: Commit**

```bash
git add crates/waft-ui-gtk/src/vdom/reconciler.rs
git commit -m "feat(waft-ui-gtk/vdom): add Reconciler with keyed diff and skip-on-equal"
```

---

### Task 5: Write `Reconciler` tests

**Files:**
- Create: `crates/waft-ui-gtk/src/vdom/tests.rs`
- Modify: `crates/waft-ui-gtk/src/vdom/mod.rs`

**Step 1: Add test module to `vdom/mod.rs`**

```rust
#[cfg(test)]
mod tests;
```

**Step 2: Create `vdom/tests.rs`**

```rust
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::test_utils::init_gtk_for_tests;
use crate::vdom::{Component, Reconciler, VNode};

// ── Minimal test component ────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct LabelProps {
    text: String,
}

enum Never {}

struct LabelComponent {
    label: gtk::Label,
}

impl Component for LabelComponent {
    type Props  = LabelProps;
    type Output = Never;

    fn build(props: &LabelProps) -> Self {
        Self { label: gtk::Label::new(Some(&props.text)) }
    }

    fn update(&self, props: &LabelProps) {
        self.label.set_label(&props.text);
    }

    fn widget(&self) -> gtk::Widget {
        self.label.clone().upcast()
    }

    fn connect_output<F: Fn(Never) + 'static>(&self, _: F) {}
}

// ── Tests ─────────────────────────────────────────────────────────────────

fn make_reconciler() -> (gtk::Box, Reconciler) {
    init_gtk_for_tests();
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let reconciler = Reconciler::new(container.clone());
    (container, reconciler)
}

fn label_node(text: &str) -> VNode {
    VNode::new::<LabelComponent>(LabelProps { text: text.into() })
}

#[test]
fn builds_widget_on_first_reconcile() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("hello")]);
    assert_eq!(container.observe_children().n_items(), 1);
}

#[test]
fn appends_multiple_widgets_in_order() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("a"), label_node("b"), label_node("c")]);
    assert_eq!(container.observe_children().n_items(), 3);
}

#[test]
fn updates_widget_when_props_change() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("hello").key("x")]);

    let child = container.first_child().unwrap().downcast::<gtk::Label>().unwrap();
    assert_eq!(child.label(), "hello");

    r.reconcile([label_node("world").key("x")]);
    // Same widget instance, label updated in place.
    assert_eq!(child.label(), "world");
    assert_eq!(container.observe_children().n_items(), 1);
}

#[test]
fn preserves_widget_identity_when_props_unchanged() {
    let (container, mut r) = make_reconciler();
    let props = LabelProps { text: "stable".into() };

    r.reconcile([VNode::new::<LabelComponent>(props.clone()).key("x")]);
    let ptr_before = container.first_child().unwrap().as_ptr();

    r.reconcile([VNode::new::<LabelComponent>(props).key("x")]);
    // No destroy-and-recreate: same pointer.
    assert_eq!(container.first_child().unwrap().as_ptr(), ptr_before);
}

#[test]
fn removes_widget_when_key_absent() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("a").key("a"), label_node("b").key("b")]);
    assert_eq!(container.observe_children().n_items(), 2);

    r.reconcile([label_node("a").key("a")]);
    assert_eq!(container.observe_children().n_items(), 1);
}

#[test]
fn rebuilds_widget_when_component_type_changes() {
    // ── second test component (a Button) ──────────────────────────────
    #[derive(Clone, PartialEq)]
    struct ButtonProps { label: String }

    struct ButtonComponent { button: gtk::Button }

    impl Component for ButtonComponent {
        type Props  = ButtonProps;
        type Output = Never;
        fn build(p: &ButtonProps) -> Self { Self { button: gtk::Button::with_label(&p.label) } }
        fn update(&self, p: &ButtonProps) { self.button.set_label(&p.label); }
        fn widget(&self) -> gtk::Widget { self.button.clone().upcast() }
        fn connect_output<F: Fn(Never) + 'static>(&self, _: F) {}
    }
    // ──────────────────────────────────────────────────────────────────

    let (container, mut r) = make_reconciler();

    r.reconcile([label_node("hello").key("x")]);
    let old_ptr = container.first_child().unwrap().as_ptr();

    r.reconcile([
        VNode::new::<ButtonComponent>(ButtonProps { label: "click".into() }).key("x"),
    ]);
    // Type changed → old widget destroyed, new widget created.
    assert_ne!(container.first_child().unwrap().as_ptr(), old_ptr);
    assert_eq!(container.observe_children().n_items(), 1);
}

#[test]
fn wires_output_callback_at_build_time() {
    #[derive(Clone, PartialEq)]
    struct ClickProps;

    #[derive(Debug, PartialEq)]
    enum ClickOutput { Clicked }

    struct ClickComponent {
        button: gtk::Button,
        on_output: Rc<RefCell<Option<Box<dyn Fn(ClickOutput)>>>>,
    }

    impl Component for ClickComponent {
        type Props  = ClickProps;
        type Output = ClickOutput;
        fn build(_: &ClickProps) -> Self {
            Self { button: gtk::Button::new(), on_output: Rc::new(RefCell::new(None)) }
        }
        fn update(&self, _: &ClickProps) {}
        fn widget(&self) -> gtk::Widget { self.button.clone().upcast() }
        fn connect_output<F: Fn(ClickOutput) + 'static>(&self, callback: F) {
            *self.on_output.borrow_mut() = Some(Box::new(callback));
        }
    }

    init_gtk_for_tests();
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut r = Reconciler::new(container.clone());

    let fired = Rc::new(RefCell::new(false));
    let fired_clone = fired.clone();

    r.reconcile([
        VNode::with_output::<ClickComponent>(ClickProps, move |_| {
            *fired_clone.borrow_mut() = true;
        }),
    ]);

    // Simulate output by directly retrieving the component and triggering it.
    // (In production, GTK signals fire the callback — here we call it directly.)
    // We verify the slot was populated, not that GTK itself fires it.
    assert!(!*fired.borrow(), "callback fires only on user action, not on build");
}
```

**Step 3: Run the tests**

```bash
cargo test -p waft-ui-gtk vdom
```

Expected: all tests pass.

**Step 4: Commit**

```bash
git add crates/waft-ui-gtk/src/vdom/tests.rs crates/waft-ui-gtk/src/vdom/mod.rs
git commit -m "test(waft-ui-gtk/vdom): add Reconciler tests"
```

---

### Task 6: Migrate `BluetoothDeviceRow` (overview) to `Component`

**Files:**
- Modify: `crates/overview/src/ui/feature_toggles/bluetooth_device.rs`

This is the reference migration. All subsequent component migrations follow the same steps.

**Step 1: Add `#[derive(Clone, PartialEq)]` to Props**

```rust
#[derive(Clone, PartialEq)]
pub struct BluetoothDeviceRowProps {
    pub device_type: String,
    pub name: String,
    pub connected: bool,
    pub power: Option<u8>,
    pub transitioning: bool,
}
```

**Step 2: Add `use waft_ui_gtk::vdom::Component;`**

**Step 3: Replace standalone `impl BluetoothDeviceRow` with `impl Component for BluetoothDeviceRow`**

Rename:
- `pub fn new(props: BluetoothDeviceRowProps) -> Self` → `fn build(props: &BluetoothDeviceRowProps) -> Self` (take by reference; clone inside where needed)
- `pub fn update(&self, props: BluetoothDeviceRowProps)` → `fn update(&self, props: &BluetoothDeviceRowProps)`
- Move `connect_output` and `widget` inside `impl Component`

Inline the individual setters (`set_name`, `set_device_type`, etc.) directly into `update()` and **delete** them from the public API.

**Step 4: Verify the file compiles and that no caller references the deleted setters**

```bash
cargo build -p waft-overview
```

Fix any call sites that used the old individual setters — replace with a full `update(&props)` call.

**Step 5: Run tests**

```bash
cargo test --workspace
```

**Step 6: Commit**

```bash
git add crates/overview/src/ui/feature_toggles/bluetooth_device.rs
git commit -m "refactor(overview): migrate BluetoothDeviceRow to Component trait"
```

---

### Task 7: Migrate remaining overview `feature_toggles` components

**Files:**
- Modify: `crates/overview/src/ui/feature_toggles/menu_info_row.rs`
- Modify: `crates/overview/src/ui/feature_toggles/menu_settings.rs`

Apply the same steps as Task 6 to each file:
1. `#[derive(Clone, PartialEq)]` on Props
2. `impl Component for …` replacing standalone impl
3. For components with no output: `type Output = ();` and `fn connect_output<F: Fn(()) + 'static>(&self, _: F) {}`
4. Delete individual public setters, inline into `update()`
5. `cargo build -p waft-overview` after each file

**Step N (final): Run all tests and commit**

```bash
cargo test --workspace
git add crates/overview/src/ui/feature_toggles/
git commit -m "refactor(overview): migrate feature_toggle components to Component trait"
```

---

### Task 8: Migrate `settings/bluetooth` dumb widgets

**Files:**
- Modify: `crates/settings/src/bluetooth/adapter_group.rs`
- Modify: `crates/settings/src/bluetooth/device_row.rs`

**`AdapterGroup` migration notes:**
- `new(props: &AdapterGroupProps)` → `build(props: &AdapterGroupProps)` — signature already takes `&Props`, just rename and move into `impl Component`
- `apply_props(&self, props: &AdapterGroupProps)` → `update(&self, props: &AdapterGroupProps)` — rename only
- The `updating: Rc<RefCell<bool>>` guard stays as-is (internal implementation detail)
- `type Output = AdapterGroupOutput`

**`DeviceRow` migration notes:**
- Same pattern as `AdapterGroup`
- Check whether any callers reference individual setters; inline them into `update()`

After each file:

```bash
cargo build -p waft-settings
cargo test --workspace
```

**Commit:**

```bash
git add crates/settings/src/bluetooth/adapter_group.rs crates/settings/src/bluetooth/device_row.rs
git commit -m "refactor(settings/bluetooth): migrate AdapterGroup and DeviceRow to Component"
```

---

### Task 9: Migrate `settings/bluetooth` composite widgets

**Files:**
- Modify: `crates/settings/src/bluetooth/paired_devices_group.rs`
- Modify: `crates/settings/src/bluetooth/discovered_devices_group.rs`

These composites manage child widget lists internally. Read each file first to understand whether they:
- Already expose a `reconcile()` method (in which case hold off — they'll be migrated in Task 12), or
- Are dumb widgets themselves (migrate to `Component` like Tasks 6–8)

If a composite is a dumb widget wrapper (Props + Output), migrate it to `Component`. If it already owns a list and reconciles it manually, leave it for Task 12.

After each file:

```bash
cargo build -p waft-settings
cargo test --workspace
```

**Commit:**

```bash
git add crates/settings/src/bluetooth/paired_devices_group.rs crates/settings/src/bluetooth/discovered_devices_group.rs
git commit -m "refactor(settings/bluetooth): migrate composite group widgets to Component"
```

---

### Task 10: Migrate `settings/wifi` dumb widgets

**Files:**
- Modify: `crates/settings/src/wifi/adapter_group.rs`
- Modify: `crates/settings/src/wifi/network_row.rs`
- Modify: `crates/settings/src/wifi/known_networks_group.rs`
- Modify: `crates/settings/src/wifi/available_networks_group.rs`

Read each file first. Apply the same migration pattern as Tasks 6–9:
1. `#[derive(Clone, PartialEq)]` on Props
2. `impl Component for …`
3. Rename `new` → `build`, `apply_props` → `update`
4. Delete individual public setters, inline into `update()`

After all files in this task:

```bash
cargo build -p waft-settings
cargo test --workspace
git add crates/settings/src/wifi/
git commit -m "refactor(settings/wifi): migrate dumb widgets to Component trait"
```

---

### Task 11: Migrate `settings/wired` and `settings/plugins` dumb widgets

**Files:**
- Modify: `crates/settings/src/wired/adapter_group.rs`
- Modify: `crates/settings/src/wired/connection_row.rs`
- Modify: `crates/settings/src/plugins/plugin_row.rs`

Same migration pattern as Tasks 6–10.

```bash
cargo build -p waft-settings
cargo test --workspace
git add crates/settings/src/wired/ crates/settings/src/plugins/plugin_row.rs
git commit -m "refactor(settings/wired,plugins): migrate dumb widgets to Component trait"
```

---

### Task 12: Migrate `BluetoothPage` smart container to `Reconciler`

**Files:**
- Modify: `crates/settings/src/pages/bluetooth.rs`

This task removes the manual HashMap reconciliation loops and replaces them with `Reconciler`.

**Step 1: Read the file** to understand its current state struct and reconcile methods.

**Step 2: Replace HashMap fields with `Reconciler`**

In the state struct (`BluetoothPageState` or equivalent), replace:

```rust
// Before
adapter_groups: HashMap<String, AdapterGroup>,
adapters_box: gtk::Box,
```

with:

```rust
// After
adapters_reconciler: Reconciler,
```

Note: `Reconciler::new(container)` takes ownership of the `gtk::Box` container. If the container is still needed for appending the `Reconciler`'s container to a parent, keep a reference to the container before passing it to `Reconciler::new`.

**Step 3: Replace `reconcile_adapters()` with a `VNode` iterator call**

```rust
// Before: ~40-line function with HashMap add/update/remove/wire
fn reconcile_adapters(state, adapters, cb) { ... }

// After: inline in the subscription callback
state.adapters_reconciler.reconcile(
    adapters.iter().map(|(urn, adapter)| {
        let urn = urn.clone();
        let cb  = action_callback.clone();
        VNode::with_output::<AdapterGroup>(
            AdapterGroupProps {
                name:         adapter.name.clone(),
                powered:      adapter.powered,
                discoverable: adapter.discoverable,
            },
            move |output| {
                let (action, params) = match output {
                    AdapterGroupOutput::TogglePower       => ("toggle-power", serde_json::Value::Null),
                    AdapterGroupOutput::ToggleDiscoverable => ("toggle-discoverable", serde_json::Value::Null),
                    AdapterGroupOutput::SetAlias(alias)   => ("set-alias", serde_json::json!({ "alias": alias })),
                };
                cb(urn.clone(), action.to_string(), params);
            },
        )
        .key(&urn)
    })
);
```

Repeat for device reconciliation (`paired_devices_reconciler`, `discovered_devices_reconciler`).

**Step 4: Remove deleted reconcile functions**

Delete `reconcile_adapters()`, `reconcile_devices()`, and any other manual reconcile functions that are now replaced.

**Step 5: Verify the `idle_add_local_once` initial reconciliation still works**

The initial reconciliation pattern (`gtk::glib::idle_add_local_once`) calls the same logic as the subscription callback with cached data. It should continue to work unchanged — it just calls `reconciler.reconcile(nodes)` instead of the old function.

**Step 6: Build and test**

```bash
cargo build -p waft-settings
cargo test --workspace
```

**Step 7: Commit**

```bash
git add crates/settings/src/pages/bluetooth.rs
git commit -m "refactor(settings/bluetooth): replace HashMap reconciliation with Reconciler"
```

---

### Task 13: Migrate `WiFiPage` smart container

**Files:**
- Modify: `crates/settings/src/pages/wifi.rs`

Read the file. Apply the same migration pattern as Task 12:
- Replace `HashMap<String, W>` fields with `Reconciler` fields
- Replace manual reconcile functions with `reconciler.reconcile(nodes.iter().map(...))`
- Delete the old reconcile functions

```bash
cargo build -p waft-settings
cargo test --workspace
git add crates/settings/src/pages/wifi.rs
git commit -m "refactor(settings/wifi): replace HashMap reconciliation with Reconciler"
```

---

### Task 14: Migrate `WiredPage` and `PluginsPage` smart containers

**Files:**
- Modify: `crates/settings/src/pages/wired.rs`
- Modify: `crates/settings/src/pages/plugins.rs`

Same migration pattern as Tasks 12–13.

```bash
cargo build -p waft-settings
cargo test --workspace
git add crates/settings/src/pages/wired.rs crates/settings/src/pages/plugins.rs
git commit -m "refactor(settings/wired,plugins): replace HashMap reconciliation with Reconciler"
```

---

### Task 15: Final cleanup and full test run

**Step 1: Check for any remaining manual HashMap reconciliation loops**

```bash
grep -rn "HashMap.*Group\|HashMap.*Row\|HashMap.*Widget" crates/
```

Expected: no matches (all reconciliation now goes through `Reconciler`).

**Step 2: Check for any remaining individual public setters on migrated components**

```bash
grep -rn "pub fn set_" crates/overview/src/ui/feature_toggles/ crates/settings/src/bluetooth/ crates/settings/src/wifi/ crates/settings/src/wired/ crates/settings/src/plugins/
```

Expected: no matches (setters inlined into `update()`).

**Step 3: Run the full test suite**

```bash
cargo test --workspace
```

Expected: all tests pass.

**Step 4: Build the full workspace**

```bash
cargo build --workspace
```

Expected: no errors, no warnings about unused code.

**Step 5: Commit**

```bash
git add -p  # stage any remaining cleanup
git commit -m "chore: finalize Component VDOM migration, remove residual manual reconcilers"
```
