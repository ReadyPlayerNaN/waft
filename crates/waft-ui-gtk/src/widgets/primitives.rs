//! Primitive widget renderers - simple, direct GTK mappings

use crate::renderer::ActionCallback;
use crate::utils::css::apply_css_classes;
use waft_ipc::widget::Action;
use gtk::prelude::*;

/// Render a Switch widget
///
/// Maps to gtk::Switch with active state and sensitivity.
/// Connects state_set signal to trigger on_toggle action.
pub fn render_switch(
    callback: &ActionCallback,
    active: bool,
    sensitive: bool,
    on_toggle: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let switch = gtk::Switch::new();
    switch.set_active(active);
    switch.set_sensitive(sensitive);

    // Clone necessary data for the closure
    let widget_id = widget_id.to_string();
    let on_toggle = on_toggle.clone();
    let callback = callback.clone();

    switch.connect_state_set(move |_switch, state| {
        // Trigger the action with the new state
        let mut action = on_toggle.clone();
        action.params = crate::types::ActionParams::Value(if state { 1.0 } else { 0.0 });
        callback(widget_id.clone(), action);
        gtk::glib::Propagation::Proceed
    });

    switch.upcast()
}

/// Render a Spinner widget
///
/// Maps to gtk::Spinner. Calls start() if spinning is true.
pub fn render_spinner(spinning: bool) -> gtk::Widget {
    let spinner = gtk::Spinner::new();
    if spinning {
        spinner.start();
    }
    spinner.upcast()
}

/// Render a Checkmark widget
///
/// Maps to gtk::Image with "object-select-symbolic" icon.
/// Applies "checkmark" CSS class and sets visibility.
pub fn render_checkmark(visible: bool) -> gtk::Widget {
    let image = gtk::Image::from_icon_name("object-select-symbolic");
    image.add_css_class("checkmark");
    image.set_visible(visible);
    image.upcast()
}

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

/// Render a Label widget
///
/// Maps to gtk::Label with text and CSS classes.
pub fn render_label(text: &str, css_classes: &[String]) -> gtk::Widget {
    let label = gtk::Label::new(Some(text));
    apply_css_classes(&label, css_classes);
    label.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActionParams;
    use std::cell::RefCell;
    use std::rc::Rc;

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
    fn test_render_switch_basic() {
        init_gtk();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let action = Action {
            id: "toggle_test".to_string(),
            params: ActionParams::None,
        };

        let widget = render_switch(&callback, true, true, &action, "test_switch");

        assert!(widget.is::<gtk::Switch>());
        let switch: gtk::Switch = widget.downcast().unwrap();
        assert!(switch.is_active());
        assert!(switch.is_sensitive());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_switch_inactive_insensitive() {
        init_gtk();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let action = Action {
            id: "toggle_test".to_string(),
            params: ActionParams::None,
        };

        let widget = render_switch(&callback, false, false, &action, "test_switch");

        let switch: gtk::Switch = widget.downcast().unwrap();
        assert!(!switch.is_active());
        assert!(!switch.is_sensitive());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_spinner_spinning() {
        init_gtk();
        let widget = render_spinner(true);

        assert!(widget.is::<gtk::Spinner>());
        let spinner: gtk::Spinner = widget.downcast().unwrap();
        assert!(spinner.is_spinning());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_spinner_not_spinning() {
        init_gtk();
        let widget = render_spinner(false);

        let spinner: gtk::Spinner = widget.downcast().unwrap();
        assert!(!spinner.is_spinning());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_checkmark_visible() {
        init_gtk();
        let widget = render_checkmark(true);

        assert!(widget.is::<gtk::Image>());
        let image: gtk::Image = widget.downcast().unwrap();
        assert!(image.is_visible());
        assert!(image.has_css_class("checkmark"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_checkmark_hidden() {
        init_gtk();
        let widget = render_checkmark(false);

        let image: gtk::Image = widget.downcast().unwrap();
        assert!(!image.is_visible());
        assert!(image.has_css_class("checkmark"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_button_with_label() {
        init_gtk();
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
        init_gtk();
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
        init_gtk();

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

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_basic() {
        init_gtk();
        let widget = render_label("Hello World", &[]);

        assert!(widget.is::<gtk::Label>());
        let label: gtk::Label = widget.downcast().unwrap();
        assert_eq!(label.text(), "Hello World");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_with_css_classes() {
        init_gtk();
        let classes = vec!["bold".to_string(), "accent".to_string()];
        let widget = render_label("Styled Label", &classes);

        let label: gtk::Label = widget.downcast().unwrap();
        assert_eq!(label.text(), "Styled Label");
        assert!(label.has_css_class("bold"));
        assert!(label.has_css_class("accent"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_empty_text() {
        init_gtk();
        let widget = render_label("", &[]);

        let label: gtk::Label = widget.downcast().unwrap();
        assert_eq!(label.text(), "");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_primitives_upcast_correctly() {
        init_gtk();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let action = Action {
            id: "test".to_string(),
            params: ActionParams::None,
        };

        // All primitives should return gtk::Widget
        let _switch: gtk::Widget = render_switch(&callback, true, true, &action, "id");
        let _spinner: gtk::Widget = render_spinner(true);
        let _checkmark: gtk::Widget = render_checkmark(true);
        let _button: gtk::Widget = render_button(&callback, &None, &None, &action, "id");
        let _label: gtk::Widget = render_label("test", &[]);

        // If we got here without type errors, upcasting works correctly
    }
}
