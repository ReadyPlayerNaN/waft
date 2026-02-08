//! Comprehensive integration tests for waft-ui-gtk renderer
//!
//! These tests verify end-to-end widget rendering scenarios, complex widget trees,
//! action callbacks, and menu coordination across the full system.
//!
//! Run GTK tests with: cargo test -p waft-ui-gtk --test integration_tests -- --ignored --test-threads=1

use std::cell::RefCell;
use std::rc::Rc;
use waft_core::menu_state::create_menu_store;
use waft_ui_gtk::renderer::{ActionCallback, WidgetRenderer};
use waft_ui_gtk::types::{Action, ActionParams, Orientation, Widget};

#[cfg(test)]
use gtk::prelude::*;

// Helper to ensure GTK is initialized for widget tests
fn init_gtk() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        gtk::init().expect("Failed to initialize GTK");
    });
}

/// Helper to create a test renderer with action capturing
fn create_test_renderer() -> (WidgetRenderer, Rc<RefCell<Vec<(String, Action)>>>) {
    let menu_store = Rc::new(create_menu_store());
    let captured_actions: Rc<RefCell<Vec<(String, Action)>>> = Rc::new(RefCell::new(Vec::new()));
    let captured_actions_clone = captured_actions.clone();

    let callback: ActionCallback = Rc::new(move |widget_id, action| {
        captured_actions_clone
            .borrow_mut()
            .push((widget_id, action));
    });

    let renderer = WidgetRenderer::new(menu_store, callback);
    (renderer, captured_actions)
}

// ============================================================================
// Complex Widget Tree Tests
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_nested_containers() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 12,
        css_classes: vec!["outer".to_string()],
        children: vec![
            Widget::Label {
                text: "Header".to_string(),
                css_classes: vec!["header".to_string()],
            },
            Widget::Container {
                orientation: Orientation::Horizontal,
                spacing: 6,
                css_classes: vec!["inner".to_string()],
                children: vec![
                    Widget::Label {
                        text: "Left".to_string(),
                        css_classes: vec![],
                    },
                    Widget::Label {
                        text: "Right".to_string(),
                        css_classes: vec![],
                    },
                ],
            },
            Widget::Label {
                text: "Footer".to_string(),
                css_classes: vec!["footer".to_string()],
            },
        ],
    };

    let gtk_widget = renderer.render(&widget, "nested_container");
    assert!(gtk_widget.is::<gtk::Box>());

    let container: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(container.has_css_class("outer"));

    // Verify children are rendered
    let first_child = container.first_child().expect("Should have first child");
    assert!(first_child.is::<gtk::Label>());
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_feature_toggle_with_expanded_content() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::FeatureToggle {
        title: "Bluetooth".to_string(),
        icon: "bluetooth-active".to_string(),
        details: Some("Connected".to_string()),
        active: true,
        busy: false,
        expandable: true,
        expanded_content: Some(Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 4,
            css_classes: vec!["devices-list".to_string()],
            children: vec![
                Widget::MenuRow {
                    icon: Some("device-headphones".to_string()),
                    label: "Headphones".to_string(),
                    sublabel: Some("Connected".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: Some(Action {
                        id: "select_device".to_string(),
                        params: ActionParams::String("headphones".to_string()),
                    }),
                },
                Widget::MenuRow {
                    icon: Some("device-speaker".to_string()),
                    label: "Speaker".to_string(),
                    sublabel: None,
                    trailing: None,
                    sensitive: true,
                    on_click: None,
                },
            ],
        })),
        on_toggle: Action {
            id: "toggle_bluetooth".to_string(),
            params: ActionParams::None,
        },
    };

    let gtk_widget = renderer.render(&widget, "bluetooth_toggle");
    assert!(gtk_widget.is::<gtk::Box>());

    let container: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(container.has_css_class("feature-toggle"));
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_slider_with_expanded_content() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::Slider {
        icon: "volume-high".to_string(),
        value: 0.75,
        muted: false,
        expandable: true,
        expanded_content: Some(Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 4,
            css_classes: vec![],
            children: vec![
                Widget::MenuRow {
                    icon: Some("audio-card".to_string()),
                    label: "Output Device".to_string(),
                    sublabel: Some("Speakers".to_string()),
                    trailing: None,
                    sensitive: true,
                    on_click: None,
                },
            ],
        })),
        on_value_change: Action {
            id: "set_volume".to_string(),
            params: ActionParams::Value(0.75),
        },
        on_icon_click: Action {
            id: "toggle_mute".to_string(),
            params: ActionParams::None,
        },
    };

    let gtk_widget = renderer.render(&widget, "volume_slider");
    assert!(gtk_widget.is::<gtk::Box>());

    let container: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(container.has_css_class("control-slider"));
}

