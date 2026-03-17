//! Main settings window with AdwNavigationSplitView layout.
//!
//! Contains a sidebar for category navigation and a content area
//! that displays the selected settings page via a gtk::Stack.

use std::cell::RefCell;
use std::collections::HashMap;
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
use crate::pages::keyboard_shortcuts::KeyboardShortcutsPage;
use crate::pages::niri_windows::NiriWindowsPage;
use crate::pages::notifications::NotificationsPage;
use crate::pages::online_accounts::OnlineAccountsPage;
use crate::pages::plugins::PluginsPage;
use crate::pages::scheduler::SchedulerPage;
use crate::pages::services::ServicesPage;
use crate::pages::sounds::SoundsPage;
use crate::pages::startup::StartupPage;
use crate::pages::wallpaper::WallpaperPage;
use crate::pages::weather::WeatherPage;
use crate::pages::wifi::WiFiPage;
use crate::pages::wired::WiredPage;
use crate::search_index::SearchIndex;
use crate::sidebar::{Sidebar, SidebarOutput};

type PageFactory = Box<dyn FnOnce() -> gtk::Widget>;

/// Map a page_id to its translated display title.
fn page_title(page_id: &str) -> String {
    let key = match page_id {
        "audio" => "settings-audio",
        "bluetooth" => "settings-bluetooth",
        "wifi" => "settings-wifi",
        "wired" => "settings-wired",
        "online-accounts" => "settings-online-accounts",
        "appearance" => "settings-appearance",
        "display" => "settings-display",
        "notifications" => "settings-notifications",
        "sounds" => "settings-sounds",
        "keyboard" => "settings-keyboard",
        "wallpaper" => "settings-wallpaper",
        "windows" => "settings-windows",
        "weather" => "settings-weather",
        "plugins" => "settings-plugins",
        "services" => "settings-services",
        "startup" => "settings-startup",
        "keyboard-shortcuts" => "settings-keyboard-shortcuts",
        "scheduled-tasks" => "settings-scheduled-tasks",
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

        // Create NavigationView early so sub-page-aware pages can reference it.
        // The root navigation page is added later after the stack is built.
        let navigation_view = adw::NavigationView::new();

        // -- Content pages --
        // Register page-level search entries, then construct pages which
        // register their own section/input entries via search_index.
        {
            let mut idx = search_index.borrow_mut();
            idx.add_page("audio", &t("settings-audio"), "settings-audio");
            idx.add_page("bluetooth", &t("settings-bluetooth"), "settings-bluetooth");
            idx.add_page("wifi", &t("settings-wifi"), "settings-wifi");
            idx.add_page("wired", &t("settings-wired"), "settings-wired");
            idx.add_page("online-accounts", &t("settings-online-accounts"), "settings-online-accounts");
            idx.add_page("weather", &t("settings-weather"), "settings-weather");
            idx.add_page("appearance", &t("settings-appearance"), "settings-appearance");
            idx.add_page("display", &t("settings-display"), "settings-display");
            idx.add_page("wallpaper", &t("settings-wallpaper"), "settings-wallpaper");
            idx.add_page("windows", &t("settings-windows"), "settings-windows");
            idx.add_page("keyboard", &t("settings-keyboard"), "settings-keyboard");
            idx.add_page("notifications", &t("settings-notifications"), "settings-notifications");
            idx.add_page("sounds", &t("settings-sounds"), "settings-sounds");
            idx.add_page("plugins", &t("settings-plugins"), "settings-plugins");
            idx.add_page("services", &t("settings-services"), "settings-services");
            idx.add_page("startup", &t("settings-startup"), "settings-startup");
            idx.add_page("keyboard-shortcuts", &t("settings-keyboard-shortcuts"), "settings-keyboard-shortcuts");
            idx.add_page("scheduled-tasks", &t("settings-scheduled-tasks"), "settings-scheduled-tasks");
        }

        // Helper: wrap a page root in adw::Clamp and upcast to gtk::Widget.
        fn clamped(child: &gtk::Box) -> gtk::Widget {
            adw::Clamp::builder()
                .maximum_size(600)
                .child(child)
                .build()
                .upcast()
        }

        // Entity-based pages are constructed eagerly so their subscribe_type
        // callbacks are registered before the main loop delivers entity data.
        // Pages that only do synchronous file I/O (no entity subscriptions)
        // are deferred to an idle callback to avoid blocking startup.
        let audio_page = AudioPage::new(entity_store, action_callback, &search_index);
        let bluetooth_page = BluetoothPage::new(entity_store, action_callback, &search_index);
        let wifi_page = WiFiPage::new(entity_store, action_callback, &search_index);
        let wired_page = WiredPage::new(entity_store, action_callback, &search_index);
        let weather_page = WeatherPage::new(entity_store, action_callback, &search_index);
        let appearance_page = AppearancePage::new(entity_store, action_callback, &search_index, &navigation_view);
        let display_page = DisplayPage::new(entity_store, action_callback, &search_index);
        let wallpaper_page = WallpaperPage::new(entity_store, action_callback, &search_index);
        let windows_page = NiriWindowsPage::new(entity_store, &search_index);
        let keyboard_page = KeyboardPage::new(entity_store, action_callback, &search_index);
        let notifications_page = NotificationsPage::new(entity_store, action_callback, &search_index);
        let sounds_page = SoundsPage::new(entity_store, action_callback, &search_index);
        let plugins_page = PluginsPage::new(entity_store, &search_index);
        let services_page = ServicesPage::new(entity_store, action_callback, &search_index);
        let scheduler_page = SchedulerPage::new(entity_store, action_callback, &search_index);
        let online_accounts_page = OnlineAccountsPage::new(entity_store, action_callback, &search_index, &navigation_view);

        // Deferred page factories — these pages only read files (KDL config),
        // have no entity subscriptions, and are safe to construct after startup.
        let factories: Rc<RefCell<HashMap<String, PageFactory>>> =
            Rc::new(RefCell::new(HashMap::new()));
        {
            let mut f = factories.borrow_mut();

            let si = search_index.clone();
            f.insert("keyboard-shortcuts".into(), Box::new(move || {
                clamped(&KeyboardShortcutsPage::new(&si).root)
            }));

            let si = search_index.clone();
            f.insert("startup".into(), Box::new(move || {
                clamped(&StartupPage::new(&si).root)
            }));
        }

        // Stack for page switching (keyed by stable page_id)
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vhomogeneous(false)
            .build();
        stack.add_named(&clamped(&audio_page.root), Some("audio"));
        stack.add_named(&clamped(&bluetooth_page.root), Some("bluetooth"));
        stack.add_named(&clamped(&wifi_page.root), Some("wifi"));
        stack.add_named(&clamped(&wired_page.root), Some("wired"));
        stack.add_named(&clamped(&online_accounts_page.root), Some("online-accounts"));
        stack.add_named(&clamped(&weather_page.root), Some("weather"));
        stack.add_named(&clamped(&appearance_page.root), Some("appearance"));
        stack.add_named(&clamped(&display_page.root), Some("display"));
        stack.add_named(&clamped(&wallpaper_page.root), Some("wallpaper"));
        stack.add_named(&clamped(&windows_page.root), Some("windows"));
        stack.add_named(&clamped(&keyboard_page.root), Some("keyboard"));
        stack.add_named(&clamped(&notifications_page.root), Some("notifications"));
        stack.add_named(&clamped(&sounds_page.root), Some("sounds"));
        stack.add_named(&clamped(&plugins_page.root), Some("plugins"));
        stack.add_named(&clamped(&services_page.root), Some("services"));
        stack.add_named(&clamped(&scheduler_page.root), Some("scheduled-tasks"));

        // Deferred pages: construct eagerly if they are the initial page,
        // otherwise build on first navigation (sidebar callback or idle).
        let default_page = initial_page.unwrap_or("bluetooth");
        if stack.child_by_name(default_page).is_none() {
            if let Some(factory) = factories.borrow_mut().remove(default_page) {
                let widget = factory();
                stack.add_named(&widget, Some(default_page));
            } else if initial_page.is_some() {
                log::warn!(
                    "[settings] Requested page '{}' not found, falling back to bluetooth",
                    default_page
                );
            }
        }
        stack.set_visible_child_name(
            if stack.child_by_name(default_page).is_some() { default_page } else { "bluetooth" }
        );

        let content_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();

        content_scrolled.set_child(Some(&stack));

        // Wrap the stack content in a NavigationView to support sub-page drill-down.
        // The stack becomes the root navigation page; sub-pages are pushed on top.
        // The HeaderBar is inside the root NavigationPage so it reacts to pushed
        // sub-pages (back button, title changes) automatically.
        let initial_title =
            page_title(stack.visible_child_name().as_deref().unwrap_or("bluetooth"));

        let root_header = adw::HeaderBar::new();
        let root_toolbar = adw::ToolbarView::new();
        root_toolbar.add_top_bar(&root_header);
        root_toolbar.set_content(Some(&content_scrolled));

        let root_nav_page = adw::NavigationPage::builder()
            .title(&initial_title)
            .child(&root_toolbar)
            .build();

        navigation_view.add(&root_nav_page);

        let content_page = adw::NavigationPage::builder()
            .title(initial_title)
            .child(&navigation_view)
            .build();

        split_view.set_content(Some(&content_page));

        // -- Connect sidebar selection --
        let stack_ref = stack.clone();
        let content_scrolled_ref = content_scrolled.clone();
        let nav_view_ref = navigation_view.clone();
        let root_nav_page_ref = root_nav_page.clone();
        let current_page: Rc<RefCell<String>> = Rc::new(RefCell::new(
            stack.visible_child_name().map(|s| s.to_string()).unwrap_or_else(|| "bluetooth".to_string()),
        ));
        let current_page_ref = current_page.clone();
        let factories_ref = factories.clone();
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

            // Pop any sub-pages back to root when switching sidebar pages
            nav_view_ref.pop_to_page(&root_nav_page_ref);

            // Reset display page when leaving it
            let prev = current_page_ref.borrow().clone();
            if prev == "display" && new_page_id != "display" {
                display_page.reset();
            }

            // Construct the page from its factory on first navigation
            if stack_ref.child_by_name(&new_page_id).is_none()
                && let Some(factory) = factories_ref.borrow_mut().remove(&new_page_id)
            {
                let widget = factory();
                stack_ref.add_named(&widget, Some(&new_page_id));
            }

            content_page.set_title(&new_title);
            root_nav_page_ref.set_title(&new_title);
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
        let stack_for_idle = stack.clone();
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

        // Construct deferred pages (file-I/O-only, no entity subscriptions) in
        // the next idle cycle so their search entries get registered without
        // blocking startup.
        {
            let factories_idle = factories;
            let stack_idle = stack_for_idle;
            gtk::glib::idle_add_local_once(move || {
                for (page_id, factory) in factories_idle.borrow_mut().drain() {
                    let widget = factory();
                    stack_idle.add_named(&widget, Some(&page_id));
                }
            });
        }

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

