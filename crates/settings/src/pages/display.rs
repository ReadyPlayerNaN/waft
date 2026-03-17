//! Display settings page -- thin composer.
//!
//! Composes brightness and output sections into a single scrollable page.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::brightness_section::BrightnessSection;
use crate::display::output_section::OutputSection;
use crate::search_index::SearchIndex;

/// Display settings page composed of independent sections.
pub struct DisplayPage {
    pub root: gtk::Box,
    output_section: OutputSection,
}

impl DisplayPage {
    /// Phase 1: Register static search entries without constructing widgets.
    ///
    /// Display page itself has no static entries -- brightness and output
    /// sections are dynamic (entity-driven) and register entries themselves.
    pub fn register_search(_idx: &mut SearchIndex) {
        // No static entries; brightness_section and output_section
        // register dynamic entries during reconciliation.
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let brightness = BrightnessSection::new(entity_store, action_callback, search_index);
        root.append(&brightness.root);

        let output_section = OutputSection::new(entity_store, action_callback, search_index);
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
