//! Dynamic widget update testing
//!
//! This example tests dynamic addition, removal, and updates of widgets,
//! simulating how plugins update their UI in real-time.
//!
//! Run with: cargo run -p waft-ui-gtk --example dynamic_updates

use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use waft_core::menu_state::create_menu_store;
use waft_ui_gtk::renderer::{ActionCallback, WidgetRenderer};
use waft_ipc::widget::{Action, ActionParams, Node, Widget};

/// State for the dynamic test application
struct AppState {
    renderer: Rc<WidgetRenderer>,
    container: gtk::Box,
    slider_value: f64,
    toggle_active: bool,
    device_count: usize,
}

impl AppState {
    fn new(renderer: Rc<WidgetRenderer>, container: gtk::Box) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            renderer,
            container,
            slider_value: 0.5,
            toggle_active: false,
            device_count: 2,
        }))
    }

    /// Clear all widgets from container
    fn clear_widgets(&self) {
        while let Some(child) = self.container.first_child() {
            self.container.remove(&child);
        }
    }

    /// Render a list of widgets into the container
    fn render_widgets(&self, widgets: Vec<(&str, Widget)>) {
        for (id, widget) in widgets {
            let gtk_widget = self.renderer.render(&widget, id);
            self.container.append(&gtk_widget);
        }
    }

    /// Test 1: Add widgets progressively
    fn test_add_widgets(&mut self) {
        println!("\n📦 Test 1: Progressive Widget Addition");
        self.clear_widgets();

        let widgets = vec![
            (
                "label1",
                Widget::Label {
                    text: "First widget added".to_string(),
                    css_classes: vec!["title-4".to_string()],
                },
            ),
            (
                "label2",
                Widget::Label {
                    text: "Second widget added".to_string(),
                    css_classes: vec![],
                },
            ),
            (
                "toggle1",
                Widget::FeatureToggle {
                    title: "New Feature".to_string(),
                    icon: "emblem-ok".to_string(),
                    details: Some("Just added".to_string()),
                    active: true,
                    busy: false,
                    expandable: false,
                    expanded_content: None,
                    on_toggle: Action {
                        id: "toggle_new".to_string(),
                        params: ActionParams::None,
                    },
                },
            ),
        ];

        self.render_widgets(widgets);
        println!("✅ Added 3 widgets progressively");
    }

    /// Test 2: Remove widgets
    fn test_remove_widgets(&mut self) {
        println!("\n🗑️  Test 2: Widget Removal");
        self.clear_widgets();

        // Start with 5 widgets
        let widgets = vec![
            (
                "w1",
                Widget::Label {
                    text: "Widget 1 (will stay)".to_string(),
                    css_classes: vec![],
                },
            ),
            (
                "w2",
                Widget::Label {
                    text: "Widget 2 (will be removed)".to_string(),
                    css_classes: vec![],
                },
            ),
            (
                "w3",
                Widget::Label {
                    text: "Widget 3 (will stay)".to_string(),
                    css_classes: vec![],
                },
            ),
            (
                "w4",
                Widget::Label {
                    text: "Widget 4 (will be removed)".to_string(),
                    css_classes: vec![],
                },
            ),
            (
                "w5",
                Widget::Label {
                    text: "Widget 5 (will stay)".to_string(),
                    css_classes: vec![],
                },
            ),
        ];

        self.render_widgets(widgets);
        println!("✅ Initially added 5 widgets");

        // Now remove widgets 2 and 4 by re-rendering without them
        gtk::glib::timeout_add_seconds_local_once(2, {
            let container = self.container.clone();
            let renderer = self.renderer.clone();
            move || {
                println!("🗑️  Removing widgets 2 and 4...");

                // Clear and re-render only w1, w3, w5
                while let Some(child) = container.first_child() {
                    container.remove(&child);
                }

                let remaining_widgets = vec![
                    (
                        "w1",
                        Widget::Label {
                            text: "Widget 1 (stayed)".to_string(),
                            css_classes: vec!["accent".to_string()],
                        },
                    ),
                    (
                        "w3",
                        Widget::Label {
                            text: "Widget 3 (stayed)".to_string(),
                            css_classes: vec!["accent".to_string()],
                        },
                    ),
                    (
                        "w5",
                        Widget::Label {
                            text: "Widget 5 (stayed)".to_string(),
                            css_classes: vec!["accent".to_string()],
                        },
                    ),
                ];

                for (id, widget) in remaining_widgets {
                    let gtk_widget = renderer.render(&widget, id);
                    container.append(&gtk_widget);
                }

                println!("✅ Removed 2 widgets, 3 remaining");
            }
        });
    }

    /// Test 3: Update widget properties
    fn test_update_properties(&mut self) {
        println!("\n🔄 Test 3: Widget Property Updates");
        self.clear_widgets();
        self.slider_value = 0.0;
        self.toggle_active = false;

        // Schedule periodic updates
        let state_clone = Rc::new(RefCell::new((
            self.renderer.clone(),
            self.container.clone(),
            self.slider_value,
            self.toggle_active,
        )));

        gtk::glib::timeout_add_seconds_local(1, move || {
            let (renderer, container, mut slider_value, mut toggle_active) =
                state_clone.borrow().clone();

            // Update values
            slider_value = (slider_value + 0.1).min(1.0);
            if slider_value >= 1.0 {
                slider_value = 0.0;
                toggle_active = !toggle_active;
            }

            // Clear and re-render with updated values
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }

            let widgets = vec![
                (
                    "slider",
                    Widget::Slider {
                        icon: "audio-volume-high".to_string(),
                        value: slider_value,
                        muted: false,
                        expandable: false,
                        expanded_content: None,
                        on_value_change: Action {
                            id: "set_volume".to_string(),
                            params: ActionParams::Value(slider_value),
                        },
                        on_icon_click: Action {
                            id: "toggle_mute".to_string(),
                            params: ActionParams::None,
                        },
                    },
                ),
                (
                    "toggle",
                    Widget::FeatureToggle {
                        title: "Feature Toggle".to_string(),
                        icon: if toggle_active {
                            "emblem-ok"
                        } else {
                            "emblem-important"
                        }
                        .to_string(),
                        details: Some(format!(
                            "Slider: {:.0}%, Active: {}",
                            slider_value * 100.0,
                            toggle_active
                        )),
                        active: toggle_active,
                        busy: false,
                        expandable: false,
                        expanded_content: None,
                        on_toggle: Action {
                            id: "toggle".to_string(),
                            params: ActionParams::None,
                        },
                    },
                ),
            ];

            for (id, widget) in widgets {
                let gtk_widget = renderer.render(&widget, id);
                container.append(&gtk_widget);
            }

            println!(
                "🔄 Updated: slider={:.1}%, toggle={}",
                slider_value * 100.0,
                toggle_active
            );

            // Update state for next iteration
            *state_clone.borrow_mut() = (renderer, container, slider_value, toggle_active);

            gtk::glib::ControlFlow::Continue
        });

        println!("✅ Started periodic property updates (every 1s)");
    }

    /// Test 4: Dynamic list updates (simulating device list)
    fn test_dynamic_list(&mut self) {
        println!("\n📋 Test 4: Dynamic List Updates");
        self.clear_widgets();
        self.device_count = 0;

        let state_clone = Rc::new(RefCell::new((
            self.renderer.clone(),
            self.container.clone(),
            self.device_count,
        )));

        // Add a device every 2 seconds
        gtk::glib::timeout_add_seconds_local(2, move || {
            let (renderer, container, mut device_count) = state_clone.borrow().clone();

            device_count += 1;

            // Clear and re-render list
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }

            // Create header
            let header = Widget::Label {
                text: format!("Devices ({}/5)", device_count),
                css_classes: vec!["title-3".to_string()],
            };
            container.append(&renderer.render(&header, "header"));

            // Create device list
            let mut children: Vec<Node> = Vec::new();
            for i in 1..=device_count {
                children.push(
                    Widget::MenuRow {
                        icon: Some(format!("device-{}", i % 4)),
                        label: format!("Device {}", i),
                        sublabel: Some("Connected".to_string()),
                        trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                        sensitive: true,
                        on_click: Some(Action {
                            id: "select_device".to_string(),
                            params: ActionParams::String(format!("device_{}", i)),
                        }),
                    }
                    .into(),
                );
            }

            let device_container = Widget::Col {
                spacing: 0,
                css_classes: vec!["menu-container".to_string()],
                children,
            };

            container.append(&renderer.render(&device_container, "devices"));

            println!("📋 Added device {}/{}", device_count, 5);

            *state_clone.borrow_mut() = (renderer, container, device_count);

            // Stop after 5 devices
            if device_count >= 5 {
                println!("✅ Completed: 5 devices added");
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });

        println!("✅ Started adding devices (every 2s)");
    }

    /// Test 5: Stress test - rapid updates
    fn test_stress_updates(&mut self) {
        println!("\n⚡ Test 5: Stress Test (Rapid Updates)");
        self.clear_widgets();

        let state_clone = Rc::new(RefCell::new((
            self.renderer.clone(),
            self.container.clone(),
            0u32,
        )));

        // Update every 100ms
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let (renderer, container, mut counter) = state_clone.borrow().clone();

            counter += 1;

            // Clear and re-render
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }

            let widgets = vec![
                (
                    "counter",
                    Widget::Label {
                        text: format!("Update #{}", counter),
                        css_classes: vec!["title-2".to_string()],
                    },
                ),
                (
                    "progress",
                    Widget::Slider {
                        icon: "emblem-synchronizing".to_string(),
                        value: (counter as f64 % 100.0) / 100.0,
                        muted: false,
                        expandable: false,
                        expanded_content: None,
                        on_value_change: Action {
                            id: "progress".to_string(),
                            params: ActionParams::None,
                        },
                        on_icon_click: Action {
                            id: "stop".to_string(),
                            params: ActionParams::None,
                        },
                    },
                ),
            ];

            for (id, widget) in widgets {
                let gtk_widget = renderer.render(&widget, id);
                container.append(&gtk_widget);
            }

            if counter % 10 == 0 {
                println!("⚡ Rapid update #{}", counter);
            }

            *state_clone.borrow_mut() = (renderer, container, counter);

            // Stop after 100 updates
            if counter >= 100 {
                println!("✅ Stress test complete: 100 rapid updates");
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });

        println!("✅ Started stress test (update every 100ms)");
    }
}

