//! Main settings window with AdwNavigationSplitView layout.
//!
//! Contains a sidebar for category navigation and a content area
//! that displays the selected settings page.

use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::pages::bluetooth::BluetoothPage;
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

        // -- Content: Bluetooth page (default) --
        let bluetooth_page = BluetoothPage::new(entity_store, action_callback);
        let content_header = adw::HeaderBar::new();
        let content_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        let content_clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&bluetooth_page.root)
            .build();

        content_scrolled.set_child(Some(&content_clamp));

        let content_toolbar = adw::ToolbarView::new();
        content_toolbar.add_top_bar(&content_header);
        content_toolbar.set_content(Some(&content_scrolled));

        let content_page = adw::NavigationPage::builder()
            .title("Bluetooth")
            .child(&content_toolbar)
            .build();

        split_view.set_content(Some(&content_page));

        // -- Connect sidebar selection --
        sidebar.connect_output(move |output| {
            match output {
                SidebarOutput::Selected(category) => {
                    log::debug!("[settings] sidebar selected: {category}");
                    // For now, only Bluetooth is implemented
                    content_page.set_title(&category);
                }
            }
        });

        // -- Window --
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Waft Settings")
            .default_width(900)
            .default_height(600)
            .content(&split_view)
            .build();

        Self { window }
    }
}
