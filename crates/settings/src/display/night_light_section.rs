//! Night light settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `night-light` entity type.
//! Provides toggle, preset selection, and status display.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::i18n::t;
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::display::{NIGHT_LIGHT_ENTITY_TYPE, NightLight};

/// Smart container for night light settings.
pub struct NightLightSection {
    pub root: adw::PreferencesGroup,
}

impl NightLightSection {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("display-night-light"))
            .visible(false)
            .build();

        let toggle_row = adw::SwitchRow::builder().title(t("display-night-light-toggle")).build();
        group.add(&toggle_row);

        let preset_model = gtk::StringList::new(&[]);
        let preset_row = adw::ComboRow::builder()
            .title(t("display-color-preset"))
            .model(&preset_model)
            .visible(false)
            .build();
        group.add(&preset_row);

        let status_row = adw::ActionRow::builder()
            .title(t("display-status"))
            .visible(false)
            .build();
        group.add(&status_row);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-appearance");
            let section_title = t("display-night-light");
            idx.add_section("appearance", &page_title, &section_title, "display-night-light", &group);
            idx.add_input("appearance", &page_title, &section_title, &t("display-night-light-toggle"), "display-night-light-toggle", &toggle_row);
            idx.add_input("appearance", &page_title, &section_title, &t("display-color-preset"), "display-color-preset", &preset_row);
        }

        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));
        let current_presets: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

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

        // Wire preset selection
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            let presets_ref = current_presets.clone();
            preset_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                let idx = row.selected() as usize;
                let presets = presets_ref.borrow();
                let preset_value = if idx == 0 {
                    "default".to_string()
                } else if let Some(name) = presets.get(idx - 1) {
                    name.clone()
                } else {
                    return;
                };
                if let Some(ref urn) = *urn_ref.borrow() {
                    cb(
                        urn.clone(),
                        "select_preset".to_string(),
                        serde_json::Value::String(preset_value),
                    );
                }
            });
        }

        // Subscribe to night-light entities
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let toggle_ref = toggle_row.clone();
            let preset_row_ref = preset_row.clone();
            let preset_model_ref = preset_model.clone();
            let status_row_ref = status_row.clone();
            let urn_ref = current_urn;
            let presets_ref = current_presets;
            let guard = updating;

            entity_store.subscribe_type(NIGHT_LIGHT_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, NightLight)> =
                    store.get_entities_typed(NIGHT_LIGHT_ENTITY_TYPE);

                if let Some((urn, nl)) = entities.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    toggle_ref.set_active(nl.active);

                    if let Some(ref period) = nl.period {
                        let label = match period.as_str() {
                            "day" => t("display-day"),
                            "night" => t("display-night"),
                            other => other.to_string(),
                        };
                        toggle_ref.set_subtitle(&label);
                    } else {
                        toggle_ref.set_subtitle("");
                    }

                    let has_presets = !nl.presets.is_empty();
                    preset_row_ref.set_visible(nl.active && has_presets);

                    let prev_presets = presets_ref.borrow();
                    if *prev_presets != nl.presets {
                        drop(prev_presets);
                        let count = preset_model_ref.n_items();
                        if count > 0 {
                            preset_model_ref.splice(0, count, &[] as &[&str]);
                        }
                        preset_model_ref.append(&t("display-default"));
                        for preset in &nl.presets {
                            preset_model_ref.append(preset);
                        }
                        *presets_ref.borrow_mut() = nl.presets.clone();
                    }

                    let selected_idx = match &nl.active_preset {
                        Some(name) => {
                            let presets = presets_ref.borrow();
                            presets
                                .iter()
                                .position(|p| p == name)
                                .map(|i| (i + 1) as u32)
                                .unwrap_or(0)
                        }
                        None => 0,
                    };
                    preset_row_ref.set_selected(selected_idx);

                    if nl.active {
                        if let Some(ref next) = nl.next_transition {
                            status_row_ref.set_subtitle(next);
                            status_row_ref.set_title(&t("display-next-transition"));
                            status_row_ref.set_visible(true);
                        } else {
                            status_row_ref.set_visible(false);
                        }
                    } else {
                        status_row_ref.set_visible(false);
                    }

                    guard.set(false);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        Self { root: group }
    }
}