// ============================================================================
// Menu Row Variations Tests
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_menu_row_with_all_elements() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::MenuRow {
        icon: Some("network-wifi".to_string()),
        label: "Wi-Fi".to_string(),
        sublabel: Some("Connected to HomeNet".to_string()),
        trailing: Some(Box::new(Widget::Switch {
            active: true,
            sensitive: true,
            on_toggle: Action {
                id: "toggle_wifi".to_string(),
                params: ActionParams::None,
            },
        })),
        sensitive: true,
        on_click: Some(Action {
            id: "open_wifi_settings".to_string(),
            params: ActionParams::None,
        }),
    };

    let gtk_widget = renderer.render(&widget, "wifi_row");
    assert!(gtk_widget.is::<gtk::Box>());

    let row: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(row.has_css_class("menu-row"));
    assert!(row.is_sensitive());
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_menu_row_minimal() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::MenuRow {
        icon: None,
        label: "Simple Item".to_string(),
        sublabel: None,
        trailing: None,
        sensitive: true,
        on_click: None,
    };

    let gtk_widget = renderer.render(&widget, "simple_row");
    assert!(gtk_widget.is::<gtk::Box>());

    let row: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(row.has_css_class("menu-row"));
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_menu_row_with_spinner() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::MenuRow {
        icon: Some("network-cellular".to_string()),
        label: "Connecting...".to_string(),
        sublabel: None,
        trailing: Some(Box::new(Widget::Spinner { spinning: true })),
        sensitive: false,
        on_click: None,
    };

    let gtk_widget = renderer.render(&widget, "connecting_row");
    assert!(gtk_widget.is::<gtk::Box>());

    let row: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(!row.is_sensitive());
}

