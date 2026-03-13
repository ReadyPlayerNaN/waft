//! Generic settings sub-page wrapper.
//!
//! Wraps arbitrary content in a scrollable `adw::NavigationPage` with its own
//! `AdwHeaderBar`. When pushed onto a `NavigationView`, the header bar
//! automatically shows a back button and the page title.

use adw::prelude::*;

/// A reusable sub-page wrapper for settings navigation.
pub struct SettingsSubPage {
    pub root: adw::NavigationPage,
}

impl SettingsSubPage {
    pub fn new(title: &str, content: &impl IsA<gtk::Widget>) -> Self {
        let clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(content)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
            .build();

        let header = adw::HeaderBar::new();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scrolled));

        let root = adw::NavigationPage::builder()
            .title(title)
            .child(&toolbar)
            .build();

        Self { root }
    }
}
