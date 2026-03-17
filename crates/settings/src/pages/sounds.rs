//! Sounds settings page -- thin composer.
//!
//! Contains the sound gallery for managing custom notification sound files.
//! Per-urgency sound defaults live on the Notifications page alongside DnD
//! and filtering rules, since they control notification behaviour.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::search_index::SearchIndex;
use crate::sounds::gallery_section::GallerySection;

/// Sounds settings page containing the sound file gallery.
pub struct SoundsPage {
    pub root: gtk::Box,
}

impl SoundsPage {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        GallerySection::register_search(idx);
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let gallery = GallerySection::new(entity_store, action_callback, search_index);
        root.append(&gallery.root);

        Self { root }
    }
}
