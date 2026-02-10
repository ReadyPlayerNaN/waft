//! ListRow widget renderer — a horizontal row of children with CSS classes.

use crate::css::apply_css_classes;
use crate::renderer::WidgetRenderer;
use gtk::prelude::*;
use waft_ipc::widget::Node;

/// Render a ListRow widget as a horizontal gtk::Box with children.
pub(crate) fn render_list_row(
    renderer: &WidgetRenderer,
    children: &[Node],
    css_classes: &[String],
    widget_id: &str,
) -> gtk::Widget {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();

    apply_css_classes(&row, css_classes);

    for (i, child) in children.iter().enumerate() {
        let child_id = match &child.key {
            Some(key) => format!("{}:{}", widget_id, key),
            None => format!("{}:{}", widget_id, i),
        };
        let gtk_child = renderer.render(&child.widget, &child_id);
        row.append(&gtk_child);
    }

    row.upcast()
}
