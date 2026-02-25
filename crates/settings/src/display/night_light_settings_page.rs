//! Night light settings sub-page -- thin composer.
//!
//! Wraps `NightLightConfigSection` in a scrollable `adw::NavigationPage`
//! for push/pop sub-page navigation from the Appearance page.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::night_light_config_section::NightLightConfigSection;
use crate::i18n::t;
use crate::search_index::SearchIndex;

/// Sub-page wrapping the night light configuration section.
pub struct NightLightSettingsPage {
    pub page: adw::NavigationPage,
}

impl NightLightSettingsPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let config = NightLightConfigSection::new(entity_store, action_callback, search_index);
        content.append(&config.root);

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
            .title(t("display-night-light-settings"))
            .child(&toolbar)
            .build();

        Self { page }
    }
}
