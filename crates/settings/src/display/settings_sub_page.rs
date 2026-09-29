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
        let builder = gtk::Builder::from_resource("/com/waft/settings/settings-sub-page.ui");
        let root: adw::NavigationPage = builder
            .object("root")
            .expect("settings-sub-page.ui must contain root");
        let clamp: adw::Clamp = builder
            .object("clamp")
            .expect("settings-sub-page.ui must contain clamp");
        clamp.set_child(Some(content));
        root.set_title(title);
        Self { root }
    }
}
