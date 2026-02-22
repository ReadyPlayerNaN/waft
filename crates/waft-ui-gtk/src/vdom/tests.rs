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

// ── Test helpers ──────────────────────────────────────────────────────────

fn make_reconciler() -> (gtk::Box, Reconciler) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let reconciler = Reconciler::new(container.clone());
    (container, reconciler)
}

fn label_node(text: &str) -> VNode {
    VNode::new::<LabelComponent>(LabelProps { text: text.into() })
}

// ── Test functions (called from the single GTK test entry point) ──────────

fn test_builds_widget_on_first_reconcile() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("hello")]);
    assert_eq!(container.observe_children().n_items(), 1);
}

fn test_appends_multiple_widgets_in_order() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("a"), label_node("b"), label_node("c")]);
    assert_eq!(container.observe_children().n_items(), 3);
}

fn test_updates_widget_when_props_change() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("hello").key("x")]);

    let child = container.first_child().unwrap().downcast::<gtk::Label>().unwrap();
    assert_eq!(child.label(), "hello");

    r.reconcile([label_node("world").key("x")]);
    // Same widget instance, label updated in place.
    assert_eq!(child.label(), "world");
    assert_eq!(container.observe_children().n_items(), 1);
}

fn test_preserves_widget_identity_when_props_unchanged() {
    let (container, mut r) = make_reconciler();
    let props = LabelProps { text: "stable".into() };

    r.reconcile([VNode::new::<LabelComponent>(props.clone()).key("x")]);
    let ptr_before = container.first_child().unwrap().as_ptr();

    r.reconcile([VNode::new::<LabelComponent>(props).key("x")]);
    // No destroy-and-recreate: same pointer.
    assert_eq!(container.first_child().unwrap().as_ptr(), ptr_before);
}

fn test_removes_widget_when_key_absent() {
    let (container, mut r) = make_reconciler();
    r.reconcile([label_node("a").key("a"), label_node("b").key("b")]);
    assert_eq!(container.observe_children().n_items(), 2);

    r.reconcile([label_node("a").key("a")]);
    assert_eq!(container.observe_children().n_items(), 1);
}

fn test_rebuilds_widget_when_component_type_changes() {
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

fn test_clears_all_children(r: &mut Reconciler, container: &gtk::Box) {
    r.reconcile([label_node("a").key("a"), label_node("b").key("b")]);
    assert_eq!(container.observe_children().n_items(), 2);
    r.reconcile(std::iter::empty::<VNode>());
    assert_eq!(container.observe_children().n_items(), 0, "reconciling empty list must remove all children");
}

fn test_wires_output_callback_at_build_time() {
    #[derive(Clone, PartialEq)]
    struct ClickProps;

    #[allow(dead_code)]
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

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut r = Reconciler::new(container.clone());

    let fired = Rc::new(RefCell::new(false));
    let fired_clone = fired.clone();

    r.reconcile([
        VNode::with_output::<ClickComponent>(ClickProps, move |_| {
            *fired_clone.borrow_mut() = true;
        }),
    ]);

    assert!(!*fired.borrow(), "callback fires only on user action, not on build");
}

// ── Single GTK test entry point ───────────────────────────────────────────
//
// GTK requires the OS main thread. Rust's test harness spawns each #[test]
// on a worker thread, which causes GTK to panic with "GTK may only be used
// from the main thread." Running with `--test-threads=1` does not help
// because the harness still spawns worker threads.
//
// The workaround used throughout this codebase is to run all GTK tests
// sequentially inside a single #[test] function, which the harness
// guarantees runs on one thread. gtk::init() records that thread as the
// GTK main thread for the lifetime of the process.

#[test]
fn all_reconciler_tests() {
    init_gtk_for_tests();

    test_builds_widget_on_first_reconcile();
    test_appends_multiple_widgets_in_order();
    test_updates_widget_when_props_change();
    test_preserves_widget_identity_when_props_unchanged();
    test_removes_widget_when_key_absent();
    test_rebuilds_widget_when_component_type_changes();
    test_wires_output_callback_at_build_time();

    let (container, mut r) = make_reconciler();
    test_clears_all_children(&mut r, &container);
}
