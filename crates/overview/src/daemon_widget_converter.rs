//! Converts serializable daemon widgets (waft_ipc::Widget) to GTK widgets (waft_plugin_api::Widget)
//!
//! This module bridges the IPC protocol layer with the GTK rendering layer.

use gtk::prelude::*;
use std::rc::Rc;
use waft_ipc::NamedWidget;
use waft_plugin_api::{Slot, Widget};

/// Convert a NamedWidget from the IPC protocol to a GTK Widget for rendering
///
/// This is a placeholder implementation that creates a basic label for now.
/// TODO: Implement proper rendering for all widget types (FeatureToggle, Slider, etc.)
pub fn convert_daemon_widget(named_widget: &NamedWidget) -> Option<Rc<Widget>> {
    // For now, create a simple label widget as a placeholder
    let label = gtk::Label::new(Some(&format!(
        "Daemon widget: {} (TODO: implement rendering)",
        named_widget.id
    )));
    label.add_css_class("daemon-widget-placeholder");

    Some(Rc::new(Widget {
        id: named_widget.id.clone(),
        slot: Slot::Info, // Default to Info slot for now
        weight: named_widget.weight as i32,
        el: label.upcast(),
    }))
}

/// Convert a list of NamedWidgets to GTK Widgets
pub fn convert_daemon_widgets(named_widgets: &[NamedWidget]) -> Vec<Rc<Widget>> {
    named_widgets
        .iter()
        .filter_map(convert_daemon_widget)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_ipc::Widget as IpcWidget;

    #[test]
    fn test_convert_basic_widget() {
        // Initialize GTK for tests
        if gtk::is_initialized() {
            let widget = NamedWidget {
                id: "test-widget".to_string(),
                weight: 10,
                widget: IpcWidget::Label {
                    text: "Test".to_string(),
                    css_classes: vec![],
                },
            };

            let converted = convert_daemon_widget(&widget);
            assert!(converted.is_some());

            let gtk_widget = converted.unwrap();
            assert_eq!(gtk_widget.id, "test-widget");
            assert_eq!(gtk_widget.weight, 10);
        }
    }
}
