//! Visual test application for waft-ui-gtk widgets
//!
//! This example demonstrates all widget types in waft-ui-gtk, showing how they
//! render and interact. Useful for visual verification and testing during development.
//!
//! Run with: cargo run -p waft-ui-gtk --example widget_test_app

use adw::prelude::*;
use std::rc::Rc;
use waft_core::menu_state::create_menu_store;
use waft_ui_gtk::renderer::{ActionCallback, WidgetRenderer};
use waft_ipc::widget::{Action, ActionParams, Orientation, Widget};

fn main() {
    // Initialize GTK and libadwaita
    gtk::init().expect("Failed to initialize GTK");
    adw::init().expect("Failed to initialize libadwaita");

    // Create action callback that prints to console
    let action_callback: ActionCallback = Rc::new(|widget_id, action| {
        println!("🎯 Action triggered!");
        println!("   Widget ID: {}", widget_id);
        println!("   Action ID: {}", action.id);
        match &action.params {
            ActionParams::None => println!("   Params: None"),
            ActionParams::Value(v) => println!("   Params: Value({})", v),
            ActionParams::String(s) => println!("   Params: String(\"{}\")", s),
            ActionParams::Map(m) => println!("   Params: Map({:?})", m),
        }
        println!();
    });

    // Create menu store and renderer
    let menu_store = Rc::new(create_menu_store());
    let renderer = WidgetRenderer::new(menu_store, action_callback);

    // Create main window
    let window = adw::ApplicationWindow::builder()
        .title("Waft UI GTK Widget Test")
        .default_width(400)
        .default_height(800)
        .build();

    // Create scrolled window for content
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();

    // Create main container
    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    main_box.set_margin_top(12);
    main_box.set_margin_bottom(12);
    main_box.set_margin_start(12);
    main_box.set_margin_end(12);

    // Add header
    let header = gtk::Label::new(Some("Waft UI GTK Widget Gallery"));
    header.add_css_class("title-1");
    main_box.append(&header);

    // Add separator
    let separator1 = gtk::Separator::new(gtk::Orientation::Horizontal);
    main_box.append(&separator1);

    // ========================================================================
    // Feature Toggles Section
    // ========================================================================

    let section_label = gtk::Label::new(Some("Feature Toggles"));
    section_label.add_css_class("title-3");
    section_label.set_halign(gtk::Align::Start);
    main_box.append(&section_label);

    // Bluetooth toggle (active, with expanded content)
    let bluetooth_widget = Widget::FeatureToggle {
        title: "Bluetooth".to_string(),
        icon: "bluetooth-active".to_string(),
        details: Some("2 devices connected".to_string()),
        active: true,
        busy: false,
        expandable: true,
        expanded_content: Some(Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 0,
            css_classes: vec!["menu-container".to_string()],
            children: vec![
                Widget::MenuRow {
                    icon: Some("audio-headphones".to_string()),
                    label: "Wireless Headphones".to_string(),
                    sublabel: Some("Connected".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: Some(Action {
                        id: "select_bt_device".to_string(),
                        params: ActionParams::String("headphones".to_string()),
                    }),
                },
                Widget::MenuRow {
                    icon: Some("audio-speakers".to_string()),
                    label: "Bluetooth Speaker".to_string(),
                    sublabel: Some("Connected".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: Some(Action {
                        id: "select_bt_device".to_string(),
                        params: ActionParams::String("speaker".to_string()),
                    }),
                },
            ],
        })),
        on_toggle: Action {
            id: "toggle_bluetooth".to_string(),
            params: ActionParams::None,
        },
    };
    main_box.append(&renderer.render(&bluetooth_widget, "bluetooth"));

    // Dark mode toggle (inactive, non-expandable)
    let darkmode_widget = Widget::FeatureToggle {
        title: "Dark Mode".to_string(),
        icon: "weather-clear-night".to_string(),
        details: None,
        active: false,
        busy: false,
        expandable: false,
        expanded_content: None,
        on_toggle: Action {
            id: "toggle_darkmode".to_string(),
            params: ActionParams::None,
        },
    };
    main_box.append(&renderer.render(&darkmode_widget, "darkmode"));

    // Caffeine toggle (busy state)
    let caffeine_widget = Widget::FeatureToggle {
        title: "Caffeine".to_string(),
        icon: "caffeine-cup-full".to_string(),
        details: Some("Activating...".to_string()),
        active: true,
        busy: true,
        expandable: false,
        expanded_content: None,
        on_toggle: Action {
            id: "toggle_caffeine".to_string(),
            params: ActionParams::None,
        },
    };
    main_box.append(&renderer.render(&caffeine_widget, "caffeine"));

    // Add separator
    let separator2 = gtk::Separator::new(gtk::Orientation::Horizontal);
    main_box.append(&separator2);

    // ========================================================================
    // Sliders Section
    // ========================================================================

    let sliders_label = gtk::Label::new(Some("Sliders / Controls"));
    sliders_label.add_css_class("title-3");
    sliders_label.set_halign(gtk::Align::Start);
    main_box.append(&sliders_label);

    // Volume slider (with expanded content)
    let volume_widget = Widget::Slider {
        icon: "audio-volume-high".to_string(),
        value: 0.65,
        muted: false,
        expandable: true,
        expanded_content: Some(Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 0,
            css_classes: vec!["menu-container".to_string()],
            children: vec![
                Widget::MenuRow {
                    icon: Some("audio-card".to_string()),
                    label: "Output Device".to_string(),
                    sublabel: Some("Built-in Speakers".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: None,
                },
                Widget::MenuRow {
                    icon: Some("audio-headphones".to_string()),
                    label: "Headphones".to_string(),
                    sublabel: Some("USB Audio".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: false })),
                    sensitive: true,
                    on_click: Some(Action {
                        id: "switch_audio_output".to_string(),
                        params: ActionParams::String("headphones".to_string()),
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
    };
    main_box.append(&renderer.render(&volume_widget, "volume"));

    // Brightness slider (non-expandable)
    let brightness_widget = Widget::Slider {
        icon: "display-brightness".to_string(),
        value: 0.8,
        muted: false,
        expandable: false,
        expanded_content: None,
        on_value_change: Action {
            id: "set_brightness".to_string(),
            params: ActionParams::Value(0.8),
        },
        on_icon_click: Action {
            id: "auto_brightness".to_string(),
            params: ActionParams::None,
        },
    };
    main_box.append(&renderer.render(&brightness_widget, "brightness"));

    // Microphone slider (muted state)
    let mic_widget = Widget::Slider {
        icon: "microphone-sensitivity-muted".to_string(),
        value: 0.5,
        muted: true,
        expandable: false,
        expanded_content: None,
        on_value_change: Action {
            id: "set_mic_volume".to_string(),
            params: ActionParams::Value(0.5),
        },
        on_icon_click: Action {
            id: "toggle_mic_mute".to_string(),
            params: ActionParams::None,
        },
    };
    main_box.append(&renderer.render(&mic_widget, "microphone"));

    // Add separator
    let separator3 = gtk::Separator::new(gtk::Orientation::Horizontal);
    main_box.append(&separator3);

    // ========================================================================
    // Menu Rows Section
    // ========================================================================

    let menu_label = gtk::Label::new(Some("Menu Rows"));
    menu_label.add_css_class("title-3");
    menu_label.set_halign(gtk::Align::Start);
    main_box.append(&menu_label);

    let menu_container = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 0,
        css_classes: vec!["menu-container".to_string()],
        children: vec![
            // Full row with all elements
            Widget::MenuRow {
                icon: Some("network-wireless".to_string()),
                label: "Wi-Fi".to_string(),
                sublabel: Some("HomeNetwork".to_string()),
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
            },
            // Row with spinner (loading state)
            Widget::MenuRow {
                icon: Some("network-cellular".to_string()),
                label: "Mobile Data".to_string(),
                sublabel: Some("Connecting...".to_string()),
                trailing: Some(Box::new(Widget::Spinner { spinning: true })),
                sensitive: false,
                on_click: None,
            },
            // Row with checkmark
            Widget::MenuRow {
                icon: Some("network-vpn".to_string()),
                label: "VPN Connection".to_string(),
                sublabel: Some("Connected".to_string()),
                trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                sensitive: true,
                on_click: Some(Action {
                    id: "disconnect_vpn".to_string(),
                    params: ActionParams::None,
                }),
            },
            // Minimal row (no icon, no sublabel, no trailing)
            Widget::MenuRow {
                icon: None,
                label: "Advanced Settings".to_string(),
                sublabel: None,
                trailing: None,
                sensitive: true,
                on_click: Some(Action {
                    id: "open_advanced_settings".to_string(),
                    params: ActionParams::None,
                }),
            },
        ],
    };
    main_box.append(&renderer.render(&menu_container, "menu_rows"));

    // Add separator
    let separator4 = gtk::Separator::new(gtk::Orientation::Horizontal);
    main_box.append(&separator4);

    // ========================================================================
    // Primitive Widgets Section
    // ========================================================================

    let primitives_label = gtk::Label::new(Some("Primitive Widgets"));
    primitives_label.add_css_class("title-3");
    primitives_label.set_halign(gtk::Align::Start);
    main_box.append(&primitives_label);

    let primitives_container = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 8,
        css_classes: vec![],
        children: vec![
            // Label examples
            Widget::Label {
                text: "Simple Label".to_string(),
                css_classes: vec![],
            },
            Widget::Label {
                text: "Styled Label".to_string(),
                css_classes: vec!["title-4".to_string(), "accent".to_string()],
            },
            // Button examples in horizontal container
            Widget::Container {
                orientation: Orientation::Horizontal,
                spacing: 8,
                css_classes: vec![],
                children: vec![
                    Widget::Button {
                        label: Some("Text Button".to_string()),
                        icon: None,
                        on_click: Action {
                            id: "text_button_click".to_string(),
                            params: ActionParams::None,
                        },
                    },
                    Widget::Button {
                        label: None,
                        icon: Some("document-edit".to_string()),
                        on_click: Action {
                            id: "icon_button_click".to_string(),
                            params: ActionParams::None,
                        },
                    },
                    Widget::Button {
                        label: Some("Mixed".to_string()),
                        icon: Some("emblem-ok".to_string()),
                        on_click: Action {
                            id: "mixed_button_click".to_string(),
                            params: ActionParams::None,
                        },
                    },
                ],
            },
            // Switch, Spinner, Checkmark in horizontal container
            Widget::Container {
                orientation: Orientation::Horizontal,
                spacing: 16,
                css_classes: vec![],
                children: vec![
                    Widget::Label {
                        text: "Switch:".to_string(),
                        css_classes: vec![],
                    },
                    Widget::Switch {
                        active: true,
                        sensitive: true,
                        on_toggle: Action {
                            id: "toggle_switch".to_string(),
                            params: ActionParams::None,
                        },
                    },
                    Widget::Label {
                        text: "Spinner:".to_string(),
                        css_classes: vec![],
                    },
                    Widget::Spinner { spinning: true },
                    Widget::Label {
                        text: "Checkmark:".to_string(),
                        css_classes: vec![],
                    },
                    Widget::Checkmark { visible: true },
                ],
            },
        ],
    };
    main_box.append(&renderer.render(&primitives_container, "primitives"));

    // Add separator
    let separator5 = gtk::Separator::new(gtk::Orientation::Horizontal);
    main_box.append(&separator5);

    // ========================================================================
    // Complex Nested Widget Section
    // ========================================================================

    let complex_label = gtk::Label::new(Some("Complex Nested Widget"));
    complex_label.add_css_class("title-3");
    complex_label.set_halign(gtk::Align::Start);
    main_box.append(&complex_label);

    let complex_widget = Widget::Container {
        orientation: Orientation::Vertical,
        spacing: 8,
        css_classes: vec!["card".to_string()],
        children: vec![
            Widget::Label {
                text: "Audio Settings".to_string(),
                css_classes: vec!["title-4".to_string()],
            },
            Widget::Container {
                orientation: Orientation::Horizontal,
                spacing: 8,
                css_classes: vec![],
                children: vec![
                    Widget::Button {
                        label: Some("Reset".to_string()),
                        icon: Some("edit-undo".to_string()),
                        on_click: Action {
                            id: "reset_audio".to_string(),
                            params: ActionParams::None,
                        },
                    },
                    Widget::Button {
                        label: Some("Apply".to_string()),
                        icon: Some("emblem-ok".to_string()),
                        on_click: Action {
                            id: "apply_audio".to_string(),
                            params: ActionParams::None,
                        },
                    },
                ],
            },
        ],
    };
    main_box.append(&renderer.render(&complex_widget, "complex"));

    // ========================================================================
    // Finalize UI
    // ========================================================================

    scrolled.set_child(Some(&main_box));
    window.set_content(Some(&scrolled));

    // Show window
    window.present();

    // Print instructions
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║        Waft UI GTK Widget Test Application                ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ Visual test application for all waft-ui-gtk widget types  ║");
    println!("║                                                            ║");
    println!("║ Interact with widgets to test functionality:              ║");
    println!("║  • Toggle switches and feature cards                      ║");
    println!("║  • Adjust sliders                                         ║");
    println!("║  • Click menu rows and buttons                            ║");
    println!("║  • Expand/collapse menus                                  ║");
    println!("║                                                            ║");
    println!("║ Action callbacks will print to this console               ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Run GTK main loop
    let app = gtk::Application::builder()
        .application_id("com.waft.ui-gtk.test-app")
        .build();

    app.connect_activate(move |_| {});
    let _hold = app.hold();

    // Keep window open
    window.connect_close_request(move |_| {
        println!("\n👋 Closing widget test application");
        gtk::glib::Propagation::Proceed
    });

    gtk::glib::MainLoop::new(None, false).run();
}
