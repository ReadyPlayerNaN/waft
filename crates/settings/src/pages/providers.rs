//! AI provider settings page.
//!
//! Provider credentials remain owned by each provider's CLI or environment.
//! This page controls quota collection and the presentation mode.

use std::cell::{Cell, RefCell};
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
    row: adw::ExpanderRow,
    toggle: gtk::Switch,
    updating: Rc<Cell<bool>>,
}

struct ProvidersPageState {
    rows: HashMap<String, ProviderRow>,
    sorted_providers: Vec<String>,
    group: adw::PreferencesGroup,
    display_row: adw::SwitchRow,
    display_updating: Rc<Cell<bool>>,
    display_urn: Rc<RefCell<Option<Urn>>>,
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
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();
        let group = adw::PreferencesGroup::builder()
            .title(t("providers-title"))
            .description(t("providers-description"))
            .build();
        let display_row = adw::SwitchRow::builder()
            .title(t("providers-display-usage"))
            .subtitle(t("providers-display-mode-description"))
            .build();
        group.add(&display_row);
        root.append(&group);

        {
            let mut index = search_index.borrow_mut();
            index.backfill_widget("providers", &t("providers-title"), None, Some(&group));
        }

        let display_updating = Rc::new(Cell::new(false));
        let display_urn = Rc::new(RefCell::new(None));
        {
            let updating = display_updating.clone();
            let target = display_urn.clone();
            let callback = action_callback.clone();
            display_row.connect_active_notify(move |row| {
                if updating.get() {
                    return;
                }
                let Some(urn) = target.borrow().clone() else {
                    return;
                };
                let _ = callback(
                    urn,
                    "set-display-mode".to_string(),
                    serde_json::json!({ "display_usage": row.is_active() }),
                );
            });
        }

        let state = Rc::new(RefCell::new(ProvidersPageState {
            rows: HashMap::new(),
            sorted_providers: Vec::new(),
            group,
            display_row,
            display_updating,
            display_urn,
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
    state: &Rc<RefCell<ProvidersPageState>>,
    providers: &[(Urn, ProviderConfig)],
    action_callback: &EntityActionCallback,
    search_index: &Rc<RefCell<SearchIndex>>,
) {
    let mut providers = providers.to_vec();
    providers.sort_by(|(_, left), (_, right)| left.display_name.cmp(&right.display_name));

    let mut state = state.borrow_mut();
    if let Some((urn, first)) = providers.first() {
        *state.display_urn.borrow_mut() = Some(urn.clone());
        state.display_updating.set(true);
        state.display_row.set_active(first.display_usage);
        let display_title = if first.display_usage {
            t("providers-display-usage")
        } else {
            t("providers-display-leftover")
        };
        state.display_row.set_title(&display_title);
        state.display_row.set_sensitive(true);
        state.display_updating.set(false);
    } else {
        *state.display_urn.borrow_mut() = None;
        state.display_updating.set(true);
        state.display_row.set_active(true);
        state.display_row.set_title(&t("providers-display-usage"));
        state.display_row.set_sensitive(false);
        state.display_updating.set(false);
    }

    let sorted_providers: Vec<String> = providers
        .iter()
        .map(|(_, provider)| provider.provider.clone())
        .collect();

    for (urn, provider) in &providers {
        let row_state = state
            .rows
            .entry(provider.provider.clone())
            .or_insert_with(|| {
                let row = adw::ExpanderRow::builder()
                    .title(&provider.display_name)
                    .build();
                let toggle = gtk::Switch::builder().valign(gtk::Align::Center).build();
                row.add_suffix(&toggle);

                let help = gtk::Label::builder()
                    .label(provider_credentials_help(&provider.provider))
                    .xalign(0.0)
                    .wrap(true)
                    .selectable(true)
                    .margin_start(12)
                    .margin_end(12)
                    .margin_top(12)
                    .margin_bottom(12)
                    .build();
                let help_row = gtk::ListBoxRow::builder().child(&help).build();
                row.add_row(&help_row);

                let updating = Rc::new(Cell::new(false));
                let updating_ref = updating.clone();
                let callback = action_callback.clone();
                let urn = urn.clone();
                toggle.connect_active_notify(move |toggle| {
                    if updating_ref.get() {
                        return;
                    }
                    let _ = callback(
                        urn.clone(),
                        "set-enabled".to_string(),
                        serde_json::json!({ "enabled": toggle.is_active() }),
                    );
                });
                ProviderRow {
                    row,
                    toggle,
                    updating,
                }
            });

        let available = provider.configured;
        row_state.updating.set(true);
        row_state.toggle.set_sensitive(available);
        row_state.toggle.set_active(available && provider.enabled);
        row_state.updating.set(false);
        row_state.row.set_title(&provider.display_name);
        let availability = if available {
            t("providers-available")
        } else {
            t("providers-unavailable")
        };
        row_state.row.set_subtitle(&availability);
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

fn provider_credentials_help(slug: &str) -> String {
    let details_key = match slug {
        "claude" => "providers-help-claude",
        "codex" => "providers-help-codex",
        "cursor" => "providers-help-cursor",
        "deepseek" => "providers-help-deepseek",
        "antigravity" => "providers-help-antigravity",
        "github-copilot" => "providers-help-github-copilot",
        "grok" => "providers-help-grok",
        "kimi" => "providers-help-kimi",
        "minimax" => "providers-help-minimax",
        "openrouter" => "providers-help-openrouter",
        "siliconflow" => "providers-help-siliconflow",
        "zai" => "providers-help-zai",
        "mimo" => "providers-help-mimo",
        _ => "providers-help-generic",
    };
    format!(
        "{}\n\n{}",
        t(details_key),
        t("providers-credentials-dialog-footer")
    )
}
