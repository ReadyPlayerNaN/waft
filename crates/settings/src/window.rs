//! Main settings window with AdwNavigationSplitView layout.
//!
//! Contains a sidebar for category navigation and a content area
//! that displays the selected settings page via a gtk::Stack.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::network::{ADAPTER_ENTITY_TYPE, AdapterKind, NetworkAdapter};

use crate::i18n::t;
use crate::pages::appearance::AppearancePage;
use crate::pages::audio::AudioPage;
use crate::pages::bluetooth::BluetoothPage;
use crate::pages::display::DisplayPage;
use crate::pages::keyboard::KeyboardPage;
use crate::pages::notifications::NotificationsPage;
use crate::pages::plugins::PluginsPage;
use crate::pages::services::ServicesPage;
use crate::pages::sounds::SoundsPage;
use crate::pages::wallpaper::WallpaperPage;
use crate::pages::weather::WeatherPage;
use crate::pages::wifi::WiFiPage;
use crate::pages::wired::WiredPage;
use crate::search_index::SearchIndex;
use crate::sidebar::{Sidebar, SidebarOutput};

/// Map a page_id to its translated display title.
fn page_title(page_id: &str) -> String {
    let key = match page_id {
        "audio" => "settings-audio",
        "bluetooth" => "settings-bluetooth",
        "wifi" => "settings-wifi",
        "wired" => "settings-wired",
        "appearance" => "settings-appearance",
        "display" => "settings-display",
        "notifications" => "settings-notifications",
        "sounds" => "settings-sounds",
        "keyboard" => "settings-keyboard",
        "wallpaper" => "settings-wallpaper",
        "weather" => "settings-weather",
        "plugins" => "settings-plugins",
        "services" => "settings-services",
        _ => "settings-bluetooth",
    };
    t(key)
}

/// The main settings window.
pub struct SettingsWindow {
    pub window: adw::ApplicationWindow,
}

