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

use std::cell::RefCell;
use std::rc::Rc;

/// Callback type used to emit output events from a rendered component.
pub type RenderCallback<T> = Rc<RefCell<Option<Box<dyn Fn(T)>>>>;

/// A component that declares its GTK content as a pure function of props.
///
/// Implement this trait instead of `Component` when your widget is fully
/// described by its props and does not store individual widget references.
/// Wrap with `RenderComponent<F>` to satisfy the `Component` trait.
///
/// # Example
/// ```rust,ignore
/// struct MyRow;
/// impl RenderFn for MyRow {
///     type Props  = MyRowProps;
///     type Output = ();
///     fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
///         VNode::label(VLabel::new(&props.text).css_class("row"))
///     }
/// }
/// type MyRowComponent = RenderComponent<MyRow>;
/// ```
pub trait RenderFn: 'static {
    type Props: Clone + PartialEq + 'static;
    type Output: 'static;

    /// Return a `VNode` describing the full content of this component.
    ///
    /// `emit` is the output callback — clone it into button/switch closures
    /// to fire output events. For `Output = ()` it can be ignored.
    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> super::VNode;
}