// ============================================================================
// Primitive Widget Tests
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_all_primitives() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    // Switch
    let switch = Widget::Switch {
        active: true,
        sensitive: true,
        on_toggle: Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        },
    };
    let gtk_switch = renderer.render(&switch, "test_switch");
    assert!(gtk_switch.is::<gtk::Switch>());
    let sw: gtk::Switch = gtk_switch.downcast().unwrap();
    assert!(sw.is_active());
    assert!(sw.is_sensitive());

    // Spinner
    let spinner = Widget::Spinner { spinning: true };
    let gtk_spinner = renderer.render(&spinner, "test_spinner");
    assert!(gtk_spinner.is::<gtk::Spinner>());
    let sp: gtk::Spinner = gtk_spinner.downcast().unwrap();
    assert!(sp.is_spinning());

    // Checkmark
    let checkmark = Widget::Checkmark { visible: true };
    let gtk_checkmark = renderer.render(&checkmark, "test_checkmark");
    assert!(gtk_checkmark.is::<gtk::Image>());
    let cm: gtk::Image = gtk_checkmark.downcast().unwrap();
    assert!(cm.is_visible());

    // Button with label
    let button = Widget::Button {
        label: Some("Click Me".to_string()),
        icon: None,
        on_click: Action {
            id: "button_click".to_string(),
            params: ActionParams::None,
        },
    };
    let gtk_button = renderer.render(&button, "test_button");
    assert!(gtk_button.is::<gtk::Button>());

    // Button with icon
    let icon_button = Widget::Button {
        label: None,
        icon: Some("edit".to_string()),
        on_click: Action {
            id: "edit_click".to_string(),
            params: ActionParams::None,
        },
    };
    let gtk_icon_button = renderer.render(&icon_button, "test_icon_button");
    assert!(gtk_icon_button.is::<gtk::Button>());
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_empty_container() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 0,
        css_classes: vec![],
        children: vec![],
    };

    let gtk_widget = renderer.render(&widget, "empty_container");
    assert!(gtk_widget.is::<gtk::Box>());

    let container: gtk::Box = gtk_widget.downcast().unwrap();
    assert_eq!(container.first_child(), None);
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_feature_toggle_without_expanded_content() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::FeatureToggle {
        title: "Simple Toggle".to_string(),
        icon: "power".to_string(),
        details: None,
        active: false,
        busy: false,
        expandable: false,
        expanded_content: None,
        on_toggle: Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        },
    };

    let gtk_widget = renderer.render(&widget, "simple_toggle");
    assert!(gtk_widget.is::<gtk::Box>());
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_render_slider_without_expanded_content() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::Slider {
        icon: "brightness".to_string(),
        value: 0.5,
        muted: false,
        expandable: false,
        expanded_content: None,
        on_value_change: Action {
            id: "set_brightness".to_string(),
            params: ActionParams::Value(0.5),
        },
        on_icon_click: Action {
            id: "auto_brightness".to_string(),
            params: ActionParams::None,
        },
    };

    let gtk_widget = renderer.render(&widget, "brightness_slider");
    assert!(gtk_widget.is::<gtk::Box>());
}

// ============================================================================
// Stateless Rendering Tests
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_multiple_renders_same_widget() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::Label {
        text: "Repeated".to_string(),
        css_classes: vec![],
    };

    // Render the same widget multiple times with different IDs
    let gtk1 = renderer.render(&widget, "label_1");
    let gtk2 = renderer.render(&widget, "label_2");
    let gtk3 = renderer.render(&widget, "label_3");

    // All should be valid labels with the same text
    let label1: gtk::Label = gtk1.downcast().unwrap();
    let label2: gtk::Label = gtk2.downcast().unwrap();
    let label3: gtk::Label = gtk3.downcast().unwrap();

    assert_eq!(label1.text(), "Repeated");
    assert_eq!(label2.text(), "Repeated");
    assert_eq!(label3.text(), "Repeated");
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_renderer_stateless_updates() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    // First render with active = false
    let widget1 = Widget::Switch {
        active: false,
        sensitive: true,
        on_toggle: Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        },
    };
    let gtk1 = renderer.render(&widget1, "switch_test");
    let sw1: gtk::Switch = gtk1.downcast().unwrap();
    assert!(!sw1.is_active());

    // Second render with active = true (simulating state update)
    let widget2 = Widget::Switch {
        active: true,
        sensitive: true,
        on_toggle: Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        },
    };
    let gtk2 = renderer.render(&widget2, "switch_test");
    let sw2: gtk::Switch = gtk2.downcast().unwrap();
    assert!(sw2.is_active());

    // Original widget unchanged (renderer is stateless)
    assert!(!sw1.is_active());
}

