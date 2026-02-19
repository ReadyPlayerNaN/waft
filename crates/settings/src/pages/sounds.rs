//! Sounds settings page -- thin composer.
//!
//! Contains the sound gallery for managing custom notification sound files.
//! Per-urgency sound defaults live on the Notifications page alongside DnD
//! and filtering rules, since they control notification behaviour.

use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::sounds::gallery_section::GallerySection;

/// Sounds settings page containing the sound file gallery.
pub struct SoundsPage {
    pub root: gtk::Box,
}

impl SoundsPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let gallery = GallerySection::new(entity_store, action_callback);
        root.append(&gallery.root);

        Self { root }
    }
}
