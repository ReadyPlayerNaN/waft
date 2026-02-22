# Implement VRevealer

## Why

`gtk::Revealer` is used in 9 files across the codebase for show/hide animations:
- `SliderWidget` (expand button reveal)
- `FeatureToggleWidget` (menu content)
- `NotificationCard` (card show/hide)
- `AgendaCard` (details expand)
- `AgendaComponent` (past events)
- `AudioSliders` (slider sections)
- `events.rs` (event content)
- `FeatureGridWidget` (menu content)
- `notification_group.rs` (group content)

Currently, any `RenderFn` component that needs a Revealer must fall back to imperative code or use a parent that manages the Revealer externally. Adding `VRevealer` as a virtual DOM primitive enables fully declarative show/hide animations in `RenderFn` components.

## What Changes

1. **Add** `VRevealer` primitive descriptor to `crates/waft-ui-gtk/src/vdom/primitives.rs`
2. **Add** `Revealer` variant to `VNodeKind` enum in `crates/waft-ui-gtk/src/vdom/vnode.rs`
3. **Add** `Revealer` entry to `ReconcilerEntry` and implement build/update in `crates/waft-ui-gtk/src/vdom/reconciler.rs`
4. **Add** `Revealer` to `KindTag` enum in reconciler
5. **Re-export** `VRevealer` from `crates/waft-ui-gtk/src/vdom/mod.rs`
6. **Add** tests for the new primitive

## Affected Files

- `crates/waft-ui-gtk/src/vdom/primitives.rs` -- add `VRevealer` struct
- `crates/waft-ui-gtk/src/vdom/vnode.rs` -- add `VNodeKind::Revealer`, add `VNode::revealer()` constructor
- `crates/waft-ui-gtk/src/vdom/reconciler.rs` -- add `KindTag::Revealer`, `ReconcilerEntry::Revealer`, `build_revealer_entry`, update_entry arm
- `crates/waft-ui-gtk/src/vdom/mod.rs` -- add `VRevealer` to re-exports

## Tasks

### 1. Add `VRevealer` descriptor to `crates/waft-ui-gtk/src/vdom/primitives.rs`

```rust
/// Descriptor for a `gtk::Revealer` container VNode.
pub struct VRevealer {
    pub reveal:          bool,
    pub transition_type: gtk::RevealerTransitionType,
    pub transition_duration: u32,
    pub child:           Box<super::VNode>,
}

impl VRevealer {
    pub fn new(reveal: bool, child: super::VNode) -> Self {
        Self {
            reveal,
            transition_type: gtk::RevealerTransitionType::SlideDown,
            transition_duration: 200,
            child: Box::new(child),
        }
    }

    pub fn transition_type(mut self, t: gtk::RevealerTransitionType) -> Self {
        self.transition_type = t;
        self
    }

    pub fn transition_duration(mut self, ms: u32) -> Self {
        self.transition_duration = ms;
        self
    }
}
```

### 2. Add `Revealer` variant to `VNodeKind` in `crates/waft-ui-gtk/src/vdom/vnode.rs`

Add to the `VNodeKind` enum:
```rust
Revealer(VRevealer),
```

Add the constructor method on `VNode`:
```rust
/// Build a `VRevealer` descriptor and wrap it in a `VNode`.
pub fn revealer(v: VRevealer) -> Self {
    Self { key: None, kind: VNodeKind::Revealer(v) }
}
```

Add the import of `VRevealer` to the `use` statement at the top.

### 3. Add `KindTag::Revealer` to `crates/waft-ui-gtk/src/vdom/reconciler.rs`

Add to the `KindTag` enum:
```rust
Revealer,
```

### 4. Add `ReconcilerEntry::Revealer` to `crates/waft-ui-gtk/src/vdom/reconciler.rs`

Add to the `ReconcilerEntry` enum:
```rust
Revealer {
    widget:           gtk::Revealer,
    child_reconciler: std::boxed::Box<Reconciler<gtk::Box>>,
},
```

The child is reconciled inside a `gtk::Box` that is set as the Revealer's child widget.

### 5. Update `ReconcilerEntry::widget()` in reconciler

Add the match arm:
```rust
Self::Revealer { widget, .. } => widget.clone().upcast(),
```

### 6. Update `ReconcilerEntry::kind_tag()` in reconciler

Add the match arm:
```rust
Self::Revealer { .. } => KindTag::Revealer,
```

### 7. Add `kind_tag_of` match arm in reconciler

Add:
```rust
VNodeKind::Revealer(_) => KindTag::Revealer,
```

### 8. Add `build_entry` match arm in reconciler

Add:
```rust
VNodeKind::Revealer(vrev) => build_revealer_entry(vrev),
```

### 9. Implement `build_revealer_entry` in reconciler

```rust
fn build_revealer_entry(vrev: VRevealer) -> ReconcilerEntry {
    let widget = gtk::Revealer::builder()
        .transition_type(vrev.transition_type)
        .transition_duration(vrev.transition_duration)
        .reveal_child(vrev.reveal)
        .build();

    let child_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    widget.set_child(Some(&child_container));

    let mut child_reconciler: std::boxed::Box<Reconciler<gtk::Box>> =
        std::boxed::Box::new(Reconciler::new(child_container));
    child_reconciler.reconcile(std::iter::once(*vrev.child));

    ReconcilerEntry::Revealer { widget, child_reconciler }
}
```

### 10. Add `update_entry` match arm in reconciler

Add to the `update_entry` function's match:

```rust
(ReconcilerEntry::Revealer { widget, child_reconciler },
 VNodeKind::Revealer(vrev)) => {
    widget.set_reveal_child(vrev.reveal);
    widget.set_transition_type(vrev.transition_type);
    widget.set_transition_duration(vrev.transition_duration);
    child_reconciler.reconcile(std::iter::once(*vrev.child));
}
```

### 11. Re-export `VRevealer` from `crates/waft-ui-gtk/src/vdom/mod.rs`

Add `VRevealer` to the `pub use primitives::{...}` line.

### 12. Add tests in `crates/waft-ui-gtk/src/vdom/tests.rs`

Add test cases:
- `revealer_build_creates_widget` -- build a VRevealer and verify the gtk::Revealer is created with correct props
- `revealer_update_toggles_reveal` -- reconcile with `reveal: true`, then `reveal: false`, verify `reveals_child()` changes
- `revealer_child_reconciled` -- verify the child VNode content is rendered inside the revealer

### 13. Run `cargo build --workspace` and `cargo test --workspace`

Verify everything compiles and all vdom tests pass.