// ============================================================================
// CSS Classes Tests
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_css_classes_applied_correctly() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 8,
        css_classes: vec![
            "menu-section".to_string(),
            "primary".to_string(),
            "highlight".to_string(),
        ],
        children: vec![],
    };

    let gtk_widget = renderer.render(&widget, "styled_container");
    let container: gtk::Box = gtk_widget.downcast().unwrap();

    assert!(container.has_css_class("menu-section"));
    assert!(container.has_css_class("primary"));
    assert!(container.has_css_class("highlight"));
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_label_with_multiple_css_classes() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    let widget = Widget::Label {
        text: "Styled Text".to_string(),
        css_classes: vec![
            "title".to_string(),
            "bold".to_string(),
            "accent-color".to_string(),
        ],
    };

    let gtk_widget = renderer.render(&widget, "styled_label");
    let label: gtk::Label = gtk_widget.downcast().unwrap();

    assert_eq!(label.text(), "Styled Text");
    assert!(label.has_css_class("title"));
    assert!(label.has_css_class("bold"));
    assert!(label.has_css_class("accent-color"));
}

// ============================================================================
// Action Callback Tests (Non-GTK)
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_action_callback_via_switch_widget() {
    init_gtk();
    let (renderer, actions) = create_test_renderer();

    // Render a switch widget (which will connect action callback)
    let widget = Widget::Switch {
        active: false,
        sensitive: true,
        on_toggle: Action {
            id: "test_toggle".to_string(),
            params: ActionParams::None,
        },
    };

    let gtk_widget = renderer.render(&widget, "test_switch");
    let switch: gtk::Switch = gtk_widget.downcast().unwrap();

    // Activate the switch programmatically (simulates user click)
    switch.set_active(true);

    // Give GTK a moment to process signals
    while gtk::glib::MainContext::default().iteration(false) {}

    let captured = actions.borrow();
    // Action should be captured when switch is toggled
    assert!(!captured.is_empty());
    assert_eq!(captured[0].0, "test_switch");
    assert_eq!(captured[0].1.id, "test_toggle");
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_multiple_action_callbacks_via_widgets() {
    init_gtk();
    let (renderer, actions) = create_test_renderer();

    // Render multiple interactive widgets
    let switch_widget = Widget::Switch {
        active: false,
        sensitive: true,
        on_toggle: Action {
            id: "toggle_action".to_string(),
            params: ActionParams::None,
        },
    };

    let button_widget = Widget::Button {
        label: Some("Click".to_string()),
        icon: None,
        on_click: Action {
            id: "click_action".to_string(),
            params: ActionParams::None,
        },
    };

    let _gtk_switch = renderer.render(&switch_widget, "switch_1");
    let gtk_button = renderer.render(&button_widget, "button_1");

    // Trigger button click
    let button: gtk::Button = gtk_button.downcast().unwrap();
    button.emit_clicked();

    // Give GTK a moment to process signals
    while gtk::glib::MainContext::default().iteration(false) {}

    let captured = actions.borrow();
    // Should capture button click action
    assert!(!captured.is_empty());
    assert!(captured.iter().any(|(id, _)| id == "button_1"));
}

// ============================================================================
// Complex Real-World Scenarios
// ============================================================================

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_real_world_audio_control_widget() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    // Realistic audio control widget tree
    let widget = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 8,
        css_classes: vec!["audio-section".to_string()],
        children: vec![
            // Volume slider with device list
            Widget::Slider {
                icon: "volume-high".to_string(),
                value: 0.65,
                muted: false,
                expandable: true,
                expanded_content: Some(Box::new(Widget::Container {
                    orientation: Orientation::Vertical,
                    spacing: 0,
                    css_classes: vec!["device-list".to_string()],
                    children: vec![
                        Widget::MenuRow {
                            icon: Some("audio-headphones".to_string()),
                            label: "Headphones".to_string(),
                            sublabel: Some("USB Audio".to_string()),
                            trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                            sensitive: true,
                            on_click: Some(Action {
                                id: "select_output".to_string(),
                                params: ActionParams::String("headphones".to_string()),
                            }),
                        },
                        Widget::MenuRow {
                            icon: Some("audio-speakers".to_string()),
                            label: "Speakers".to_string(),
                            sublabel: Some("Built-in Audio".to_string()),
                            trailing: Some(Box::new(Widget::Checkmark { visible: false })),
                            sensitive: true,
                            on_click: Some(Action {
                                id: "select_output".to_string(),
                                params: ActionParams::String("speakers".to_string()),
                            }),
                        },
                    ],
                })),
                on_value_change: Action {
                    id: "set_volume".to_string(),
                    params: ActionParams::Value(0.65),
                },
                on_icon_click: Action {
                    id: "toggle_mute".to_string(),
                    params: ActionParams::None,
                },
            },
            // Microphone slider
            Widget::Slider {
                icon: "microphone-sensitivity-high".to_string(),
                value: 0.8,
                muted: false,
                expandable: false,
                expanded_content: None,
                on_value_change: Action {
                    id: "set_mic_volume".to_string(),
                    params: ActionParams::Value(0.8),
                },
                on_icon_click: Action {
                    id: "toggle_mic_mute".to_string(),
                    params: ActionParams::None,
                },
            },
        ],
    };

    let gtk_widget = renderer.render(&widget, "audio_controls");
    assert!(gtk_widget.is::<gtk::Box>());

    let container: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(container.has_css_class("audio-section"));
}

