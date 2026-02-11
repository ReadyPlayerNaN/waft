//! Row widget renderer - horizontal layout container

use crate::css::apply_css_classes;
use crate::renderer::WidgetRenderer;
use gtk::prelude::*;
use waft_ipc::widget::Node;

/// Render a Row widget as a horizontal gtk::Box with children.
pub fn render_row(
    renderer: &WidgetRenderer,
    spacing: u32,
    css_classes: &[String],
    children: &[Node],
    widget_id: &str,
) -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, spacing as i32);
    apply_css_classes(&container, css_classes);

    for (index, node) in children.iter().enumerate() {
        let child_id = match &node.key {
            Some(key) => format!("{}:{}", widget_id, key),
            None => format!("{}:child{}", widget_id, index),
        };
        let gtk_child = renderer.render(&node.widget, &child_id);
        if let Some(ref key) = node.key {
            gtk_child.set_widget_name(key);
        }
        container.append(&gtk_child);
    }

    container.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;
    use crate::renderer::ActionCallback;
    use crate::types::Widget;
    use std::rc::Rc;
    use waft_core::menu_state::create_menu_store;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_row_empty() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let widget = render_row(&renderer, 8, &[], &[], "test_row");

        let container: gtk::Box = widget.downcast().unwrap();
        assert_eq!(container.orientation(), gtk::Orientation::Horizontal);
        assert_eq!(container.spacing(), 8);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_row_with_children() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let children: Vec<Node> = vec![
            Widget::Label {
                text: "Left".to_string(),
                css_classes: vec![],
            }
            .into(),
            Widget::Label {
                text: "Right".to_string(),
                css_classes: vec![],
            }
            .into(),
        ];

        let widget = render_row(&renderer, 12, &[], &children, "row_with_kids");

        let container: gtk::Box = widget.downcast().unwrap();
        assert_eq!(container.orientation(), gtk::Orientation::Horizontal);

        let first = container.first_child().unwrap();
        let label: gtk::Label = first.downcast().unwrap();
        assert_eq!(label.text(), "Left");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_row_with_css() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let classes = vec!["header-row".to_string()];
        let widget = render_row(&renderer, 0, &classes, &[], "styled_row");

        let container: gtk::Box = widget.downcast().unwrap();
        assert!(container.has_css_class("header-row"));
    }
}
