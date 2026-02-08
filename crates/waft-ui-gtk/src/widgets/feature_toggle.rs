//! FeatureToggle widget renderer - the most complex widget with 7 states
//!
//! States: inactive, active, busy, expandable, expanded, with_details, active_expandable

use crate::renderer::{ActionCallback, WidgetRenderer};
use crate::utils::icon::IconWidget;
use waft_ipc::widget::Action;
use crate::utils::menu_state::{is_menu_open, menu_id_for_widget, toggle_menu};
use gtk::prelude::*;
use std::rc::Rc;
use waft_core::menu_state::MenuStore;

/// Render a FeatureToggle widget with icon, title, details, and optional expanded content
///
/// Structure:
/// Box(H) → [Button with [Icon, Labels box with details revealer], Revealer with expand button]
///
/// # Parameters
///
/// - `renderer`: The WidgetRenderer instance for recursive rendering
/// - `callback`: The action callback for handling actions
/// - `menu_store`: The MenuStore for coordinating expanded menus
/// - `title`: Main title text
/// - `icon`: Themed icon name
/// - `details`: Optional details text (shown in revealer)
/// - `active`: Whether the feature is active (CSS class)
/// - `busy`: Whether the feature is busy (CSS class + spinner)
/// - `expandable`: Whether to show expand button
/// - `expanded_content`: Optional widget shown in expanded menu
/// - `on_toggle`: Action triggered when main button is clicked
/// - `widget_id`: Unique identifier for this feature toggle
///
/// # Returns
///
/// A gtk::Box containing the feature toggle layout, upcast to gtk::Widget
#[allow(clippy::too_many_arguments)]
pub fn render_feature_toggle(
    _renderer: &WidgetRenderer,
    callback: &ActionCallback,
    menu_store: &Rc<MenuStore>,
    title: &str,
    icon: &str,
    details: &Option<String>,
    active: bool,
    busy: bool,
    expandable: bool,
    _expanded_content: &Option<Box<crate::types::Widget>>,
    on_toggle: &Action,
    widget_id: &str,
) -> gtk::Widget {
    // Main horizontal container
    let main_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main_box.add_css_class("feature-toggle");

    // Apply state-based CSS classes
    if active {
        main_box.add_css_class("active");
    }
    if busy {
        main_box.add_css_class("busy");
    }
    if expandable {
        main_box.add_css_class("expandable");
    }

    // Check if menu is expanded
    let menu_id = menu_id_for_widget(widget_id);
    let is_expanded = is_menu_open(menu_store, &menu_id);
    if is_expanded {
        main_box.add_css_class("expanded");
    }

    // Main button (left side)
    let main_button = gtk::Button::new();
    main_button.set_hexpand(true);

    // Button content container
    let button_content = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    // Icon
    let icon_widget = IconWidget::from_name(icon, 32);
    button_content.append(icon_widget.widget());

    // Labels container (vertical)
    let labels_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    labels_box.set_halign(gtk::Align::Start);
    labels_box.set_hexpand(true);

    // Title label
    let title_label = gtk::Label::new(Some(title));
    title_label.set_halign(gtk::Align::Start);
    labels_box.append(&title_label);

    // Details revealer (if details provided)
    if let Some(details_text) = details {
        let details_revealer = gtk::Revealer::new();
        details_revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        details_revealer.set_transition_duration(150);
        details_revealer.set_reveal_child(true); // Always show details if provided

        let details_label = gtk::Label::new(Some(details_text));
        details_label.set_halign(gtk::Align::Start);
        details_label.add_css_class("dim-label");
        details_revealer.set_child(Some(&details_label));

        labels_box.append(&details_revealer);
    }

    button_content.append(&labels_box);

    // Busy spinner (if busy)
    if busy {
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        button_content.append(&spinner);
    }

    main_button.set_child(Some(&button_content));

    // Connect main button click
    let widget_id_clone = widget_id.to_string();
    let on_toggle = on_toggle.clone();
    let callback_clone = callback.clone();
    main_button.connect_clicked(move |_| {
        callback_clone(widget_id_clone.clone(), on_toggle.clone());
    });

    main_box.append(&main_button);

    // Expand button in revealer (if expandable)
    if expandable {
        let expand_revealer = gtk::Revealer::new();
        expand_revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        expand_revealer.set_transition_duration(150);
        expand_revealer.set_reveal_child(true); // Always show if expandable

        let expand_button = gtk::Button::new();
        expand_button.add_css_class("flat");
        expand_button.add_css_class("circular");

        // Chevron icon (up if expanded, down if collapsed)
        let chevron_icon = if is_expanded {
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

        expand_revealer.set_child(Some(&expand_button));
        main_box.append(&expand_revealer);
    }

    // TODO: Handle expanded_content below the main box in a separate revealer
    // This would require wrapping main_box in another vertical box
    // For now, the expanded content would be handled by a parent Container
    // or by the overview layer that renders FeatureToggles in a vertical layout

    main_box.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActionParams;
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
    fn test_render_feature_toggle_inactive() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_bluetooth".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Bluetooth",
            "bluetooth-symbolic",
            &None,
            false,
            false,
            false,
            &None,
            &on_toggle,
            "bluetooth",
        );

        assert!(widget.is::<gtk::Box>());
        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("feature-toggle"));
        assert!(!main_box.has_css_class("active"));
        assert!(!main_box.has_css_class("busy"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_active() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_wifi".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Wi-Fi",
            "network-wireless-symbolic",
            &None,
            true,
            false,
            false,
            &None,
            &on_toggle,
            "wifi",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_busy() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_feature".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Loading",
            "emblem-synchronizing-symbolic",
            &None,
            false,
            true,
            false,
            &None,
            &on_toggle,
            "loading_feature",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("busy"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_with_details() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_bt".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Bluetooth",
            "bluetooth-active-symbolic",
            &Some("Connected to 2 devices".to_string()),
            true,
            false,
            false,
            &None,
            &on_toggle,
            "bt_with_details",
        );

        assert!(widget.is::<gtk::Box>());
        // Just verify it renders without panicking
        // Details revealer verification would require deep widget tree traversal
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_expandable() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Settings",
            "preferences-system-symbolic",
            &None,
            false,
            false,
            true,
            &None,
            &on_toggle,
            "expandable_feature",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("expandable"));
        assert!(!main_box.has_css_class("expanded")); // Not expanded by default
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_active_expandable() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Bluetooth",
            "bluetooth-active-symbolic",
            &Some("Connected".to_string()),
            true,
            false,
            true,
            &None,
            &on_toggle,
            "active_expandable",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("feature-toggle"));
        assert!(main_box.has_css_class("active"));
        assert!(main_box.has_css_class("expandable"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_callback() {
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

        let on_toggle = Action {
            id: "toggle_feature".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Test Feature",
            "dialog-information-symbolic",
            &None,
            false,
            false,
            false,
            &None,
            &on_toggle,
            "test_feature",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        let main_button = main_box.first_child().unwrap();
        let main_button: gtk::Button = main_button.downcast().unwrap();

        // Simulate button click
        main_button.emit_clicked();

        // Verify callback was invoked
        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "test_feature");
        assert_eq!(actions[0].1.id, "toggle_feature");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_all_states() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        // Test all state combinations
        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Full Featured",
            "starred-symbolic",
            &Some("All features enabled".to_string()),
            true,
            true,
            true,
            &None,
            &on_toggle,
            "full_featured",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("feature-toggle"));
        assert!(main_box.has_css_class("active"));
        assert!(main_box.has_css_class("busy"));
        assert!(main_box.has_css_class("expandable"));
    }
}
