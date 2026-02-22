use std::any::{Any, TypeId};
use std::rc::Rc;

use super::component::{AnyWidget, Component};

type BuildFn    = Rc<dyn Fn() -> Box<dyn AnyWidget>>;
type UpdateFn   = Rc<dyn Fn(&dyn AnyWidget)>;
type PropsEqFn  = Rc<dyn Fn(&Box<dyn Any>) -> bool>;

/// A type-erased description of a component instance with its props and
/// output handler captured as closures.
pub struct VNode {
    pub(super) key:      Option<String>,
    pub(super) type_id:  TypeId,
    /// Builds a fresh component instance and wires the output callback.
    pub(super) build:    BuildFn,
    /// Calls `component.update(props)` with the new props.
    pub(super) update:   UpdateFn,
    /// Returns true if these props equal the stored `last_props` snapshot.
    pub(super) props_eq: PropsEqFn,
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
