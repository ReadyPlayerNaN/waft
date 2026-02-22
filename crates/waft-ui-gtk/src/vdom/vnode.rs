use std::any::{Any, TypeId};
use std::rc::Rc;

use super::component::{AnyWidget, Component};
use super::primitives::{VBox, VButton, VLabel, VSwitch};

// -- Component descriptor (what VNode currently stores at the top level) ---

type BuildFn   = Rc<dyn Fn() -> Box<dyn AnyWidget>>;
type UpdateFn  = Rc<dyn Fn(&dyn AnyWidget)>;
type PropsEqFn = Rc<dyn Fn(&Box<dyn Any>) -> bool>;

pub(super) struct ComponentDesc {
    pub(super) type_id:  TypeId,
    pub(super) build:    BuildFn,
    pub(super) update:   UpdateFn,
    pub(super) props_eq: PropsEqFn,
    pub(super) props:    Box<dyn Any>,
}

// -- VNodeKind ----------------------------------------------------------------

pub(super) enum VNodeKind {
    Component(ComponentDesc),
    Label(VLabel),
    Box(VBox),
    Button(VButton),
    Switch(VSwitch),
}

// -- VNode --------------------------------------------------------------------

/// A type-erased description of a UI element — either a custom `Component`
/// or a GTK primitive — with optional reconciliation key and output handler.
pub struct VNode {
    pub(super) key:  Option<String>,
    pub(super) kind: VNodeKind,
}

impl VNode {
    // -- Component constructors (existing public API — unchanged) ----------

    /// Component with no output events.
    pub fn new<C: Component>(props: C::Props) -> Self {
        Self { key: None, kind: VNodeKind::Component(make_component_desc::<C>(props, |_| {})) }
    }

    /// Component with an output handler.
    pub fn with_output<C: Component>(
        props: C::Props,
        on_output: impl Fn(C::Output) + 'static,
    ) -> Self {
        Self { key: None, kind: VNodeKind::Component(make_component_desc::<C>(props, on_output)) }
    }

    /// Set the reconciliation key. Use a stable identifier (e.g. URN string).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    // -- Primitive constructors -------------------------------------------

    /// Build a `VLabel` descriptor and wrap it in a `VNode`.
    pub fn label(v: VLabel) -> Self {
        Self { key: None, kind: VNodeKind::Label(v) }
    }

    /// Build a `VBox` descriptor and wrap it in a `VNode`.
    pub fn vbox(v: VBox) -> Self {
        Self { key: None, kind: VNodeKind::Box(v) }
    }

    /// Build a `VButton` descriptor and wrap it in a `VNode`.
    pub fn button(v: VButton) -> Self {
        Self { key: None, kind: VNodeKind::Button(v) }
    }

    /// Build a `VSwitch` descriptor and wrap it in a `VNode`.
    pub fn switch(v: VSwitch) -> Self {
        Self { key: None, kind: VNodeKind::Switch(v) }
    }
}

// -- Internal helper ----------------------------------------------------------

fn make_component_desc<C: Component>(
    props: C::Props,
    on_output: impl Fn(C::Output) + 'static,
) -> ComponentDesc {
    let props_build  = props.clone();
    let props_update = props.clone();
    let props_eq     = props.clone();
    let on_output    = Rc::new(on_output);

    ComponentDesc {
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
