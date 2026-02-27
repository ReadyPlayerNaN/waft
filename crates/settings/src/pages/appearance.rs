//! Appearance settings page -- thin composer.
//!
//! Composes dark mode, night light, and accent colour sections into a
//! single scrollable page. Sub-page navigation is handled internally
//! by each section via callbacks.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::accent_colour_section::AccentColourSection;
use crate::display::dark_mode_automation_section::DarkModeAutomationSection;
use crate::display::dark_mode_section::DarkModeSection;
use crate::display::night_light_config_section::NightLightConfigSection;
use crate::display::night_light_section::NightLightSection;
use crate::display::settings_sub_page::SettingsSubPage;
use crate::i18n::t;
use crate::search_index::SearchIndex;

/// Appearance settings page composed of independent sections.
pub struct AppearancePage {
    pub root: gtk::Box,
}

impl AppearancePage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
        navigation_view: &adw::NavigationView,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        // -- Dark Mode sub-page (created before section so callback can capture it) --
        let dark_mode_automation =
            DarkModeAutomationSection::new(entity_store, action_callback, search_index);
        let dark_mode_sub_page =
            SettingsSubPage::new(&t("display-dark-mode-settings"), &dark_mode_automation.root);

        let nav_for_dm = navigation_view.clone();
        let dm_page = dark_mode_sub_page.root.clone();
        let dark_mode = DarkModeSection::new(
            entity_store,
            action_callback,
            search_index,
            Some(Box::new(move || {
                nav_for_dm.push(&dm_page);
            })),
        );
        root.append(&dark_mode.root);

        // -- Night Light sub-page --
        let night_light_config =
            NightLightConfigSection::new(entity_store, action_callback, search_index);
        let night_light_sub_page =
            SettingsSubPage::new(&t("display-night-light-settings"), &night_light_config.root);

        let nav_for_nl = navigation_view.clone();
        let nl_page = night_light_sub_page.root.clone();
        let night_light = NightLightSection::new(
            entity_store,
            action_callback,
            search_index,
            Some(Box::new(move || {
                nav_for_nl.push(&nl_page);
            })),
        );
        root.append(&night_light.root);

        // -- Accent Colour section --
        let accent_colour =
            AccentColourSection::new(entity_store, action_callback, search_index);
        root.append(&accent_colour.root);

        Self { root }
    }
}
