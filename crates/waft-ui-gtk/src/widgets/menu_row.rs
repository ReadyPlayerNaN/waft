//! MenuRow widget renderer - converts MenuRow descriptions to GTK button with layout

use crate::renderer::{ActionCallback, WidgetRenderer};
use crate::widgets::icon::IconWidget;
use gtk::prelude::*;
use waft_ipc::widget::{Action, Widget};

/// Render a MenuRow widget as a gtk::Button with horizontal layout
///
/// Structure: Button → Box(Horizontal) → [Icon, Label, [Spinner (if busy)], Trailing widget]
///
/// # Parameters
///
/// - `renderer`: The WidgetRenderer instance for recursive rendering of trailing widget
/// - `callback`: The action callback for handling clicks
/// - `icon`: Optional themed icon name
/// - `label`: Main text label
/// - `trailing`: Optional trailing widget (rendered recursively)
/// - `sensitive`: Whether the button should be clickable
/// - `busy`: Whether the row is busy (shows spinner, disables click)
/// - `on_click`: Optional action to trigger on click
/// - `widget_id`: Unique identifier for this menu row
///
/// # Returns
///
/// A gtk::Button containing the menu row layout, upcast to gtk::Widget
pub fn render_menu_row(
    renderer: &WidgetRenderer,
    callback: &ActionCallback,
    icon: &Option<String>,
    label: &str,
    trailing: &Option<Box<Widget>>,
    sensitive: bool,
    busy: bool,
    on_click: &Option<Action>,
    widget_id: &str,
) -> gtk::Widget {
    let button = gtk::Button::new();
    // Button is not clickable when busy or not sensitive
    button.set_sensitive(sensitive && !busy);

    // Create horizontal container
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    // Add icon if provided
    if let Some(icon_name) = icon {
        let icon_widget = IconWidget::from_name(icon_name, 24);
        container.append(icon_widget.widget());
    }

    // Main label
    let main_label = gtk::Label::new(Some(label));
    main_label.set_hexpand(true); // Label should take available space
    main_label.set_halign(gtk::Align::Start);
    main_label.set_valign(gtk::Align::Center);
    container.append(&main_label);

    // Add spinner if busy (left of trailing widget)
    if busy {
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        container.append(&spinner);
    }

    // Add trailing widget if provided (recursively rendered)
    if let Some(trailing_widget) = trailing {
        let trailing_id = format!("{}:trailing", widget_id);
        let gtk_trailing = renderer.render(trailing_widget, &trailing_id);
        container.append(&gtk_trailing);
    }

    button.set_child(Some(&container));

    // Connect click handler if action is provided
    if let Some(action) = on_click {
        let widget_id = widget_id.to_string();
        let action = action.clone();
        let callback = callback.clone();

        button.connect_clicked(move |_button| {
            callback(widget_id.clone(), action.clone());
        });
    }

    button.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;
    use crate::types::{ActionParams, Widget};
    use std::cell::RefCell;
    use std::rc::Rc;
    use waft_core::menu_state::create_menu_store;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_minimal() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Test Label",
            &None,
            true,
            false,
            &None,
            "test_row",
        );

        assert!(widget.is::<gtk::Button>());
        let button: gtk::Button = widget.downcast().unwrap();
        assert!(button.is_sensitive());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_with_icon() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let widget = render_menu_row(
            &renderer,
            &callback,
            &Some("audio-volume-high-symbolic".to_string()),
            "Volume",
            &None,
            true,
            false,
            &None,
            "volume_row",
        );

        let button: gtk::Button = widget.downcast().unwrap();
        let container = button.child().unwrap();
        assert!(container.is::<gtk::Box>());

        // Container should have children (icon + labels box)
        let container_box: gtk::Box = container.downcast().unwrap();
        let first_child = container_box.first_child();
        assert!(first_child.is_some());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_busy() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Loading...",
            &None,
            true,
            true, // busy = true
            &None,
            "busy_row",
        );

        let button: gtk::Button = widget.downcast().unwrap();
        assert!(!button.is_sensitive()); // Should be insensitive when busy
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_insensitive() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Disabled",
            &None,
            false,
            false,
            &None,
            "disabled_row",
        );

        let button: gtk::Button = widget.downcast().unwrap();
        assert!(!button.is_sensitive());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_with_switch_trailing() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let trailing = Some(Box::new(Widget::Switch {
            active: true,
            sensitive: true,
            on_toggle: Action {
                id: "toggle_switch".to_string(),
                params: ActionParams::None,
            },
        }));

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Toggle Option",
            &trailing,
            true,
            false,
            &None,
            "row_with_switch",
        );

        let button: gtk::Button = widget.downcast().unwrap();
        let container = button.child().unwrap();
        let container_box: gtk::Box = container.downcast().unwrap();

        // Should have labels box and trailing switch
        // Verify at least there are multiple children
        assert!(container_box.first_child().is_some());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_with_checkmark_trailing() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let trailing = Some(Box::new(Widget::Checkmark { visible: true }));

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Selected Item",
            &trailing,
            true,
            false,
            &None,
            "row_with_checkmark",
        );

        assert!(widget.is::<gtk::Button>());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_with_spinner_trailing() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let trailing = Some(Box::new(Widget::Spinner { spinning: true }));

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Loading...",
            &trailing,
            true,
            false,
            &None,
            "row_with_spinner",
        );

        assert!(widget.is::<gtk::Button>());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_with_click_action() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let action = Some(Action {
            id: "row_clicked".to_string(),
            params: ActionParams::None,
        });

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Clickable Row",
            &None,
            true,
            false,
            &action,
            "clickable_row",
        );

        let button: gtk::Button = widget.downcast().unwrap();

        // Simulate a click
        button.emit_clicked();

        // Verify the callback was invoked
        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "clickable_row");
        assert_eq!(actions[0].1.id, "row_clicked");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_no_click_action() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let widget = render_menu_row(
            &renderer,
            &callback,
            &None,
            "Non-clickable Row",
            &None,
            true,
            false,
            &None,
            "nonclickable_row",
        );

        let button: gtk::Button = widget.downcast().unwrap();

        // Simulate a click
        button.emit_clicked();

        // Verify no callback was invoked
        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 0);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_menu_row_full_featured() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback.clone());

        let trailing = Some(Box::new(Widget::Switch {
            active: false,
            sensitive: true,
            on_toggle: Action {
                id: "toggle".to_string(),
                params: ActionParams::None,
            },
        }));

        let action = Some(Action {
            id: "open_settings".to_string(),
            params: ActionParams::None,
        });

        let widget = render_menu_row(
            &renderer,
            &callback,
            &Some("preferences-system-symbolic".to_string()),
            "Settings",
            &trailing,
            true,
            false,
            &action,
            "full_featured_row",
        );

        let button: gtk::Button = widget.downcast().unwrap();
        assert!(button.is_sensitive());

        // Just verify it renders without errors
        // Full verification would require deep widget tree inspection
    }
}
