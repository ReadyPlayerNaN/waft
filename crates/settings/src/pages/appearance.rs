//! Appearance settings page -- thin composer.
//!
//! Composes dark mode and night light sections into a single scrollable page.

use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::dark_mode_automation_section::DarkModeAutomationSection;
use crate::display::dark_mode_section::DarkModeSection;
use crate::display::night_light_config_section::NightLightConfigSection;
use crate::display::night_light_section::NightLightSection;

/// Appearance settings page composed of independent sections.
pub struct AppearancePage {
    pub root: gtk::Box,
}

impl AppearancePage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let dark_mode = DarkModeSection::new(entity_store, action_callback);
        root.append(&dark_mode.root);

        let dark_mode_automation =
            DarkModeAutomationSection::new(entity_store, action_callback);
        root.append(&dark_mode_automation.root);

        let night_light = NightLightSection::new(entity_store, action_callback);
        root.append(&night_light.root);

        let night_light_config =
            NightLightConfigSection::new(entity_store, action_callback);
        root.append(&night_light_config.root);

        Self { root }
    }
}
