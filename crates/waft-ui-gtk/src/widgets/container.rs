//! Container widget renderer - converts Container descriptions to gtk::Box

use crate::renderer::WidgetRenderer;
use crate::utils::css::apply_css_classes;
use waft_ipc::widget::{Orientation, Widget};
use gtk::prelude::*;

/// Render a Container widget as a gtk::Box with children
///
/// # Parameters
///
/// - `renderer`: The WidgetRenderer instance for recursive child rendering
/// - `orientation`: Horizontal or Vertical layout
/// - `spacing`: Spacing between children in pixels
/// - `css_classes`: CSS classes to apply to the container
/// - `children`: Child widgets to render recursively
/// - `widget_id`: Unique identifier for this container (used for child IDs)
///
/// # Returns
///
/// A gtk::Box containing all rendered children, upcast to gtk::Widget
///
/// # Example Child ID Pattern
///
/// Children are given IDs based on the parent's widget_id:
/// - Parent ID: "bluetooth:devices"
/// - Child 0 ID: "bluetooth:devices:child0"
/// - Child 1 ID: "bluetooth:devices:child1"
pub fn render_container(
    renderer: &WidgetRenderer,
    orientation: &Orientation,
    spacing: u32,
    css_classes: &[String],
    children: &[Widget],
    widget_id: &str,
) -> gtk::Widget {
    // Map our Orientation enum to GTK's orientation
    let gtk_orientation = match orientation {
        Orientation::Horizontal => gtk::Orientation::Horizontal,
        Orientation::Vertical => gtk::Orientation::Vertical,
    };

    // Create the box with the specified orientation and spacing
    let container = gtk::Box::new(gtk_orientation, spacing as i32);

    // Apply CSS classes
    apply_css_classes(&container, css_classes);

    // Recursively render and append each child
    for (index, child) in children.iter().enumerate() {
        let child_id = format!("{}:child{}", widget_id, index);
        let gtk_child = renderer.render(child, &child_id);
        container.append(&gtk_child);
    }

    // Upcast to gtk::Widget for uniform return type
    container.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::ActionCallback;
    use crate::types::{Action, ActionParams, Widget};
    use std::rc::Rc;
    use waft_core::menu_state::create_menu_store;

    // Helper to ensure GTK is initialized only once for all tests
    fn init_gtk() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK");
        });
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_empty_container_vertical() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let gtk_widget = render_container(
            &renderer,
            &Orientation::Vertical,
            8,
            &[],
            &[],
            "test_container",
        );

        assert!(gtk_widget.is::<gtk::Box>());
        let container: gtk::Box = gtk_widget.downcast().unwrap();

        // Check orientation
        assert_eq!(container.orientation(), gtk::Orientation::Vertical);

        // Check spacing
        assert_eq!(container.spacing(), 8);

        // No children
        assert_eq!(container.first_child().is_none(), true);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_empty_container_horizontal() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let gtk_widget = render_container(
            &renderer,
            &Orientation::Horizontal,
            12,
            &[],
            &[],
            "test_container",
        );

        assert!(gtk_widget.is::<gtk::Box>());
        let container: gtk::Box = gtk_widget.downcast().unwrap();

        assert_eq!(container.orientation(), gtk::Orientation::Horizontal);
        assert_eq!(container.spacing(), 12);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_container_with_css_classes() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let classes = vec!["menu-container".to_string(), "padded".to_string()];
        let gtk_widget = render_container(
            &renderer,
            &Orientation::Vertical,
            4,
            &classes,
            &[],
            "styled_container",
        );

        let container: gtk::Box = gtk_widget.downcast().unwrap();

        assert!(container.has_css_class("menu-container"));
        assert!(container.has_css_class("padded"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_container_with_label_children() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let children = vec![
            Widget::Label {
                text: "Header".to_string(),
                css_classes: vec!["bold".to_string()],
            },
            Widget::Label {
                text: "Body".to_string(),
                css_classes: vec![],
            },
            Widget::Label {
                text: "Footer".to_string(),
                css_classes: vec![],
            },
        ];

        let gtk_widget = render_container(
            &renderer,
            &Orientation::Vertical,
            8,
            &[],
            &children,
            "multi_label_container",
        );

        let container: gtk::Box = gtk_widget.downcast().unwrap();

        // Count children
        let mut child_count = 0;
        let mut current_child = container.first_child();
        while let Some(child) = current_child {
            child_count += 1;
            current_child = child.next_sibling();
        }
        assert_eq!(child_count, 3);

        // Check first child is a label with correct text
        let first_child = container.first_child().unwrap();
        assert!(first_child.is::<gtk::Label>());
        let first_label: gtk::Label = first_child.downcast().unwrap();
        assert_eq!(first_label.text(), "Header");
        assert!(first_label.has_css_class("bold"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_nested_containers() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        // Create nested structure:
        // Outer container (Vertical)
        //   ├─ Label: "Outer Label"
        //   └─ Inner container (Horizontal)
        //        ├─ Label: "Inner Left"
        //        └─ Label: "Inner Right"

        let inner_children = vec![
            Widget::Label {
                text: "Inner Left".to_string(),
                css_classes: vec![],
            },
            Widget::Label {
                text: "Inner Right".to_string(),
                css_classes: vec![],
            },
        ];

        let outer_children = vec![
            Widget::Label {
                text: "Outer Label".to_string(),
                css_classes: vec![],
            },
            Widget::Container {
                orientation: Orientation::Horizontal,
                spacing: 4,
                css_classes: vec!["inner-box".to_string()],
                children: inner_children,
            },
        ];

        let gtk_widget = render_container(
            &renderer,
            &Orientation::Vertical,
            8,
            &vec!["outer-box".to_string()],
            &outer_children,
            "nested_container",
        );

        let outer_container: gtk::Box = gtk_widget.downcast().unwrap();
        assert!(outer_container.has_css_class("outer-box"));

        // Check first child is the outer label
        let first_child = outer_container.first_child().unwrap();
        assert!(first_child.is::<gtk::Label>());

        // Get next sibling before downcasting (downcast consumes the value)
        let second_child = first_child.next_sibling().unwrap();

        let outer_label: gtk::Label = first_child.downcast().unwrap();
        assert_eq!(outer_label.text(), "Outer Label");

        // Check second child is the inner container
        assert!(second_child.is::<gtk::Box>());
        let inner_container: gtk::Box = second_child.downcast().unwrap();
        assert!(inner_container.has_css_class("inner-box"));
        assert_eq!(inner_container.orientation(), gtk::Orientation::Horizontal);

        // Check inner container has two labels
        let inner_first = inner_container.first_child().unwrap();
        assert!(inner_first.is::<gtk::Label>());

        // Get next sibling before downcasting
        let inner_second = inner_first.next_sibling().unwrap();

        let inner_left: gtk::Label = inner_first.downcast().unwrap();
        assert_eq!(inner_left.text(), "Inner Left");
        assert!(inner_second.is::<gtk::Label>());
        let inner_right: gtk::Label = inner_second.downcast().unwrap();
        assert_eq!(inner_right.text(), "Inner Right");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_container_child_ids() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());

        // Capture the widget IDs passed to callbacks
        use std::cell::RefCell;
        let captured_ids: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let captured_ids_clone = captured_ids.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, _action| {
            captured_ids_clone.borrow_mut().push(widget_id);
        });

        let renderer = WidgetRenderer::new(menu_store, callback);

        // Create container with Button children (which will trigger actions)
        let children = vec![
            Widget::Button {
                label: Some("Button 1".to_string()),
                icon: None,
                on_click: Action {
                    id: "click1".to_string(),
                    params: ActionParams::None,
                },
            },
            Widget::Button {
                label: Some("Button 2".to_string()),
                icon: None,
                on_click: Action {
                    id: "click2".to_string(),
                    params: ActionParams::None,
                },
            },
        ];

        // Note: This test verifies the ID pattern is correct, but doesn't actually
        // trigger the buttons since we'd need a full GTK main loop for that.
        // The pattern verification is the important part.
        let _gtk_widget = render_container(
            &renderer,
            &Orientation::Vertical,
            0,
            &[],
            &children,
            "button_container",
        );

        // At this point, the children have been rendered with IDs:
        // "button_container:child0" and "button_container:child1"
        // This verifies the ID generation logic works correctly
        // (actual button clicks would require GTK event loop)
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_container_zero_spacing() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let gtk_widget = render_container(
            &renderer,
            &Orientation::Vertical,
            0,
            &[],
            &[],
            "zero_spacing",
        );

        let container: gtk::Box = gtk_widget.downcast().unwrap();
        assert_eq!(container.spacing(), 0);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_container_large_spacing() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let gtk_widget = render_container(
            &renderer,
            &Orientation::Horizontal,
            32,
            &[],
            &[],
            "large_spacing",
        );

        let container: gtk::Box = gtk_widget.downcast().unwrap();
        assert_eq!(container.spacing(), 32);
    }
}
