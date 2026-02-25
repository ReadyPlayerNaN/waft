//! Appearance settings page -- thin composer.
//!
//! Composes dark mode and night light sections into a single scrollable page.
//! Detailed configuration sections are accessible via sub-page navigation.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::entity::display::{DarkMode, DARK_MODE_ENTITY_TYPE};
use waft_protocol::Urn;
use waft_ui_gtk::icons::icon::IconWidget;

use crate::display::accent_colour_section::AccentColourSection;
use crate::display::dark_mode_section::DarkModeSection;
use crate::display::dark_mode_settings_page::DarkModeSettingsPage;
use crate::display::night_light_section::NightLightSection;
use crate::display::night_light_settings_page::NightLightSettingsPage;
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

        // -- Dark Mode toggle section (unchanged) --
        let dark_mode = DarkModeSection::new(entity_store, action_callback, search_index);
        root.append(&dark_mode.root);

        // -- Dark Mode settings link row (standalone, after DarkModeSection) --
        let dark_mode_link = adw::ActionRow::builder()
            .title(t("display-dark-mode-settings"))
            .activatable(true)
            .visible(false)
            .build();
        let chevron = IconWidget::from_name("go-next-symbolic", 16);
        dark_mode_link.add_suffix(chevron.widget());
        root.append(&dark_mode_link);

        // Create the dark mode settings sub-page
        let dark_mode_settings =
            DarkModeSettingsPage::new(entity_store, action_callback, search_index);

        // Wire dark mode link row to push sub-page
        {
            let nav = navigation_view.clone();
            let page = dark_mode_settings.page.clone();
            dark_mode_link.connect_activated(move |_| {
                nav.push(&page);
            });
        }

        // Show/hide dark mode link row based on dark-mode entity presence
        {
            let store = entity_store.clone();
            let link_ref = dark_mode_link.clone();
            entity_store.subscribe_type(DARK_MODE_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, DarkMode)> =
                    store.get_entities_typed(DARK_MODE_ENTITY_TYPE);
                link_ref.set_visible(!entities.is_empty());
            });
        }

        // -- Night Light toggle section (with navigation callback) --
        let night_light_settings =
            NightLightSettingsPage::new(entity_store, action_callback, search_index);

        let nav_for_nl = navigation_view.clone();
        let nl_page = night_light_settings.page.clone();
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

        // Register dark mode link row in search index
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-appearance");
            idx.add_input(
                "appearance",
                &page_title,
                &t("display-appearance"),
                &t("display-dark-mode-settings"),
                "display-dark-mode-settings",
                &dark_mode_link,
            );
        }

        Self { root }
    }
}
