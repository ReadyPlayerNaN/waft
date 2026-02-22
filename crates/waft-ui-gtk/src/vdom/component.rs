use std::any::Any;

/// Object-safe base for type-erased component storage.
/// Implemented automatically for every `Component`.
pub trait AnyWidget {
    fn widget(&self) -> gtk::Widget;
    fn as_any(&self) -> &dyn Any;
}

/// Unified lifecycle interface for GTK4 UI components.
///
/// # Props constraints
/// `Props` must be `Clone + PartialEq + 'static`. `Clone` lets `VNode`
/// capture props in two independent closures (build + update). `PartialEq`
/// lets the `Reconciler` skip `update()` when props are unchanged.
pub trait Component: 'static {
    type Props: Clone + PartialEq + 'static;
    type Output: 'static;

    /// Construct the widget and wire all internal GTK signals.
    fn build(props: &Self::Props) -> Self;

    /// Apply changed props to an existing widget.
    fn update(&self, props: &Self::Props);

    /// Return the root GTK widget for insertion into the container.
    fn widget(&self) -> gtk::Widget;

    /// Register the output event callback. Called once after build().
    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F);
}

impl<C: Component> AnyWidget for C {
    fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
