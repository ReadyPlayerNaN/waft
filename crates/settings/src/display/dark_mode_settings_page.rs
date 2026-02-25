//! Dark mode settings sub-page -- thin composer.
//!
//! Wraps `DarkModeAutomationSection` in a scrollable `adw::NavigationPage`
//! for push/pop sub-page navigation from the Appearance page.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::dark_mode_automation_section::DarkModeAutomationSection;
use crate::i18n::t;
use crate::search_index::SearchIndex;

/// Sub-page wrapping the dark mode automation config section.
pub struct DarkModeSettingsPage {
    pub page: adw::NavigationPage,
}

impl DarkModeSettingsPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let automation =
            DarkModeAutomationSection::new(entity_store, action_callback, search_index);
        content.append(&automation.root);

        let clamp = adw::Clamp::builder()
            .maximum_size(600)
            .child(&content)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build();

        let header = adw::HeaderBar::new();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scrolled));

        let page = adw::NavigationPage::builder()
            .title(t("display-dark-mode-settings"))
            .child(&toolbar)
            .build();

        Self { page }
    }
}
