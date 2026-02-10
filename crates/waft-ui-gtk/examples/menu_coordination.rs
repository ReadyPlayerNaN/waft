//! Menu coordination testing
//!
//! This example tests the "only one menu open at a time" coordination between
//! multiple expandable widgets (FeatureToggle and Slider).
//!
//! Run with: cargo run -p waft-ui-gtk --example menu_coordination

use adw::prelude::*;
use std::rc::Rc;
use waft_core::menu_state::{create_menu_store, MenuOp};
use waft_ui_gtk::renderer::{ActionCallback, WidgetRenderer};
use waft_ipc::widget::{Action, ActionParams, Orientation, Widget};

fn main() {
    // Initialize GTK and libadwaita
    gtk::init().expect("Failed to initialize GTK");
    adw::init().expect("Failed to initialize libadwaita");

    // Create action callback
    let action_callback: ActionCallback = Rc::new(|widget_id, action| {
        println!(
            "🎯 Action: [{}] {} ({:?})",
            widget_id, action.id, action.params
        );
    });

    // Create menu store (shared across all widgets)
    let menu_store = Rc::new(create_menu_store());
    let renderer = WidgetRenderer::new(menu_store.clone(), action_callback);

    // Create main window
    let window = adw::ApplicationWindow::builder()
        .title("Menu Coordination Test")
        .default_width(400)
        .default_height(700)
        .build();

    // Create main layout
    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    main_box.set_margin_top(12);
    main_box.set_margin_bottom(12);
    main_box.set_margin_start(12);
    main_box.set_margin_end(12);

    // Header
    let header = gtk::Label::new(Some("Menu Coordination Test"));
    header.add_css_class("title-1");
    main_box.append(&header);

    let description = gtk::Label::new(Some(
        "Only one menu can be open at a time.\nTry expanding different widgets to see coordination.",
    ));
    description.add_css_class("dim-label");
    description.set_justify(gtk::Justification::Center);
    main_box.append(&description);

    main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Status label showing menu store state
    let status_label = gtk::Label::new(Some("No menu open"));
    status_label.add_css_class("title-4");
    status_label.add_css_class("accent");
    main_box.append(&status_label);

    main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Create scrolled window for widgets
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let widgets_box = gtk::Box::new(gtk::Orientation::Vertical, 8);

    // ========================================================================
    // Feature Toggle 1: Bluetooth (expandable with device list)
    // ========================================================================

    let bluetooth_widget = Widget::FeatureToggle {
        title: "Bluetooth".to_string(),
        icon: "bluetooth-active".to_string(),
        details: Some("3 devices".to_string()),
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
                    label: "Headphones".to_string(),
                    sublabel: Some("Connected".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: None,
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("audio-speakers".to_string()),
                    label: "Speaker".to_string(),
                    sublabel: Some("Connected".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: None,
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("input-keyboard".to_string()),
                    label: "Keyboard".to_string(),
                    sublabel: Some("Connected".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: None,
                }
                .into(),
            ],
        })),
        on_toggle: Action {
            id: "toggle_bluetooth".to_string(),
            params: ActionParams::None,
        },
    };
    widgets_box.append(&renderer.render(&bluetooth_widget, "bluetooth"));

    // ========================================================================
    // Feature Toggle 2: Wi-Fi (expandable with network list)
    // ========================================================================

    let wifi_widget = Widget::FeatureToggle {
        title: "Wi-Fi".to_string(),
        icon: "network-wireless".to_string(),
        details: Some("HomeNetwork".to_string()),
        active: true,
        busy: false,
        expandable: true,
        expanded_content: Some(Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 0,
            css_classes: vec!["menu-container".to_string()],
            children: vec![
                Widget::MenuRow {
                    icon: Some("network-wireless-signal-excellent".to_string()),
                    label: "HomeNetwork".to_string(),
                    sublabel: Some("Connected • 5GHz".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: None,
                }
                .into(),
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
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("network-wireless-signal-weak".to_string()),
                    label: "Public WiFi".to_string(),
                    sublabel: Some("Open".to_string()),
                    trailing: None,
                    sensitive: true,
                    on_click: Some(Action {
                        id: "connect_network".to_string(),
                        params: ActionParams::String("Public WiFi".to_string()),
                    }),
                }
                .into(),
            ],
        })),
        on_toggle: Action {
            id: "toggle_wifi".to_string(),
            params: ActionParams::None,
        },
    };
    widgets_box.append(&renderer.render(&wifi_widget, "wifi"));

    // ========================================================================
    // Feature Toggle 3: VPN (expandable with connection list)
    // ========================================================================

    let vpn_widget = Widget::FeatureToggle {
        title: "VPN".to_string(),
        icon: "network-vpn".to_string(),
        details: Some("Office VPN".to_string()),
        active: true,
        busy: false,
        expandable: true,
        expanded_content: Some(Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 0,
            css_classes: vec!["menu-container".to_string()],
            children: vec![
                Widget::MenuRow {
                    icon: Some("network-vpn".to_string()),
                    label: "Office VPN".to_string(),
                    sublabel: Some("Connected".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: None,
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("network-vpn".to_string()),
                    label: "Home VPN".to_string(),
                    sublabel: Some("Available".to_string()),
                    trailing: None,
                    sensitive: true,
                    on_click: Some(Action {
                        id: "connect_vpn".to_string(),
                        params: ActionParams::String("home".to_string()),
                    }),
                }
                .into(),
            ],
        })),
        on_toggle: Action {
            id: "toggle_vpn".to_string(),
            params: ActionParams::None,
        },
    };
    widgets_box.append(&renderer.render(&vpn_widget, "vpn"));

    // Add separator
    widgets_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // ========================================================================
    // Slider 1: Volume (expandable with output devices)
    // ========================================================================

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
                    icon: Some("audio-speakers".to_string()),
                    label: "Built-in Speakers".to_string(),
                    sublabel: Some("Default".to_string()),
                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                    sensitive: true,
                    on_click: None,
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("audio-headphones".to_string()),
                    label: "Headphones".to_string(),
                    sublabel: Some("USB Audio".to_string()),
                    trailing: None,
                    sensitive: true,
                    on_click: Some(Action {
                        id: "switch_output".to_string(),
                        params: ActionParams::String("headphones".to_string()),
                    }),
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("audio-card".to_string()),
                    label: "HDMI Audio".to_string(),
                    sublabel: Some("Monitor".to_string()),
                    trailing: None,
                    sensitive: true,
                    on_click: Some(Action {
                        id: "switch_output".to_string(),
                        params: ActionParams::String("hdmi".to_string()),
                    }),
                }
                .into(),
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
    widgets_box.append(&renderer.render(&volume_widget, "volume"));

    // ========================================================================
    // Slider 2: Brightness (expandable with presets)
    // ========================================================================

    let brightness_widget = Widget::Slider {
        icon: "display-brightness".to_string(),
        value: 0.8,
        muted: false,
        expandable: true,
        expanded_content: Some(Box::new(Widget::Container {
            orientation: Orientation::Vertical,
            spacing: 0,
            css_classes: vec!["menu-container".to_string()],
            children: vec![
                Widget::MenuRow {
                    icon: Some("weather-clear".to_string()),
                    label: "Bright (100%)".to_string(),
                    sublabel: None,
                    trailing: None,
                    sensitive: true,
                    on_click: Some(Action {
                        id: "set_brightness_preset".to_string(),
                        params: ActionParams::Value(1.0),
                    }),
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("weather-few-clouds".to_string()),
                    label: "Medium (60%)".to_string(),
                    sublabel: None,
                    trailing: None,
                    sensitive: true,
                    on_click: Some(Action {
                        id: "set_brightness_preset".to_string(),
                        params: ActionParams::Value(0.6),
                    }),
                }
                .into(),
                Widget::MenuRow {
                    icon: Some("weather-clear-night".to_string()),
                    label: "Dim (30%)".to_string(),
                    sublabel: None,
                    trailing: None,
                    sensitive: true,
                    on_click: Some(Action {
                        id: "set_brightness_preset".to_string(),
                        params: ActionParams::Value(0.3),
                    }),
                }
                .into(),
            ],
        })),
        on_value_change: Action {
            id: "set_brightness".to_string(),
            params: ActionParams::Value(0.8),
        },
        on_icon_click: Action {
            id: "auto_brightness".to_string(),
            params: ActionParams::None,
        },
    };
    widgets_box.append(&renderer.render(&brightness_widget, "brightness"));

    // ========================================================================
    // Finalize UI
    // ========================================================================

    scrolled.set_child(Some(&widgets_box));
    main_box.append(&scrolled);

    // Add test buttons
    main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let button_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let info_button = gtk::Button::with_label("🔍 Check MenuStore State");
    info_button.connect_clicked({
        let menu_store = menu_store.clone();
        let status_label = status_label.clone();
        move |_| {
            let state = menu_store.get_state();
            let active_menu = &state.active_menu_id;
            match active_menu {
                Some(menu_id) => {
                    println!("📋 MenuStore state: active_menu_id = Some(\"{}\")", menu_id);
                    status_label.set_text(&format!("Active menu: {}", menu_id));
                }
                None => {
                    println!("📋 MenuStore state: active_menu_id = None");
                    status_label.set_text("No menu open");
                }
            }
        }
    });
    button_box.append(&info_button);

    let test_button = gtk::Button::with_label("🧪 Run Coordination Test");
    test_button.connect_clicked({
        let menu_store = menu_store.clone();
        move |_| {
            println!("\n🧪 Running menu coordination test...");

            // Simulate opening different menus in sequence
            let test_sequence = vec![
                ("bluetooth_menu", "Bluetooth"),
                ("wifi_menu", "Wi-Fi"),
                ("vpn_menu", "VPN"),
                ("volume_menu", "Volume"),
                ("brightness_menu", "Brightness"),
            ];

            for (menu_id, name) in test_sequence {
                // Request to open menu
                menu_store.emit(MenuOp::OpenMenu(menu_id.to_string()));

                // Check active menu
                let state = menu_store.get_state();
                let active = &state.active_menu_id;
                match active {
                    Some(id) if id == menu_id => {
                        println!("  ✅ {} menu opened successfully", name);
                    }
                    Some(other) => {
                        println!("  ❌ Expected {} but got {}", menu_id, other);
                    }
                    None => {
                        println!("  ❌ No menu active (expected {})", menu_id);
                    }
                }
            }

            // Close all menus
            menu_store.emit(MenuOp::CloseAll);
            let state = menu_store.get_state();
            let active = &state.active_menu_id;
            if active.is_none() {
                println!("  ✅ All menus closed successfully");
            } else {
                println!("  ❌ Menu still open: {:?}", active);
            }

            println!("🧪 Test complete!");
        }
    });
    button_box.append(&test_button);

    main_box.append(&button_box);

    window.set_content(Some(&main_box));
    window.present();

    // Print instructions
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║           Menu Coordination Testing Application           ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ This app tests the 'only one menu open at a time'         ║");
    println!("║ coordination between expandable widgets.                  ║");
    println!("║                                                            ║");
    println!("║ Try the following:                                        ║");
    println!("║  1. Click chevron on Bluetooth to expand menu             ║");
    println!("║  2. Click chevron on Wi-Fi (Bluetooth should close)       ║");
    println!("║  3. Click chevron on VPN (Wi-Fi should close)             ║");
    println!("║  4. Try Volume and Brightness sliders                     ║");
    println!("║  5. Click 'Check MenuStore State' to see active menu      ║");
    println!("║  6. Click 'Run Coordination Test' for automated test      ║");
    println!("║                                                            ║");
    println!("║ Expected behavior:                                        ║");
    println!("║  • Only ONE menu can be expanded at any time              ║");
    println!("║  • Opening a new menu closes the previous one             ║");
    println!("║  • MenuStore tracks the currently active menu             ║");
    println!("║  • Menu IDs are deterministic (widgetId + '_menu')        ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Run GTK main loop
    let app = gtk::Application::builder()
        .application_id("com.waft.ui-gtk.menu-coordination")
        .build();

    app.connect_activate(move |_| {});
    let _hold = app.hold();

    window.connect_close_request(move |_| {
        println!("\n👋 Closing menu coordination test");
        gtk::glib::Propagation::Proceed
    });

    gtk::glib::MainLoop::new(None, false).run();
}
