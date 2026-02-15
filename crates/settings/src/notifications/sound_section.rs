//! Sound defaults settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `sound-config` entity type.
//! Provides master toggle and per-urgency default sound entries.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::notification_filter::{SOUND_CONFIG_ENTITY_TYPE, SoundConfigEntity};

/// Smart container for notification sound settings.
pub struct SoundSection {
    pub root: adw::PreferencesGroup,
}

impl SoundSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Sounds")
            .visible(false)
            .build();

        let enabled_row = adw::SwitchRow::builder()
            .title("Enable notification sounds")
            .build();
        group.add(&enabled_row);

        let low_row = adw::EntryRow::builder()
            .title("Default sound (low urgency)")
            .show_apply_button(true)
            .build();
        group.add(&low_row);

        let normal_row = adw::EntryRow::builder()
            .title("Default sound (normal urgency)")
            .show_apply_button(true)
            .build();
        group.add(&normal_row);

        let critical_row = adw::EntryRow::builder()
            .title("Default sound (critical urgency)")
            .show_apply_button(true)
            .build();
        group.add(&critical_row);

        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Closure to build and send the update action from current widget state
        let send_update = {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let enabled_ref = enabled_row.clone();
            let low_ref = low_row.clone();
            let normal_ref = normal_row.clone();
            let critical_ref = critical_row.clone();

            Rc::new(move || {
                let Some(ref urn) = *urn_ref.borrow() else {
                    return;
                };
                let entity = SoundConfigEntity {
                    enabled: enabled_ref.is_active(),
                    default_low: low_ref.text().to_string(),
                    default_normal: normal_ref.text().to_string(),
                    default_critical: critical_ref.text().to_string(),
                };
                let params = match serde_json::to_value(&entity) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("[sound-section] failed to serialize: {e}");
                        return;
                    }
                };
                cb(urn.clone(), "update-sound-config".to_string(), params);
            })
        };

        // Wire enabled toggle
        {
            let guard = updating.clone();
            let send = send_update.clone();
            enabled_row.connect_active_notify(move |_row| {
                if guard.get() {
                    return;
                }
                send();
            });
        }

        // Wire entry rows (apply button)
        {
            let guard = updating.clone();
            let send = send_update.clone();
            low_row.connect_apply(move |_| {
                if guard.get() {
                    return;
                }
                send();
            });
        }
        {
            let guard = updating.clone();
            let send = send_update.clone();
            normal_row.connect_apply(move |_| {
                if guard.get() {
                    return;
                }
                send();
            });
        }
        {
            let guard = updating.clone();
            let send = send_update;
            critical_row.connect_apply(move |_| {
                if guard.get() {
                    return;
                }
                send();
            });
        }

        // Subscribe to sound-config entity
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let enabled_ref = enabled_row;
            let low_ref = low_row;
            let normal_ref = normal_row;
            let critical_ref = critical_row;
            let urn_ref = current_urn;
            let guard = updating;

            entity_store.subscribe_type(SOUND_CONFIG_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, SoundConfigEntity)> =
                    store.get_entities_typed(SOUND_CONFIG_ENTITY_TYPE);

                if let Some((urn, cfg)) = entities.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    enabled_ref.set_active(cfg.enabled);
                    low_ref.set_text(&cfg.default_low);
                    normal_ref.set_text(&cfg.default_normal);
                    critical_ref.set_text(&cfg.default_critical);
                    guard.set(false);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        Self { root: group }
    }
}
