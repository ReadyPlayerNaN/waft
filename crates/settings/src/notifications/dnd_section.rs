//! Do Not Disturb settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `dnd` entity type.
//! Provides a single toggle switch.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::notification::{DND_ENTITY_TYPE, Dnd};

use crate::i18n::t;
use crate::search_index::SearchIndex;

/// Smart container for Do Not Disturb settings.
pub struct DndSection {
    pub root: adw::PreferencesGroup,
}

impl DndSection {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-notifications");
        let section_title = t("notif-dnd");
        idx.add_section_deferred("notifications", &page_title, &section_title, "notif-dnd");
        idx.add_input_deferred("notifications", &page_title, &section_title, "Do Not Disturb", "notif-dnd");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("notif-dnd"))
            .visible(false)
            .build();

        let toggle_row = adw::SwitchRow::builder().title("Do Not Disturb").build();
        group.add(&toggle_row);

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            let section = t("notif-dnd");
            idx.backfill_widget("notifications", &section, None, Some(&group));
            idx.backfill_widget("notifications", &section, Some("Do Not Disturb"), Some(&toggle_row));
        }

        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Wire toggle
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            toggle_row.connect_active_notify(move |_row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    cb(urn.clone(), "toggle".to_string(), serde_json::Value::Null);
                }
            });
        }

        // Subscribe to dnd entities
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let toggle_ref = toggle_row;
            let urn_ref = current_urn;
            let guard = updating;

            let reconcile: Rc<dyn Fn()> = Rc::new({
                let store = store.clone();
                let group_ref = group_ref.clone();
                let toggle_ref = toggle_ref.clone();
                let urn_ref = urn_ref.clone();
                let guard = guard.clone();
                move || {
                    let entities: Vec<(Urn, Dnd)> = store.get_entities_typed(DND_ENTITY_TYPE);

                    if let Some((urn, dnd)) = entities.first() {
                        guard.set(true);
                        *urn_ref.borrow_mut() = Some(urn.clone());
                        group_ref.set_visible(true);
                        toggle_ref.set_active(dnd.active);
                        guard.set(false);
                    } else {
                        group_ref.set_visible(false);
                    }
                }
            });

            entity_store.subscribe_type(DND_ENTITY_TYPE, {
                let r = reconcile.clone();
                move || r()
            });
            gtk::glib::idle_add_local_once(move || reconcile());
        }

        Self { root: group }
    }
}
