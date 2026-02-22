# Component VDOM Design

**Date:** 2026-02-22
**Branch:** larger-larger-picture

## Problem

UI components share a repeated lifecycle: `Props` struct, `Output` enum, constructor, update method,
`connect_output` callback wiring, and a `widget()` accessor. Smart containers duplicate a 40-line
`HashMap<String, W>` add/update/remove reconciliation loop for every list they manage. There is no
interface that enforces this pattern, no mechanism to skip redundant GTK updates when props haven't
changed, and no way to declare what a component tree looks like without also manually managing its
lifecycle.

No mature GTK4 VDOM library exists in Rust (vgtk targets GTK3 and is abandoned; Grex is
experimental with known memory leaks; Xilem uses its own renderer). The solution is built in-crate.

## Goals

1. Define a `Component` trait that unifies the component lifecycle interface.
2. Provide a `VNode` type that describes a component declaratively (props + output handler + key).
3. Provide a `Reconciler` that diffs a new `VNode` list against its current state and applies only
   the changes to GTK — skipping `update()` entirely when props are unchanged.
4. Incrementally extend `VNode` to cover GTK primitives (`Box`, `Label`, `Button`, `Switch`, …) so
   dumb widgets can eventually be expressed as pure `render()` calls with no manual GTK.
5. Migrate all existing smart containers and dumb widgets to the new interface.

## Non-Goals

- Replacing GTK4 or libadwaita — this is a thin reconciliation layer on top of gtk-rs.
- Animation or CSS transition management.
- Full widget-tree server-side rendering or serialization.
- Async rendering / scheduling (all reconciliation is synchronous on the GTK main thread).

---

## Architecture

```
EntityStore change
  → smart container subscription fires
  → container calls render() → impl IntoIterator<Item = VNode>
  → reconciler.reconcile(nodes)
      → for each (key, new_vnode):
          if key absent   → build() + connect_output() + append to GTK container
          if type changed → remove old widget, build() new
          if props equal  → skip (no GTK call)
          if props differ → update()
      → for each key absent from new list → remove widget
```

Two phases delivered incrementally:

- **Phase 1** — `VNode` wraps custom `Component` implementations (dumb widgets). Smart containers
  use `Reconciler` instead of manual HashMaps.
- **Phase 2** — `VNode` gains primitive variants (`VBox`, `VLabel`, `VButton`, `VSwitch`, …).
  Dumb widgets can be rewritten as pure `render()` returning a `VNode` tree, eliminating the manual
  GTK wiring inside `build()`.

---

## Phase 1: Component Trait and Reconciler

### `Component` trait

Lives in `crates/waft-ui-gtk/src/vdom/component.rs`.

```rust
pub trait Component: 'static {
    type Props: Clone + PartialEq + 'static;
    type Output: 'static;

    /// Called once to construct the widget and wire internal GTK signals.
    fn build(props: &Self::Props) -> Self;

    /// Called when props have changed (Reconciler already confirmed inequality).
    fn update(&self, props: &Self::Props);

    /// Return the root GTK widget to be appended to / removed from the container.
    fn widget(&self) -> gtk::Widget;

    /// Register the output callback. Called once by the Reconciler after build().
    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F);
}
```

`Props: Clone` is required so `VNode` can capture props in two closures (build and update).
`Props: PartialEq` is required so the Reconciler can skip redundant updates.

For components with no output events, `Output` is `()` and `connect_output` is a no-op.

### `VNode`

Lives in `crates/waft-ui-gtk/src/vdom/vnode.rs`.

```rust
pub struct VNode {
    key:      Option<String>,
    type_id:  TypeId,
    build:    Rc<dyn Fn() -> Box<dyn AnyWidget>>,
    update:   Rc<dyn Fn(&dyn AnyWidget)>,
    props_eq: Rc<dyn Fn(&Box<dyn Any>) -> bool>,
    props:    Box<dyn Any>,   // snapshot stored by Reconciler after each update
}

impl VNode {
    /// Component with no output events.
    pub fn new<C: Component>(props: C::Props) -> Self;

    /// Component with an output handler.
    pub fn with_output<C: Component>(
        props: C::Props,
        on_output: impl Fn(C::Output) + 'static,
    ) -> Self;

    /// Set the reconciliation key (should be stable across renders).
    pub fn key(mut self, key: impl Into<String>) -> Self;
}
```

`props_eq` is a closure that captures the new props and compares them against the `Box<dyn Any>`
stored from the previous render, using `downcast_ref::<C::Props>()` + `PartialEq`.

### `AnyWidget` (object-safe base)

```rust
pub trait AnyWidget {
    fn widget(&self) -> gtk::Widget;
    fn as_any(&self) -> &dyn Any;
    fn type_id(&self) -> TypeId;
}

impl<C: Component> AnyWidget for C { … }
```

### `Reconciler`

Lives in `crates/waft-ui-gtk/src/vdom/reconciler.rs`.

```rust
pub struct Reconciler {
    children:  IndexMap<String, ReconcilerEntry>,
    container: gtk::Box,
}

struct ReconcilerEntry {
    component:  Box<dyn AnyWidget>,
    last_props: Box<dyn Any>,
    type_id:    TypeId,
}

impl Reconciler {
    pub fn new(container: gtk::Box) -> Self;

    pub fn reconcile(&mut self, nodes: impl IntoIterator<Item = VNode>);
}
```

`reconcile()` algorithm:

1. Collect `nodes` into a `Vec<VNode>` and build a `seen: HashSet<String>` of keys.
   Nodes without an explicit key receive a positional key (`"$0"`, `"$1"`, …).
