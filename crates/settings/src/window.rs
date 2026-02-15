//! Main settings window with AdwNavigationSplitView layout.
//!
//! Contains a sidebar for category navigation and a content area
//! that displays the selected settings page via a gtk::Stack.

use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::network::{ADAPTER_ENTITY_TYPE, AdapterKind, NetworkAdapter};

use crate::pages::bluetooth::BluetoothPage;
use crate::pages::display::DisplayPage;
use crate::pages::keyboard::KeyboardPage;
use crate::pages::notifications::NotificationsPage;
use crate::pages::weather::WeatherPage;
use crate::pages::wifi::WiFiPage;
use crate::pages::wired::WiredPage;
use crate::sidebar::{Sidebar, SidebarOutput};

/// The main settings window.
pub struct SettingsWindow {
    pub window: adw::ApplicationWindow,
}

impl SettingsWindow {
    pub fn new(
        app: &adw::Application,
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
    ) -> Self {
        let split_view = adw::NavigationSplitView::new();

        // -- Sidebar --
        let sidebar = Sidebar::new();
        let sidebar_header = adw::HeaderBar::new();
        let sidebar_toolbar = adw::ToolbarView::new();
        sidebar_toolbar.add_top_bar(&sidebar_header);
        sidebar_toolbar.set_content(Some(&sidebar.root));

        let sidebar_page = adw::NavigationPage::builder()
            .title("Settings")
            .child(&sidebar_toolbar)
            .build();

        split_view.set_sidebar(Some(&sidebar_page));

        // -- Content pages --
        let bluetooth_page = BluetoothPage::new(entity_store, action_callback);
        let wifi_page = WiFiPage::new(entity_store, action_callback);
        let wired_page = WiredPage::new(entity_store, action_callback);
        let weather_page = WeatherPage::new(entity_store, action_callback);
        let display_page = DisplayPage::new(entity_store, action_callback);
        let keyboard_page = KeyboardPage::new(entity_store, action_callback);
        let notifications_page = NotificationsPage::new(entity_store, action_callback);

        // Wrap each page in a clamp for consistent max width
        let bt_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&bluetooth_page.root)
            .build();
        let wifi_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&wifi_page.root)
            .build();
        let wired_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&wired_page.root)
            .build();
        let weather_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&weather_page.root)
            .build();
        let display_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&display_page.root)
            .build();
        let keyboard_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&keyboard_page.root)
            .build();
        let notif_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&notifications_page.root)
            .build();

        // Stack for page switching
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&bt_clamp, Some("Bluetooth"));
        stack.add_named(&wifi_clamp, Some("WiFi"));
        stack.add_named(&wired_clamp, Some("Wired"));
        stack.add_named(&weather_clamp, Some("Weather"));
        stack.add_named(&display_clamp, Some("Display"));
        stack.add_named(&keyboard_clamp, Some("Keyboard"));
        stack.add_named(&notif_clamp, Some("Notifications"));
        stack.set_visible_child_name("Bluetooth");

        let content_header = adw::HeaderBar::new();
        let content_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        content_scrolled.set_child(Some(&stack));

        let content_toolbar = adw::ToolbarView::new();
        content_toolbar.add_top_bar(&content_header);
        content_toolbar.set_content(Some(&content_scrolled));

        let content_page = adw::NavigationPage::builder()
            .title("Bluetooth")
            .child(&content_toolbar)
            .build();

        split_view.set_content(Some(&content_page));

        // -- Connect sidebar selection --
        let stack_ref = stack.clone();
        sidebar.connect_output(move |output| match output {
            SidebarOutput::Selected(category) => {
                log::debug!("[settings] sidebar selected: {category}");
                content_page.set_title(&category);
                stack_ref.set_visible_child_name(&category);
            }
        });

        // -- WiFi sidebar visibility based on adapter presence --
        {
            let store = entity_store.clone();
            let sidebar_ref = Rc::new(sidebar);
            let sidebar_for_sub = sidebar_ref.clone();
            let stack_for_wifi = stack;

            entity_store.subscribe_type(ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, NetworkAdapter)> =
                    store.get_entities_typed(ADAPTER_ENTITY_TYPE);
                let has_wireless = adapters
                    .iter()
                    .any(|(_, a)| a.kind == AdapterKind::Wireless);
                sidebar_for_sub.set_wifi_visible(has_wireless);

                // If WiFi page is active and WiFi row was hidden, switch to Bluetooth
                if !has_wireless
                    && let Some(name) = stack_for_wifi.visible_child_name()
                    && name == "WiFi"
                {
                    stack_for_wifi.set_visible_child_name("Bluetooth");
                }
            });

            // Initial WiFi visibility check
            {
                let store = entity_store.clone();
                let sidebar_for_init = sidebar_ref.clone();
                gtk::glib::idle_add_local_once(move || {
                    let adapters: Vec<(Urn, NetworkAdapter)> =
                        store.get_entities_typed(ADAPTER_ENTITY_TYPE);
                    let has_wireless = adapters
                        .iter()
                        .any(|(_, a)| a.kind == AdapterKind::Wireless);
                    sidebar_for_init.set_wifi_visible(has_wireless);
                });
            }

            // -- Window --
            let window = adw::ApplicationWindow::builder()
                .application(app)
                .title("Waft Settings")
                .default_width(900)
                .default_height(600)
                .content(&split_view)
                .build();

            // Prevent sidebar from being dropped
            std::mem::forget(sidebar_ref);

            Self { window }
        }
    }
}
