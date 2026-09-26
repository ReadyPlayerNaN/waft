//! AI provider settings page.
//!
//! Provider credentials remain owned by each provider's CLI or environment.
//! This page controls whether Waft fetches quota data for each provider.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::ai::{CONFIG_ENTITY_TYPE, ProviderConfig};

use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::subscription::subscribe_entities;

struct ProviderRow {
    row: adw::SwitchRow,
    updating: Rc<Cell<bool>>,
}

struct ProvidersPageState {
    rows: HashMap<String, ProviderRow>,
    sorted_providers: Vec<String>,
    group: adw::PreferencesGroup,
}

/// Settings page for enabling and disabling provider quota collection.
pub struct ProvidersPage {
    pub root: gtk::Box,
}

impl ProvidersPage {
    pub fn register_search(index: &mut SearchIndex) {
        let page_title = t("settings-providers");
        index.add_section_deferred(
            "providers",
            &page_title,
            &t("providers-title"),
            "providers-title",
        );
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<std::cell::RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();
        let group = adw::PreferencesGroup::builder()
            .title(t("providers-title"))
            .description(t("providers-description"))
            .build();
        root.append(&group);

        {
            let mut index = search_index.borrow_mut();
            index.backfill_widget("providers", &t("providers-title"), None, Some(&group));
        }

        let state = Rc::new(std::cell::RefCell::new(ProvidersPageState {
            rows: HashMap::new(),
            sorted_providers: Vec::new(),
            group,
        }));
        let callback = action_callback.clone();
        let search_index = search_index.clone();
        subscribe_entities::<ProviderConfig, _>(entity_store, CONFIG_ENTITY_TYPE, {
            let state = state.clone();
            move |providers| {
                reconcile(&state, &providers, &callback, &search_index);
            }
        });

        Self { root }
    }
}

fn reconcile(
    state: &Rc<std::cell::RefCell<ProvidersPageState>>,
    providers: &[(Urn, ProviderConfig)],
    action_callback: &EntityActionCallback,
    search_index: &Rc<std::cell::RefCell<SearchIndex>>,
) {
    let mut providers = providers.to_vec();
    providers.sort_by(|(_, left), (_, right)| left.display_name.cmp(&right.display_name));

    let mut state = state.borrow_mut();
    let sorted_providers: Vec<String> = providers
        .iter()
        .map(|(_, provider)| provider.provider.clone())
        .collect();

    for (urn, provider) in &providers {
        let row_state = state
            .rows
            .entry(provider.provider.clone())
            .or_insert_with(|| {
                let row = adw::SwitchRow::builder()
                    .title(&provider.display_name)
                    .build();
                let updating = Rc::new(Cell::new(false));
                let updating_ref = updating.clone();
                let callback = action_callback.clone();
                let urn = urn.clone();
                row.connect_active_notify(move |row| {
                    if updating_ref.get() {
                        return;
                    }
                    let _ = callback(
                        urn.clone(),
                        "set-enabled".to_string(),
                        serde_json::json!({ "enabled": row.is_active() }),
                    );
                });
                ProviderRow { row, updating }
            });

        row_state.updating.set(true);
        row_state.row.set_active(provider.enabled);
        row_state.updating.set(false);
        row_state.row.set_title(&provider.display_name);
        let subtitle = if provider.configured {
            t("providers-credentials-found")
        } else {
            t("providers-credentials-missing")
        };
        row_state.row.set_subtitle(&subtitle);
    }

    let stale: Vec<String> = state
        .rows
        .keys()
        .filter(|provider| !sorted_providers.contains(provider))
        .cloned()
        .collect();
    for provider in stale {
        if let Some(row) = state.rows.remove(&provider) {
            state.group.remove(&row.row);
        }
    }

    for provider in &sorted_providers {
        if let Some(row) = state.rows.get(provider)
            && row.row.parent().is_none()
        {
            state.group.add(&row.row);
        }
    }
    state.sorted_providers = sorted_providers;

    let page_title = t("settings-providers");
    let section_title = t("providers-title");
    let mut index = search_index.borrow_mut();
    index.remove_entries("providers", &section_title);
    index.add_section(
        "providers",
        &page_title,
        &section_title,
        "providers-title",
        &state.group,
    );
    for provider in &state.sorted_providers {
        if let Some(row) = state.rows.get(provider) {
            index.add_input(
                "providers",
                &page_title,
                &section_title,
                provider,
                provider,
                &row.row,
            );
        }
    }
}
