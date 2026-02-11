//! Button widget renderer

use crate::renderer::ActionCallback;
use waft_ipc::widget::Action;
use gtk::prelude::*;

/// Render a Button widget
///
/// Maps to gtk::Button with optional label and icon.
/// Connects clicked signal to trigger on_click action.
pub fn render_button(
    callback: &ActionCallback,
    label: &Option<String>,
    icon: &Option<String>,
    on_click: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let button = gtk::Button::new();

    // Set label if provided
    if let Some(label_text) = label {
        button.set_label(label_text);
    }

    // Set icon if provided
    if let Some(icon_name) = icon {
        let image = gtk::Image::from_icon_name(icon_name);
        button.set_child(Some(&image));
    }

    // Clone necessary data for the closure
    let widget_id = widget_id.to_string();
    let on_click = on_click.clone();
    let callback = callback.clone();

    button.connect_clicked(move |_button| {
        callback(widget_id.clone(), on_click.clone());
    });

    button.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;
    use crate::types::ActionParams;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_button_with_label() {
        init_gtk_for_tests();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let action = Action {
            id: "button_click".to_string(),
            params: ActionParams::None,
        };

        let widget = render_button(
            &callback,
            &Some("Click Me".to_string()),
            &None,
            &action,
            "test_button",
        );

        assert!(widget.is::<gtk::Button>());
        let button: gtk::Button = widget.downcast().unwrap();
        assert_eq!(button.label().unwrap(), "Click Me");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_button_with_icon() {
        init_gtk_for_tests();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let action = Action {
            id: "button_click".to_string(),
            params: ActionParams::None,
        };

        let widget = render_button(
            &callback,
            &None,
            &Some("go-home-symbolic".to_string()),
            &action,
            "test_button",
        );

        assert!(widget.is::<gtk::Button>());
        let button: gtk::Button = widget.downcast().unwrap();

        // Button should have an image child
        let child = button.child();
        assert!(child.is_some());
        assert!(child.unwrap().is::<gtk::Image>());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_button_callback() {
        init_gtk_for_tests();

        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let action = Action {
            id: "button_click".to_string(),
            params: ActionParams::None,
        };

        let widget = render_button(
            &callback,
            &Some("Test".to_string()),
            &None,
            &action,
            "test_button",
        );

        let button: gtk::Button = widget.downcast().unwrap();

        // Simulate a click
        button.emit_clicked();

        // Verify the callback was invoked
        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "test_button");
        assert_eq!(actions[0].1.id, "button_click");
    }
}
