//! IconList widget renderer — a list with a leading icon and child widgets.

use crate::renderer::WidgetRenderer;
use crate::widgets::icon::IconWidget;
use gtk::prelude::*;
use waft_ipc::widget::Node;

/// Render an IconList widget as a horizontal box with icon + vertical children.
pub(crate) fn render_icon_list(
    renderer: &WidgetRenderer,
    icon: &str,
    icon_size: i32,
    children: &[Node],
    widget_id: &str,
) -> gtk::Widget {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();

    let icon_widget = IconWidget::from_name(icon, icon_size);
    icon_widget.widget().set_valign(gtk::Align::Start);
    root.append(icon_widget.widget());

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();

    for (i, child) in children.iter().enumerate() {
        let child_id = match &child.key {
            Some(key) => format!("{}:{}", widget_id, key),
            None => format!("{}:{}", widget_id, i),
        };
        let gtk_child = renderer.render(&child.widget, &child_id);
        content.append(&gtk_child);
    }

    root.append(&content);
    root.upcast()
}