impl SettingsWindow {
    pub fn new(
        app: &adw::Application,
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        initial_page: Option<&str>,
    ) -> Self {
        let search_index = Rc::new(RefCell::new(SearchIndex::new()));
        let split_view = adw::NavigationSplitView::new();

        // -- Sidebar --
        let sidebar = Sidebar::new(search_index.clone());
        let sidebar_header = adw::HeaderBar::new();

        // Search toggle button in sidebar header
        let search_button = gtk::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .build();
        sidebar_header.pack_start(&search_button);

        // Bind search button to search bar
        sidebar
            .search_bar
            .bind_property("search-mode-enabled", &search_button, "active")
            .bidirectional()
            .sync_create()
            .build();

        let sidebar_toolbar = adw::ToolbarView::new();
        sidebar_toolbar.add_top_bar(&sidebar_header);

        let sidebar_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        sidebar_scrolled.set_child(Some(&sidebar.root));
        sidebar_toolbar.set_content(Some(&sidebar_scrolled));

        let sidebar_page = adw::NavigationPage::builder()
            .title(t("settings-title"))
            .child(&sidebar_toolbar)
            .build();

        split_view.set_sidebar(Some(&sidebar_page));

        // -- Content pages --
        // Register page-level search entries, then construct pages which
        // register their own section/input entries via search_index.
        {
            let mut idx = search_index.borrow_mut();
            idx.add_page("audio", &t("settings-audio"), "settings-audio");
            idx.add_page("bluetooth", &t("settings-bluetooth"), "settings-bluetooth");
            idx.add_page("wifi", &t("settings-wifi"), "settings-wifi");
            idx.add_page("wired", &t("settings-wired"), "settings-wired");
            idx.add_page("weather", &t("settings-weather"), "settings-weather");
            idx.add_page("appearance", &t("settings-appearance"), "settings-appearance");
            idx.add_page("display", &t("settings-display"), "settings-display");
            idx.add_page("wallpaper", &t("settings-wallpaper"), "settings-wallpaper");
            idx.add_page("keyboard", &t("settings-keyboard"), "settings-keyboard");
            idx.add_page("notifications", &t("settings-notifications"), "settings-notifications");
            idx.add_page("sounds", &t("settings-sounds"), "settings-sounds");
            idx.add_page("plugins", &t("settings-plugins"), "settings-plugins");
            idx.add_page("services", &t("settings-services"), "settings-services");
        }

        let audio_page = AudioPage::new(entity_store, action_callback, &search_index);
        let bluetooth_page = BluetoothPage::new(entity_store, action_callback, &search_index);
        let wifi_page = WiFiPage::new(entity_store, action_callback, &search_index);
        let wired_page = WiredPage::new(entity_store, action_callback, &search_index);
        let weather_page = WeatherPage::new(entity_store, action_callback, &search_index);
        let appearance_page = AppearancePage::new(entity_store, action_callback, &search_index);
        let display_page = DisplayPage::new(entity_store, action_callback, &search_index);
        let wallpaper_page = WallpaperPage::new(entity_store, action_callback, &search_index);
        let keyboard_page = KeyboardPage::new(entity_store, action_callback, &search_index);
        let notifications_page = NotificationsPage::new(entity_store, action_callback, &search_index);
        let sounds_page = SoundsPage::new(entity_store, action_callback, &search_index);
        let plugins_page = PluginsPage::new(entity_store, &search_index);
        let services_page = ServicesPage::new(entity_store, action_callback, &search_index);

        // Wrap each page in a clamp for consistent max width
        let audio_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&audio_page.root)
            .build();
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
        let appearance_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&appearance_page.root)
            .build();
        let display_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&display_page.root)
            .build();
        let wallpaper_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&wallpaper_page.root)
            .build();
        let keyboard_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&keyboard_page.root)
            .build();
        let notif_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&notifications_page.root)
            .build();

        let sounds_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&sounds_page.root)
            .build();

        let plugins_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&plugins_page.root)
            .build();

        let services_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&services_page.root)
            .build();

        // Stack for page switching (keyed by stable page_id)
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&audio_clamp, Some("audio"));
        stack.add_named(&bt_clamp, Some("bluetooth"));
        stack.add_named(&wifi_clamp, Some("wifi"));
        stack.add_named(&wired_clamp, Some("wired"));
        stack.add_named(&weather_clamp, Some("weather"));
        stack.add_named(&appearance_clamp, Some("appearance"));
        stack.add_named(&display_clamp, Some("display"));
        stack.add_named(&wallpaper_clamp, Some("wallpaper"));
        stack.add_named(&keyboard_clamp, Some("keyboard"));
        stack.add_named(&notif_clamp, Some("notifications"));
        stack.add_named(&sounds_clamp, Some("sounds"));
        stack.add_named(&plugins_clamp, Some("plugins"));
        stack.add_named(&services_clamp, Some("services"));
        // Navigate to the requested page, or default to bluetooth
        let default_page = initial_page.unwrap_or("bluetooth");
        if stack.child_by_name(default_page).is_some() {
            stack.set_visible_child_name(default_page);
        } else {
            if let Some(page_id) = initial_page {
                log::warn!(
                    "[settings] Requested page '{}' not found, falling back to bluetooth",
                    page_id
                );
            }
            stack.set_visible_child_name("bluetooth");
        }

        let content_header = adw::HeaderBar::new();
        let content_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        content_scrolled.set_child(Some(&stack));

        let content_toolbar = adw::ToolbarView::new();
        content_toolbar.add_top_bar(&content_header);
        content_toolbar.set_content(Some(&content_scrolled));

        let initial_title =
            page_title(stack.visible_child_name().as_deref().unwrap_or("bluetooth"));
        let content_page = adw::NavigationPage::builder()
            .title(initial_title)
            .child(&content_toolbar)
            .build();

        split_view.set_content(Some(&content_page));

        // -- Connect sidebar selection --
        let stack_ref = stack.clone();
        let content_scrolled_ref = content_scrolled.clone();
        let current_page: Rc<RefCell<String>> = Rc::new(RefCell::new(
            stack.visible_child_name().map(|s| s.to_string()).unwrap_or_else(|| "bluetooth".to_string()),
        ));
        let current_page_ref = current_page.clone();
        sidebar.connect_output(move |output| {
            let (new_page_id, new_title) = match output {
                SidebarOutput::Selected { page_id, title } => {
                    log::debug!("[settings] sidebar selected: {page_id} ({title})");
                    (page_id, title)
                }
                SidebarOutput::SearchSelected {
                    page_id,
                    page_title,
                    target_widget,
                } => {
                    log::debug!("[settings] search selected: {page_id}");

                    // Scroll to target widget after page switch completes
                    if let Some(weak) = target_widget {
                        let scrolled = content_scrolled_ref.clone();
                        gtk::glib::idle_add_local_once(move || {
                            if let Some(widget) = weak.upgrade() {
                                scroll_to_and_highlight(&scrolled, &widget);
                            }
                        });
                    }
                    (page_id, page_title)
                }
            };

            // Reset display page when leaving it
            let prev = current_page_ref.borrow().clone();
            if prev == "display" && new_page_id != "display" {
                display_page.reset();
            }

            content_page.set_title(&new_title);
            stack_ref.set_visible_child_name(&new_page_id);
            *current_page_ref.borrow_mut() = new_page_id;
        });

        // Sync sidebar selection with the initial page.
        // The sidebar defaults to "bluetooth" at construction time; for any
        // other initial page, navigate_to fires the output callback so the
        // sidebar highlight and content header title both update correctly.
        sidebar.navigate_to(default_page);

        // Move sidebar into Rc for shared access across action handler and WiFi
        // visibility subscription.
        let sidebar_ref = Rc::new(sidebar);

        // Register navigate-to action on the app for single-instance page navigation.
        // When waft-settings is already running, a second invocation forwards --page
        // to connect_command_line in the primary instance, which activates this action.
        {
            let nav_action = gtk::gio::SimpleAction::new(
                "navigate-to",
                Some(gtk::glib::VariantTy::STRING),
            );
            let sidebar_for_action = sidebar_ref.clone();
            nav_action.connect_activate(move |_, param| {
                if let Some(page_id) = param.and_then(|p| p.str()) {
                    sidebar_for_action.navigate_to(page_id);
                }
            });
            app.add_action(&nav_action);
        }

        // -- WiFi sidebar visibility based on adapter presence --
        {
            let store = entity_store.clone();
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
                    && name == "wifi"
                {
                    stack_for_wifi.set_visible_child_name("bluetooth");
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
        }

        // -- Window --
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(t("settings-window-title"))
            .default_width(900)
            .default_height(600)
            .content(&split_view)
            .build();

        // Set search bar key capture widget to window for type-to-search
        sidebar_ref
            .search_bar
            .set_key_capture_widget(Some(&window));

        // Prevent sidebar from being dropped
        std::mem::forget(sidebar_ref);

        Self { window }
    }
}

/// Scroll a widget into view within a ScrolledWindow and apply a brief highlight.
fn scroll_to_and_highlight(scrolled: &gtk::ScrolledWindow, widget: &gtk::Widget) {
    if let Some((_, y)) = widget.translate_coordinates(scrolled, 0.0, 0.0) {
        let adj = scrolled.vadjustment();
        let target = adj.value() + y - 20.0; // 20px margin above
        adj.set_value(target.max(0.0));
    }

    // Add highlight class and grab focus
    widget.add_css_class("search-highlight");
    widget.grab_focus();

    // Remove highlight after 1.5s
    let weak = widget.downgrade();
    gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || {
        if let Some(w) = weak.upgrade() {
            w.remove_css_class("search-highlight");
        }
    });
}

