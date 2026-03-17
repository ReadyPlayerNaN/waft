//! Dark mode settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `dark-mode` entity type.
//! Provides a single toggle switch.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_ui_gtk::icons::icon::IconWidget;

use crate::i18n::t;
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::display::{DARK_MODE_ENTITY_TYPE, DarkMode};

/// Smart container for dark mode settings.
pub struct DarkModeSection {
    pub root: adw::PreferencesGroup,
}

impl DarkModeSection {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-appearance");
        let section_title = t("display-appearance");
        idx.add_section_deferred("appearance", &page_title, &section_title, "display-appearance");
        idx.add_input_deferred("appearance", &page_title, &section_title, &t("display-dark-mode"), "display-dark-mode");
        idx.add_input_deferred("appearance", &page_title, &section_title, &t("display-dark-mode-settings"), "display-dark-mode-settings");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
        on_navigate: Option<Box<dyn Fn()>>,
    ) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("display-appearance"))
            .visible(false)
            .build();

        let toggle_row = adw::SwitchRow::builder().title(t("display-dark-mode")).build();
        group.add(&toggle_row);

        // Navigation link row (only when on_navigate callback is provided)
        let nav_row = if let Some(navigate_fn) = on_navigate {
            let row = adw::ActionRow::builder()
                .title(t("display-dark-mode-settings"))
                .activatable(true)
                .build();
            let chevron = IconWidget::from_name("go-next-symbolic", 16);
            row.add_suffix(chevron.widget());
            group.add(&row);

            let navigate = Rc::new(navigate_fn);
            row.connect_activated(move |_| {
                navigate();
            });
            Some(row)
        } else {
            None
        };

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

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            let section = t("display-appearance");
            idx.backfill_widget("appearance", &section, None, Some(&group));
            idx.backfill_widget("appearance", &section, Some(&t("display-dark-mode")), Some(&toggle_row));
            if let Some(ref row) = nav_row {
                idx.backfill_widget("appearance", &section, Some(&t("display-dark-mode-settings")), Some(row));
            }
        }

        // Subscribe to dark-mode entities
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let toggle_ref = toggle_row;
            let urn_ref = current_urn;
            let guard = updating;

            entity_store.subscribe_type(DARK_MODE_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, DarkMode)> =
                    store.get_entities_typed(DARK_MODE_ENTITY_TYPE);

                if let Some((urn, dm)) = entities.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    toggle_ref.set_active(dm.active);
                    guard.set(false);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        Self { root: group }
    }
}
