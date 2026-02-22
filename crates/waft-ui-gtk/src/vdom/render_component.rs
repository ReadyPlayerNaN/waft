use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use super::component::{Component, RenderCallback, RenderFn};
use super::reconciler::Reconciler;

/// Implements `Component` for any type that implements `RenderFn`.
///
/// The root widget is a transparent `gtk::Box` (vertical, spacing=0).
/// The rendered `VNode` is reconciled as the single child of that box.
/// On every `update()`, `F::render()` is called and the result diffed.
pub struct RenderComponent<F: RenderFn> {
    root:       gtk::Box,
    reconciler: RefCell<Reconciler>,
    emit:       RenderCallback<F::Output>,
}

impl<F: RenderFn> Component for RenderComponent<F> {
    type Props  = F::Props;
    type Output = F::Output;

    fn build(props: &Self::Props) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let emit: RenderCallback<F::Output> = Rc::new(RefCell::new(None));
        let mut reconciler = Reconciler::new(root.clone());
        reconciler.reconcile(std::iter::once(F::render(props, &emit)));
        Self { root, reconciler: RefCell::new(reconciler), emit }
    }

    fn update(&self, props: &Self::Props) {
        let vnode = F::render(props, &self.emit);
        self.reconciler.borrow_mut().reconcile(std::iter::once(vnode));
    }

    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn connect_output<G: Fn(Self::Output) + 'static>(&self, callback: G) {
        *self.emit.borrow_mut() = Some(Box::new(callback));
    }
}
