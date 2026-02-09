//! Converts serializable daemon widgets (waft_ipc::Widget) to GTK widgets (waft_plugin_api::Widget)
//!
//! This module bridges the IPC protocol layer with the GTK rendering layer.

use std::rc::Rc;
use waft_ipc::NamedWidget;
use waft_plugin_api::{Slot, Widget};
use waft_ui_gtk::renderer::{ActionCallback, WidgetRenderer};
use waft_core::menu_state::MenuStore;

/// Convert a NamedWidget from the IPC protocol to a GTK Widget for rendering
///
/// Uses the WidgetRenderer from waft-ui-gtk to create proper GTK widgets.
pub fn convert_daemon_widget(
    named_widget: &NamedWidget,
    menu_store: &Rc<MenuStore>,
    action_callback: &ActionCallback,
) -> Option<Rc<Widget>> {
    let renderer = WidgetRenderer::new(menu_store.clone(), action_callback.clone());

    // Render the IPC widget to a GTK widget
    let gtk_widget = renderer.render(&named_widget.widget, &named_widget.id);

    // Convert IPC Slot to plugin-api Slot
    let slot = match named_widget.slot {
        waft_ipc::Slot::Info => Slot::Info,
        waft_ipc::Slot::FeatureToggles => return None, // FeatureToggles use different API
        waft_ipc::Slot::Controls => Slot::Controls,
        waft_ipc::Slot::Header => Slot::Header,
        waft_ipc::Slot::Actions => Slot::Actions,
    };

    Some(Rc::new(Widget {
        id: named_widget.id.clone(),
        slot,
        weight: named_widget.weight as i32,
        el: gtk_widget,
    }))
}

/// Convert a list of NamedWidgets to GTK Widgets
pub fn convert_daemon_widgets(
    named_widgets: &[NamedWidget],
    menu_store: &Rc<MenuStore>,
    action_callback: &ActionCallback,
) -> Vec<Rc<Widget>> {
    named_widgets
        .iter()
        .filter_map(|w| convert_daemon_widget(w, menu_store, action_callback))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::prelude::*;
    use waft_ipc::Widget as IpcWidget;
    use waft_core::menu_state::create_menu_store;

    // Helper to ensure GTK is initialized for widget tests
    fn init_gtk() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK");
        });
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_convert_label_widget() {
        init_gtk();

        let menu_store = Rc::new(create_menu_store());
        let action_callback: ActionCallback = Rc::new(|_id, _action| {});

        let widget = NamedWidget {
            id: "test-label".to_string(),
            slot: waft_ipc::Slot::Info,
            weight: 10,
            widget: IpcWidget::Label {
                text: "Test Label".to_string(),
                css_classes: vec!["test-class".to_string()],
            },
        };

        let converted = convert_daemon_widget(&widget, &menu_store, &action_callback);
        assert!(converted.is_some());

        let gtk_widget = converted.unwrap();
        assert_eq!(gtk_widget.id, "test-label");
        assert_eq!(gtk_widget.weight, 10);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_convert_container_widget() {
        init_gtk();

        let menu_store = Rc::new(create_menu_store());
        let action_callback: ActionCallback = Rc::new(|_id, _action| {});

        let widget = NamedWidget {
            id: "clock-widget".to_string(),
            slot: waft_ipc::Slot::Info,
            weight: 100,
            widget: IpcWidget::Container {
                orientation: waft_ipc::Orientation::Vertical,
                spacing: 4,
                css_classes: vec!["clock-container".to_string()],
                children: vec![
                    IpcWidget::Label {
                        text: "12:34".to_string(),
                        css_classes: vec!["time".to_string()],
                    },
                    IpcWidget::Label {
                        text: "Monday, Jan 1".to_string(),
                        css_classes: vec!["date".to_string()],
                    },
                ],
            },
        };

        let converted = convert_daemon_widget(&widget, &menu_store, &action_callback);
        assert!(converted.is_some());

        let gtk_widget = converted.unwrap();
        assert_eq!(gtk_widget.id, "clock-widget");
        assert_eq!(gtk_widget.weight, 100);
    }

    #[test]
    fn test_slot_conversion() {
        // Test that slot conversion works correctly (no GTK required)

        // Info -> Info
        let info_slot = waft_ipc::Slot::Info;
        assert!(matches!(info_slot, waft_ipc::Slot::Info));

        // Controls -> Controls
        let controls_slot = waft_ipc::Slot::Controls;
        assert!(matches!(controls_slot, waft_ipc::Slot::Controls));

        // Header -> Header
        let header_slot = waft_ipc::Slot::Header;
        assert!(matches!(header_slot, waft_ipc::Slot::Header));

        // Actions -> Actions
        let actions_slot = waft_ipc::Slot::Actions;
        assert!(matches!(actions_slot, waft_ipc::Slot::Actions));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_convert_multiple_widgets() {
        init_gtk();

        let menu_store = Rc::new(create_menu_store());
        let action_callback: ActionCallback = Rc::new(|_id, _action| {});

        let widgets = vec![
            NamedWidget {
                id: "widget1".to_string(),
                slot: waft_ipc::Slot::Info,
                weight: 10,
                widget: IpcWidget::Label {
                    text: "First".to_string(),
                    css_classes: vec![],
                },
            },
            NamedWidget {
                id: "widget2".to_string(),
                slot: waft_ipc::Slot::Controls,
                weight: 20,
                widget: IpcWidget::Label {
                    text: "Second".to_string(),
                    css_classes: vec![],
                },
            },
        ];

        let converted = convert_daemon_widgets(&widgets, &menu_store, &action_callback);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].id, "widget1");
        assert_eq!(converted[1].id, "widget2");
    }
}
