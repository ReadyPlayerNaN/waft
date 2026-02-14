//! Display settings page -- thin composer.
//!
//! Composes three independent smart containers: brightness, dark mode,
//! and night light sections into a single scrollable page.

use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::brightness_section::BrightnessSection;
use crate::display::dark_mode_section::DarkModeSection;
use crate::display::night_light_section::NightLightSection;

/// Display settings page composed of independent sections.
pub struct DisplayPage {
    pub root: gtk::Box,
}

impl DisplayPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let brightness = BrightnessSection::new(entity_store, action_callback);
        root.append(&brightness.root);

        let dark_mode = DarkModeSection::new(entity_store, action_callback);
        root.append(&dark_mode.root);

        let night_light = NightLightSection::new(entity_store, action_callback);
        root.append(&night_light.root);

        Self { root }
    }
}