#[test]
#[ignore = "Requires GTK main thread - run with --test-threads=1"]
fn test_real_world_network_settings_widget() {
    init_gtk();
    let (renderer, _actions) = create_test_renderer();

    // Realistic network settings widget tree
    let widget = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 12,
        css_classes: vec!["network-section".to_string()],
        children: vec![
            // Wi-Fi feature toggle
            Widget::FeatureToggle {
                title: "Wi-Fi".to_string(),
                icon: "network-wireless".to_string(),
                details: Some("HomeNetwork".to_string()),
                active: true,
                busy: false,
                expandable: true,
                expanded_content: Some(Box::new(Widget::Container {
                    orientation: Orientation::Vertical,
                    spacing: 0,
                    css_classes: vec!["wifi-networks".to_string()],
                    children: vec![
                        Widget::MenuRow {
                            icon: Some("network-wireless-signal-excellent".to_string()),
                            label: "HomeNetwork".to_string(),
                            sublabel: Some("Connected".to_string()),
                            trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                            sensitive: true,
                            on_click: None,
                        },
                        Widget::MenuRow {
                            icon: Some("network-wireless-signal-good".to_string()),
                            label: "OfficeWiFi".to_string(),
                            sublabel: Some("Saved".to_string()),
                            trailing: None,
                            sensitive: true,
                            on_click: Some(Action {
                                id: "connect_network".to_string(),
                                params: ActionParams::String("OfficeWiFi".to_string()),
                            }),
                        },
                        Widget::MenuRow {
                            icon: Some("network-wireless-signal-weak".to_string()),
                            label: "Public WiFi".to_string(),
                            sublabel: None,
                            trailing: Some(Box::new(Widget::Spinner { spinning: false })),
                            sensitive: true,
                            on_click: Some(Action {
                                id: "connect_network".to_string(),
                                params: ActionParams::String("Public WiFi".to_string()),
                            }),
                        },
                    ],
                })),
                on_toggle: Action {
                    id: "toggle_wifi".to_string(),
                    params: ActionParams::None,
                },
            },
            // Bluetooth feature toggle
            Widget::FeatureToggle {
                title: "Bluetooth".to_string(),
                icon: "bluetooth-active".to_string(),
                details: Some("2 devices".to_string()),
                active: true,
                busy: false,
                expandable: false,
                expanded_content: None,
                on_toggle: Action {
                    id: "toggle_bluetooth".to_string(),
                    params: ActionParams::None,
                },
            },
        ],
    };

    let gtk_widget = renderer.render(&widget, "network_settings");
    assert!(gtk_widget.is::<gtk::Box>());

    let container: gtk::Box = gtk_widget.downcast().unwrap();
    assert!(container.has_css_class("network-section"));
}
