use std::cell::RefCell;
use std::rc::Rc;

use super::component::{Component, RenderCallback, RenderFn};
use super::reconciler::SingleChildReconciler;

/// Implements `Component` for any type that implements `RenderFn`.
///
/// `widget()` returns the rendered widget directly — there is no wrapping
/// `gtk::Box`. The rendered `VNode` is reconciled via `SingleChildReconciler`,
/// which holds the single entry without a container widget.
/// On every `update()`, `F::render()` is called and the result diffed.
pub struct RenderComponent<F: RenderFn> {
    reconciler: RefCell<SingleChildReconciler>,
    emit:       RenderCallback<F::Output>,
}

impl<F: RenderFn> Component for RenderComponent<F> {
    type Props  = F::Props;
    type Output = F::Output;

    fn build(props: &Self::Props) -> Self {
        let emit: RenderCallback<F::Output> = Rc::new(RefCell::new(None));
        let mut reconciler = SingleChildReconciler::new();
        reconciler.reconcile(F::render(props, &emit));
        Self { reconciler: RefCell::new(reconciler), emit }
    }

    fn update(&self, props: &Self::Props) {
        self.reconciler.borrow_mut().reconcile(F::render(props, &self.emit));
    }

    fn widget(&self) -> gtk::Widget {
        self.reconciler.borrow().widget()
    }

    fn connect_output<G: Fn(Self::Output) + 'static>(&self, callback: G) {
        *self.emit.borrow_mut() = Some(Box::new(callback));
    }
}
