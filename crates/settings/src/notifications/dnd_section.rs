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

/// Smart container for Do Not Disturb settings.
pub struct DndSection {
    pub root: adw::PreferencesGroup,
}

impl DndSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("notif-dnd"))
            .visible(false)
            .build();

        let toggle_row = adw::SwitchRow::builder().title("Do Not Disturb").build();
        group.add(&toggle_row);

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

            entity_store.subscribe_type(DND_ENTITY_TYPE, move || {
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
            });
        }

        Self { root: group }
    }
}