2. For each `(key, vnode)` in order:
   - **Missing key** → call `(vnode.build)()`, append `widget()` to container, store entry.
   - **Type mismatch** → remove old `widget()` from container, call `(vnode.build)()`, insert.
   - **Props equal** → `(vnode.props_eq)(&entry.last_props)` returns `true` → skip.
   - **Props changed** → call `(vnode.update)(&*entry.component)`, store `vnode.props`.
3. Remove entries whose keys are absent from `seen`, calling `container.remove()` for each.
4. Restore stable ordering in the GTK container using `gtk::Box::reorder_child_after()` if
   the order of existing children changed.

### Usage example (smart container)

```rust
struct BluetoothPageState {
    adapters_box:        gtk::Box,
    adapters_reconciler: Reconciler,
    devices_reconciler:  Reconciler,
}

// In subscription callback:
state.adapters_reconciler.reconcile(
    adapters.iter().map(|(urn, adapter)| {
        let urn = urn.clone();
        let cb  = action_callback.clone();
        VNode::with_output::<AdapterGroup>(
            AdapterGroupProps::from(adapter),
            move |output| dispatch_adapter_action(&cb, &urn, output),
        )
        .key(&urn)
    })
);
```

---

## Phase 2: Incremental Primitives

`VNode` gains a `kind: VNodeKind` field. Phase 1 nodes use `VNodeKind::Component(ComponentDesc)`.
Phase 2 adds:

```rust
pub enum VNodeKind {
    Component(ComponentDesc),
    Box(VBox),
    Label(VLabel),
    Button(VButton),
    Switch(VSwitch),
    Image(VImage),
    Spinner(VSpinner),
    // added only when a concrete migration requires them
}

pub struct VBox {
    pub orientation: gtk::Orientation,
    pub spacing:     i32,
    pub css_classes: Vec<&'static str>,
    pub children:    Vec<VNode>,
}

pub struct VLabel {
    pub text:        String,
    pub css_classes: Vec<&'static str>,
}

pub struct VButton {
    pub label:     String,
    pub sensitive: bool,
    pub on_click:  Option<Rc<dyn Fn()>>,
}

pub struct VSwitch {
    pub active:    bool,
    pub sensitive: bool,
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
}
```

**Callback handling for primitives.** Closures have no identity so they cannot be compared. The
Reconciler always disconnects the old signal handler and reconnects with the new closure on every
update where the callback field is `Some`. Signal handler IDs are stored in `ReconcilerEntry`
alongside the GTK widget. This is acceptable because entity-store updates occur at human-interaction
frequency, not animation rate.

A dumb widget migrated to Phase 2 replaces its `build()` + `update()` GTK imperative code with a
`render()` method returning a `VNode` tree. The `Component` trait gains an optional `render()`:

```rust
pub trait Component: 'static {
    // … existing methods …

    /// Phase 2: return a VNode tree instead of managing GTK directly.
    /// When implemented, build() and update() are auto-generated from render().
    fn render(props: &Self::Props) -> VNode { unimplemented!() }
}
```

Phase 2 is purely additive. Phase 1 components are not required to implement `render()`.

---

## Migration Path

### Dumb widget migration (Phase 1)

| Before | After |
|--------|-------|
| `pub fn new(props: Props) -> Self` | `fn build(props: &Props) -> Self` in `impl Component` |
| `pub fn apply_props(&self, props: &Props)` | `fn update(&self, props: &Props)` in `impl Component` |
| `pub fn connect_output<F>(&self, f: F)` | `fn connect_output<F>(&self, f: F)` in `impl Component` |
| `pub fn widget(&self) -> gtk::Widget` | `fn widget(&self) -> gtk::Widget` in `impl Component` |
| Individual setters (`set_name`, etc.) | Private helpers or inlined into `update()` |
| `#[derive(Clone)]` on Props | `#[derive(Clone, PartialEq)]` on Props |

Internal `Rc<RefCell<Option<Box<dyn Fn>>>>` wiring in `build()` is unchanged.

### Smart container migration (Phase 1)

| Before | After |
|--------|-------|
| `HashMap<String, W>` field per component type | `Reconciler` field per list |
| 40-line add/update/remove/wire loop | `reconciler.reconcile(nodes)` |
| `connect_output` called manually at create time | Called by `Reconciler` automatically |
| `container.append` / `container.remove` calls | Managed by `Reconciler` |

### File locations

New module: `crates/waft-ui-gtk/src/vdom/`

```
vdom/
  mod.rs          — pub re-exports (Component, VNode, Reconciler)
  component.rs    — Component trait, AnyWidget
  vnode.rs        — VNode, VNodeKind, ComponentDesc
  reconciler.rs   — Reconciler, ReconcilerEntry
  primitives.rs   — VBox, VLabel, VButton, VSwitch, … (Phase 2)
```

---

## Constraints and Trade-offs

**GTK widget type changes** destroy and recreate the widget. Focus, scroll position, and animation
state are lost. In practice this only happens when a list slot changes its component type, which
does not occur in the current entity-driven UI.

**`Props: PartialEq`** is a new constraint on all component props. Derived `PartialEq` is
sufficient for all current props structs (they contain only `String`, `bool`, `u8`, `Option<u8>`
— no GTK types, no closures).

**`Reconciler` ordering** maintains the order given by `reconcile()` on each call. If entity order
is stable (it is, keyed by URN), GTK `reorder_child_after` calls are rare.

**Phase 2 callback churn** (disconnect + reconnect on every update) is bounded by how often
`update()` is called, which is bounded by `Props: PartialEq` — if props are unchanged, no signal
handler is touched.
