//! Active profile selection section -- smart container.
//!
//! Subscribes to `active-profile` and `notification-profile` entity types.
//! Shows a combo row to select the active notification filtering profile.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use crate::i18n::t;
use crate::search_index::SearchIndex;
use waft_protocol::entity::notification_filter::{
    ACTIVE_PROFILE_ENTITY_TYPE, ActiveProfile, NOTIFICATION_PROFILE_ENTITY_TYPE,
    NotificationProfile,
};

/// Smart container for active profile selection.
pub struct ActiveProfileSection {
    pub root: adw::PreferencesGroup,
}

impl ActiveProfileSection {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-notifications");
        let section_title = t("notif-active-profile");
        idx.add_section_deferred("notifications", &page_title, &section_title, "notif-active-profile");
        idx.add_input_deferred("notifications", &page_title, &section_title, &t("notif-profile"), "notif-profile");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("notif-active-profile"))
            .visible(false)
            .build();

        let string_list = gtk::StringList::new(&[]);
        let combo_row = adw::ComboRow::builder()
            .title(t("notif-profile"))
            .model(&string_list)
            .build();
        group.add(&combo_row);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-notifications");
            let section_title = t("notif-active-profile");
            idx.add_section("notifications", &page_title, &section_title, "notif-active-profile", &group);
            idx.add_input("notifications", &page_title, &section_title, &t("notif-profile"), "notif-profile", &combo_row);
        }

        let updating = Rc::new(Cell::new(false));
        let profile_ids: Rc<RefCell<Vec<String>>> =
            Rc::new(RefCell::new(Vec::new()));

        // Wire combo selection -> action
        {
            let cb = action_callback.clone();
            let guard = updating.clone();
            let ids = profile_ids.clone();
            combo_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                let idx = row.selected() as usize;
                let ids_ref = ids.borrow();
                if let Some(profile_id) = ids_ref.get(idx) {
                    let urn = Urn::new("notifications", "active-profile", "current");
                    cb(
                        urn,
                        "set-profile".to_string(),
                        serde_json::json!({ "profile_id": profile_id }),
                    );
                }
            });
        }

        // Reconcile from both entity types
        let reconcile = {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let combo_ref = combo_row;
            let guard = updating.clone();
            let ids = profile_ids;

            Rc::new(move || {
                let profiles: Vec<(Urn, NotificationProfile)> =
                    store.get_entities_typed(NOTIFICATION_PROFILE_ENTITY_TYPE);
                let active: Vec<(Urn, ActiveProfile)> =
                    store.get_entities_typed(ACTIVE_PROFILE_ENTITY_TYPE);

                if profiles.is_empty() {
                    group_ref.set_visible(false);
                    return;
                }

                guard.set(true);

                let mut sorted_profiles: Vec<_> =
                    profiles.iter().map(|(_, p)| p).collect();
                sorted_profiles.sort_by(|a, b| a.name.cmp(&b.name));

                let string_list = gtk::StringList::new(&[]);
                let mut new_ids = Vec::new();
                for profile in &sorted_profiles {
                    string_list.append(&profile.name);
                    new_ids.push(profile.id.clone());
                }
                combo_ref.set_model(Some(&string_list));

                let active_id = active.first().map(|(_, a)| a.profile_id.as_str());
                let selected = active_id
                    .and_then(|id| new_ids.iter().position(|pid| pid == id))
                    .unwrap_or(0);
                combo_ref.set_selected(selected as u32);

                *ids.borrow_mut() = new_ids;
                group_ref.set_visible(true);

                guard.set(false);
            })
        };

        // Subscribe to both entity types
        {
            let r = reconcile.clone();
            entity_store.subscribe_type(NOTIFICATION_PROFILE_ENTITY_TYPE, move || r());
        }
        {
            let r = reconcile.clone();
            entity_store.subscribe_type(ACTIVE_PROFILE_ENTITY_TYPE, move || r());
        }

        // Initial reconciliation
        {
            let r = reconcile;
            gtk::glib::idle_add_local_once(move || r());
        }

        Self { root: group }
    }
}
