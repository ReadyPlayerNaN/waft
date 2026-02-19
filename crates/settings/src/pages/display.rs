//! Display settings page -- thin composer.
//!
//! Composes brightness and output sections into a single scrollable page.

use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::brightness_section::BrightnessSection;
use crate::display::output_section::OutputSection;

/// Display settings page composed of independent sections.
pub struct DisplayPage {
    pub root: gtk::Box,
    output_section: OutputSection,
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

        let output_section = OutputSection::new(entity_store, action_callback);
        root.append(&output_section.root);

        Self {
            root,
            output_section,
        }
    }

    /// Discard pending output changes and re-reconcile from entity store.
    pub fn reset(&self) {
        self.output_section.reset();
    }
}
