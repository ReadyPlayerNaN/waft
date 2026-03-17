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

        // Phase 1: Register section/input-level search entries (strings only, no widgets).
        {
            let mut idx = search_index.borrow_mut();
            AudioPage::register_search(&mut idx);
            BluetoothPage::register_search(&mut idx);
            WiFiPage::register_search(&mut idx);
            WiredPage::register_search(&mut idx);
            WeatherPage::register_search(&mut idx);
            AppearancePage::register_search(&mut idx);
            DisplayPage::register_search(&mut idx);
            WallpaperPage::register_search(&mut idx);
            NiriWindowsPage::register_search(&mut idx);
            KeyboardPage::register_search(&mut idx);
            NotificationsPage::register_search(&mut idx);
            SoundsPage::register_search(&mut idx);
            PluginsPage::register_search(&mut idx);
            ServicesPage::register_search(&mut idx);
            SchedulerPage::register_search(&mut idx);
            OnlineAccountsPage::register_search(&mut idx);
            KeyboardShortcutsPage::register_search(&mut idx);
            StartupPage::register_search(&mut idx);
        }

        // Helper: wrap a page root in adw::Clamp and upcast to gtk::Widget.
        fn clamped(child: &gtk::Box) -> gtk::Widget {
            adw::Clamp::builder()
                .maximum_size(600)
                .child(child)
                .build()
                .upcast()
        }

        // Phase 2: Create page factories — widgets constructed on first navigation.
        let display_page_ref: Rc<RefCell<Option<DisplayPage>>> = Rc::new(RefCell::new(None));
        let factories: Rc<RefCell<HashMap<String, PageFactory>>> =
            Rc::new(RefCell::new(HashMap::new()));
        {
            let mut f = factories.borrow_mut();

            // For each page, clone the needed captures and create a factory closure.
            // Pages that take (entity_store, action_callback, search_index):
            macro_rules! entity_page_factory {
                ($f:expr, $id:expr, $Page:ident) => {{
                    let es = entity_store.clone();
                    let ac = action_callback.clone();
                    let si = search_index.clone();
                    $f.insert($id.into(), Box::new(move || {
                        clamped(&$Page::new(&es, &ac, &si).root)
                    }));
                }};
            }

            entity_page_factory!(f, "audio", AudioPage);
            entity_page_factory!(f, "bluetooth", BluetoothPage);
            entity_page_factory!(f, "wifi", WiFiPage);
            entity_page_factory!(f, "wired", WiredPage);
            entity_page_factory!(f, "weather", WeatherPage);
            entity_page_factory!(f, "wallpaper", WallpaperPage);
            entity_page_factory!(f, "keyboard", KeyboardPage);
            entity_page_factory!(f, "notifications", NotificationsPage);
            entity_page_factory!(f, "sounds", SoundsPage);
            entity_page_factory!(f, "services", ServicesPage);
            entity_page_factory!(f, "scheduled-tasks", SchedulerPage);

            // Pages with only (entity_store, search_index):
            {
                let es = entity_store.clone();
                let si = search_index.clone();
                f.insert("windows".into(), Box::new(move || {
                    clamped(&NiriWindowsPage::new(&es, &si).root)
                }));
            }
            {
                let es = entity_store.clone();
                let si = search_index.clone();
                f.insert("plugins".into(), Box::new(move || {
                    clamped(&PluginsPage::new(&es, &si).root)
                }));
            }

            // Pages that also need navigation_view:
            {
                let es = entity_store.clone();
                let ac = action_callback.clone();
                let si = search_index.clone();
                let nv = navigation_view.clone();
                f.insert("appearance".into(), Box::new(move || {
                    clamped(&AppearancePage::new(&es, &ac, &si, &nv).root)
                }));
            }
            {
                let es = entity_store.clone();
                let ac = action_callback.clone();
                let si = search_index.clone();
                let nv = navigation_view.clone();
                f.insert("online-accounts".into(), Box::new(move || {
                    clamped(&OnlineAccountsPage::new(&es, &ac, &si, &nv).root)
                }));
            }

            // Display page — stored in Rc<RefCell> for reset() access:
            {
                let es = entity_store.clone();
                let ac = action_callback.clone();
                let si = search_index.clone();
                let dp_ref = display_page_ref.clone();
                f.insert("display".into(), Box::new(move || {
                    let page = DisplayPage::new(&es, &ac, &si);
                    let widget = clamped(&page.root);
                    *dp_ref.borrow_mut() = Some(page);
                    widget
                }));
            }

            // File-I/O-only pages:
            {
                let si = search_index.clone();
                f.insert("keyboard-shortcuts".into(), Box::new(move || {
                    clamped(&KeyboardShortcutsPage::new(&si).root)
                }));
            }
            {
                let si = search_index.clone();
                f.insert("startup".into(), Box::new(move || {
                    clamped(&StartupPage::new(&si).root)
                }));
            }
        }

        // Stack for page switching (keyed by stable page_id).
        // Starts empty — pages are constructed on first navigation via factories.
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vhomogeneous(false)
            .build();

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
                // Construct bluetooth as fallback
                if let Some(factory) = factories.borrow_mut().remove("bluetooth") {
                    let widget = factory();
                    stack.add_named(&widget, Some("bluetooth"));
                }
            }
        }
        let default_page = if stack.child_by_name(default_page).is_some() {
            default_page
        } else {
            "bluetooth"
        };
        stack.set_visible_child_name(default_page);

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
        let display_page_for_cb = display_page_ref.clone();
        let search_index_ref = search_index.clone();
        sidebar.connect_output(move |output| {
            let (new_page_id, new_title) = match output {
                SidebarOutput::Selected { page_id, title } => {
                    log::debug!("[settings] sidebar selected: {page_id} ({title})");
                    (page_id, title)
                }
                SidebarOutput::SearchSelected {
                    page_id,
                    page_title,
                    section_title,
                    input_title,
                } => {
                    log::debug!("[settings] search selected: {page_id}");

                    // Construct the page if needed (before widget lookup)
                    if stack_ref.child_by_name(&page_id).is_none()
                        && let Some(factory) = factories_ref.borrow_mut().remove(&page_id)
                    {
                        let widget = factory();
                        stack_ref.add_named(&widget, Some(&page_id));
                    }

                    // Look up target widget AFTER page construction (backfill has run)
                    if let Some(ref section) = section_title {
                        let target = search_index_ref.borrow().find_widget(
                            &page_id,
                            section,
                            input_title.as_deref(),
                        );
                        if let Some(weak) = target {
                            let scrolled = content_scrolled_ref.clone();
                            gtk::glib::idle_add_local_once(move || {
                                if let Some(widget) = weak.upgrade() {
                                    scroll_to_and_highlight(&scrolled, &widget);
                                }
                            });
                        }
                    }

                    (page_id, page_title)
                }
            };

            // Pop any sub-pages back to root when switching sidebar pages
            nav_view_ref.pop_to_page(&root_nav_page_ref);

            // Reset display page when leaving it
            let prev = current_page_ref.borrow().clone();
            if prev == "display" && new_page_id != "display" {
                if let Some(ref dp) = *display_page_for_cb.borrow() {
                    dp.reset();
                }
            }

            // Construct the page from its factory on first navigation (no-op if already built by SearchSelected)
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

