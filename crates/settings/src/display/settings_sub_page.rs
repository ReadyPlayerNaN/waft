//! Generic settings sub-page wrapper.
//!
//! Wraps arbitrary content in a scrollable `adw::NavigationPage` for
//! push/pop navigation. No HeaderBar or ToolbarView -- the parent
//! NavigationView manages the header bar automatically.

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

        let root = adw::NavigationPage::builder()
            .title(title)
            .child(&scrolled)
            .build();

        Self { root }
    }
}
