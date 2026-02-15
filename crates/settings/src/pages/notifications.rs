//! Notification filtering settings page -- thin composer.
//!
//! Composes three independent smart containers: active profile selection,
//! notification groups, and profiles sections into a single scrollable page.

use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::notifications::active_profile_section::ActiveProfileSection;
use crate::notifications::groups_section::GroupsSection;
use crate::notifications::profiles_section::ProfilesSection;

/// Notification filtering settings page composed of independent sections.
pub struct NotificationsPage {
    pub root: gtk::Box,
}

impl NotificationsPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let active_profile = ActiveProfileSection::new(entity_store, action_callback);
        root.append(&active_profile.root);

        let groups = GroupsSection::new(entity_store, action_callback);
        root.append(&groups.root);

        let profiles = ProfilesSection::new(entity_store, action_callback);
        root.append(&profiles.root);

        Self { root }
    }
}
