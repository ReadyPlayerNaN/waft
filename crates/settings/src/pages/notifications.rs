//! Notification settings page -- thin composer.
//!
//! Composes independent smart containers: DnD toggle, sound defaults,
//! active profile selection, notification groups, and profiles sections
//! into a single scrollable page.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::notifications::active_profile_section::ActiveProfileSection;
use crate::notifications::dnd_section::DndSection;
use crate::notifications::groups_section::GroupsSection;
use crate::notifications::profiles_section::ProfilesSection;
use crate::notifications::recording_section::RecordingSection;
use crate::search_index::SearchIndex;
use crate::sounds::defaults_section::DefaultsSection;

/// Notification settings page composed of independent sections.
pub struct NotificationsPage {
    pub root: gtk::Box,
}

impl NotificationsPage {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        DndSection::register_search(idx);
        DefaultsSection::register_search(idx);
        ActiveProfileSection::register_search(idx);
        GroupsSection::register_search(idx);
        ProfilesSection::register_search(idx);
        RecordingSection::register_search(idx);
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let dnd = DndSection::new(entity_store, action_callback, search_index);
        root.append(&dnd.root);

        let defaults = DefaultsSection::new(entity_store, action_callback, search_index);
        root.append(&defaults.root);

        let active_profile = ActiveProfileSection::new(entity_store, action_callback, search_index);
        root.append(&active_profile.root);

        let groups = GroupsSection::new(entity_store, action_callback, search_index);
        root.append(&groups.root);

        let profiles = ProfilesSection::new(entity_store, action_callback, search_index);
        root.append(&profiles.root);

        let recording = RecordingSection::new(entity_store, action_callback, search_index);
        root.append(&recording.root);

        Self { root }
    }
}