fn main() {
    // Initialize GTK and libadwaita
    gtk::init().expect("Failed to initialize GTK");
    adw::init().expect("Failed to initialize libadwaita");

    // Create action callback
    let action_callback: ActionCallback = Rc::new(|widget_id, action| {
        println!(
            "🎯 [{}] {} ({:?})",
            widget_id, action.id, action.params
        );
    });

    // Create renderer
    let menu_store = Rc::new(create_menu_store());
    let renderer = Rc::new(WidgetRenderer::new(menu_store, action_callback));

    // Create main window
    let window = adw::ApplicationWindow::builder()
        .title("Dynamic Widget Updates Test")
        .default_width(450)
        .default_height(600)
        .build();

    // Create main layout
    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    main_box.set_margin_top(12);
    main_box.set_margin_bottom(12);
    main_box.set_margin_start(12);
    main_box.set_margin_end(12);

    // Header
    let header = gtk::Label::new(Some("Dynamic Widget Update Tests"));
    header.add_css_class("title-1");
    main_box.append(&header);

    let description = gtk::Label::new(Some(
        "Tests add/remove/update operations on widgets",
    ));
    description.add_css_class("dim-label");
    main_box.append(&description);

    main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Test buttons
    let button_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    // Create test container (where dynamic widgets will be rendered)
    let test_container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    test_container.add_css_class("card");
    test_container.set_margin_top(12);

    let state = AppState::new(renderer, test_container.clone());

    // Test 1: Add widgets button
    let btn_add = gtk::Button::with_label("Test 1: Add Widgets Progressively");
    btn_add.connect_clicked({
        let state = state.clone();
        move |_| {
            state.borrow_mut().test_add_widgets();
        }
    });
    button_box.append(&btn_add);

    // Test 2: Remove widgets button
    let btn_remove = gtk::Button::with_label("Test 2: Remove Widgets");
    btn_remove.connect_clicked({
        let state = state.clone();
        move |_| {
            state.borrow_mut().test_remove_widgets();
        }
    });
    button_box.append(&btn_remove);

    // Test 3: Update properties button
    let btn_update = gtk::Button::with_label("Test 3: Update Properties (animated)");
    btn_update.connect_clicked({
        let state = state.clone();
        move |_| {
            state.borrow_mut().test_update_properties();
        }
    });
    button_box.append(&btn_update);

    // Test 4: Dynamic list button
    let btn_list = gtk::Button::with_label("Test 4: Dynamic List (devices)");
    btn_list.connect_clicked({
        let state = state.clone();
        move |_| {
            state.borrow_mut().test_dynamic_list();
        }
    });
    button_box.append(&btn_list);

    // Test 5: Stress test button
    let btn_stress = gtk::Button::with_label("Test 5: Stress Test (100 rapid updates)");
    btn_stress.connect_clicked({
        let state = state.clone();
        move |_| {
            state.borrow_mut().test_stress_updates();
        }
    });
    button_box.append(&btn_stress);

    main_box.append(&button_box);
    main_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Add test container
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scrolled.set_child(Some(&test_container));

    main_box.append(&scrolled);

    window.set_content(Some(&main_box));
    window.present();

    // Print instructions
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║     Dynamic Widget Update Testing Application             ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ Click buttons to run different update scenarios:          ║");
    println!("║                                                            ║");
    println!("║  Test 1: Add widgets progressively                        ║");
    println!("║  Test 2: Remove widgets from list                         ║");
    println!("║  Test 3: Update widget properties (animated)              ║");
    println!("║  Test 4: Dynamic list updates (add devices)               ║");
    println!("║  Test 5: Stress test (100 rapid updates)                  ║");
    println!("║                                                            ║");
    println!("║ Each test demonstrates different update patterns that     ║");
    println!("║ plugins might use in production.                          ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Run GTK main loop
    let app = gtk::Application::builder()
        .application_id("com.waft.ui-gtk.dynamic-test")
        .build();

    app.connect_activate(move |_| {});
    let _hold = app.hold();

    window.connect_close_request(move |_| {
        println!("\n👋 Closing dynamic updates test");
        gtk::glib::Propagation::Proceed
    });

    gtk::glib::MainLoop::new(None, false).run();
}
