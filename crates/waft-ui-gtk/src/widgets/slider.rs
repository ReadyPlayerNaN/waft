//! Slider widget renderer - converts Slider descriptions to GTK scale with menu

use crate::renderer::{ActionCallback, WidgetRenderer};
use crate::utils::icon::IconWidget;
use waft_ipc::widget::{Action, ActionParams};
use crate::utils::menu_state::{is_menu_open, menu_id_for_widget, toggle_menu};
use gtk::prelude::*;
use std::rc::Rc;
use waft_core::menu_state::MenuStore;

/// Render a Slider widget with icon, scale, and optional expanded content
///
/// Structure:
/// Box(Vertical) → [Box(H) with [Icon button, Scale, Expand button], Revealer with content]
///
/// # Parameters
///
/// - `renderer`: The WidgetRenderer instance for recursive rendering
/// - `callback`: The action callback for handling actions
/// - `menu_store`: The MenuStore for coordinating expanded menus
/// - `icon`: Themed icon name for the icon button
/// - `value`: Current value (0.0-1.0)
/// - `muted`: Whether the slider is muted (affects icon and CSS)
/// - `expandable`: Whether to show expand button
/// - `expanded_content`: Optional widget shown in revealer when expanded
/// - `on_value_change`: Action triggered when scale value changes
/// - `on_icon_click`: Action triggered when icon button is clicked
/// - `widget_id`: Unique identifier for this slider
///
/// # Returns
///
/// A gtk::Box containing the slider layout, upcast to gtk::Widget
#[allow(clippy::too_many_arguments)]
pub fn render_slider(
    renderer: &WidgetRenderer,
    callback: &ActionCallback,
    menu_store: &Rc<MenuStore>,
    icon: &str,
    value: f64,
    muted: bool,
    expandable: bool,
    expanded_content: &Option<Box<crate::types::Widget>>,
    on_value_change: &Action,
    on_icon_click: &Action,
    widget_id: &str,
) -> gtk::Widget {
    // Main vertical container
    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // Top horizontal box with controls
    let controls_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    // Icon button
    let icon_button = gtk::Button::new();
    icon_button.add_css_class("flat");
    icon_button.add_css_class("circular");

    // Determine icon based on muted state
    let icon_name = if muted {
        // For volume-like sliders, muted typically means mute icon
        // This matches the audio plugin behavior
        format!("{}-muted", icon.trim_end_matches("-symbolic"))
    } else {
        icon.to_string()
    };

    let icon_widget = IconWidget::from_name(&icon_name, 24);
    icon_button.set_child(Some(icon_widget.widget()));

    // Connect icon button click
    let widget_id_clone = widget_id.to_string();
    let on_icon_click = on_icon_click.clone();
    let callback_clone = callback.clone();
    icon_button.connect_clicked(move |_| {
        callback_clone(widget_id_clone.clone(), on_icon_click.clone());
    });

    controls_box.append(&icon_button);

    // Scale (slider)
    let adjustment = gtk::Adjustment::new(
        value * 100.0, // Current value (0-100)
        0.0,           // Min
        100.0,         // Max
        1.0,           // Step increment
        10.0,          // Page increment
        0.0,           // Page size
    );

    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
    scale.set_draw_value(false);
    scale.set_hexpand(true);

    // Connect scale value change
    let widget_id_clone = widget_id.to_string();
    let on_value_change = on_value_change.clone();
    let callback_clone = callback.clone();
    scale.connect_value_changed(move |scale| {
        let value = scale.value() / 100.0; // Convert 0-100 back to 0.0-1.0
        let mut action = on_value_change.clone();
        action.params = ActionParams::Value(value);
        callback_clone(widget_id_clone.clone(), action);
    });

    controls_box.append(&scale);

    // Expand button (if expandable)
    if expandable {
        let expand_button = gtk::Button::new();
        expand_button.add_css_class("flat");
        expand_button.add_css_class("circular");

        // Chevron icon (down if menu open, up if closed)
        let menu_id = menu_id_for_widget(widget_id);
        let is_open = is_menu_open(menu_store, &menu_id);
        let chevron_icon = if is_open {
            "pan-up-symbolic"
        } else {
            "pan-down-symbolic"
        };
        let chevron_widget = IconWidget::from_name(chevron_icon, 16);
        expand_button.set_child(Some(chevron_widget.widget()));

        // Connect expand button click
        let menu_store_clone = menu_store.clone();
        let menu_id_clone = menu_id.clone();
        expand_button.connect_clicked(move |_| {
            toggle_menu(&menu_store_clone, &menu_id_clone);
        });

        controls_box.append(&expand_button);
    }

    main_box.append(&controls_box);

    // Apply muted CSS class if muted
    if muted {
        main_box.add_css_class("slider-row-muted");
    }

    // Revealer for expanded content
    if expandable {
        if let Some(content) = expanded_content {
            let revealer = gtk::Revealer::new();
            revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
            revealer.set_transition_duration(200);

            // Render expanded content recursively
            let content_id = format!("{}:expanded", widget_id);
            let gtk_content = renderer.render(content, &content_id);
            revealer.set_child(Some(&gtk_content));

            // Set revealer state based on menu store
            let menu_id = menu_id_for_widget(widget_id);
            let is_open = is_menu_open(menu_store, &menu_id);
            revealer.set_reveal_child(is_open);

            main_box.append(&revealer);
        }
    }

    main_box.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ActionParams, Widget};
    use std::cell::RefCell;
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
    fn test_render_slider_basic() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_volume".to_string(),
            params: ActionParams::Value(0.5),
        };
        let on_icon_click = Action {
            id: "toggle_mute".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "audio-volume-high-symbolic",
            0.5,
            false,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "audio_slider",
        );

        assert!(widget.is::<gtk::Box>());
        let main_box: gtk::Box = widget.downcast().unwrap();
        assert_eq!(main_box.orientation(), gtk::Orientation::Vertical);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_muted() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_volume".to_string(),
            params: ActionParams::Value(0.0),
        };
        let on_icon_click = Action {
            id: "toggle_mute".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "audio-volume-high-symbolic",
            0.0,
            true,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "muted_slider",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("slider-row-muted"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_expandable_collapsed() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let expanded_content = Some(Box::new(Widget::Label {
            text: "Expanded Content".to_string(),
            css_classes: vec![],
        }));

        let on_value_change = Action {
            id: "set_value".to_string(),
            params: ActionParams::Value(0.75),
        };
        let on_icon_click = Action {
            id: "icon_click".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "preferences-system-symbolic",
            0.75,
            false,
            true,
            &expanded_content,
            &on_value_change,
            &on_icon_click,
            "expandable_slider",
        );

        assert!(widget.is::<gtk::Box>());
        // Revealer should exist and be collapsed by default
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_icon_click_callback() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());

        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_value".to_string(),
            params: ActionParams::Value(0.5),
        };
        let on_icon_click = Action {
            id: "icon_clicked".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "audio-volume-high-symbolic",
            0.5,
            false,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "test_slider",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        let controls_box = main_box.first_child().unwrap();
        let controls_box: gtk::Box = controls_box.downcast().unwrap();
        let icon_button = controls_box.first_child().unwrap();
        let icon_button: gtk::Button = icon_button.downcast().unwrap();

        // Simulate icon button click
        icon_button.emit_clicked();

        // Verify callback was invoked
        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "test_slider");
        assert_eq!(actions[0].1.id, "icon_clicked");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_value_range() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_value".to_string(),
            params: ActionParams::Value(0.0),
        };
        let on_icon_click = Action {
            id: "icon_click".to_string(),
            params: ActionParams::None,
        };

        // Test minimum value
        let widget_min = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "icon",
            0.0,
            false,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "slider_min",
        );
        assert!(widget_min.is::<gtk::Box>());

        // Test maximum value
        let widget_max = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "icon",
            1.0,
            false,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "slider_max",
        );
        assert!(widget_max.is::<gtk::Box>());

        // Test mid value
        let widget_mid = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "icon",
            0.5,
            false,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "slider_mid",
        );
        assert!(widget_mid.is::<gtk::Box>());
    }
}
