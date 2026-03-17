//! Online Accounts settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `online-account` entity type. On entity
//! changes, reconciles the list of account rows showing GOA accounts. Each
//! account row navigates to a detail sub-page with per-service toggles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::accounts::{self, OnlineAccount};
use waft_ui_gtk::vdom::Component;

use crate::display::settings_sub_page::SettingsSubPage;
use crate::entity_list_group::EntityListGroup;

type AccountRowEntry = (AccountRow, Urn, Rc<dyn Fn()>);
use crate::i18n::t;
use crate::online_accounts::account_detail::{
    AccountDetailOutput, AccountDetailPage, AccountDetailProps,
};
use crate::online_accounts::account_row::{AccountRow, AccountRowProps, ServiceProps};
use crate::search_index::SearchIndex;

/// Smart container for the Online Accounts settings page.
pub struct OnlineAccountsPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Online Accounts page.
struct OnlineAccountsPageState {
    account_rows: HashMap<String, AccountRowEntry>,
    account_details: HashMap<String, AccountDetailPage>,
    sorted_ids: Vec<String>,
    list_group: EntityListGroup,
}

impl OnlineAccountsPage {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-online-accounts");
        idx.add_section_deferred("online-accounts", &page_title, &t("online-accounts-title"), "online-accounts-title");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
        navigation_view: &adw::NavigationView,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let list_group = EntityListGroup::new(
            &root,
            "contacts-symbolic",
            &t("online-accounts-no-accounts"),
            &t("online-accounts-no-accounts-desc"),
            &t("online-accounts-title"),
        );

        let add_button = gtk::Button::builder()
            .label(t("online-accounts-add-account"))
            .css_classes(["suggested-action", "pill"])
            .halign(gtk::Align::Start)
            .build();
        add_button.connect_clicked(|_| {
            if let Err(e) = std::process::Command::new("gnome-control-center")
                .arg("online-accounts")
                .spawn()
            {
                log::warn!("Failed to launch gnome-control-center: {e}");
            }
        });
        root.append(&add_button);

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            idx.backfill_widget("online-accounts", &t("online-accounts-title"), None, Some(&list_group.group));
        }

        let state = Rc::new(RefCell::new(OnlineAccountsPageState {
            account_rows: HashMap::new(),
            account_details: HashMap::new(),
            sorted_ids: Vec::new(),
            list_group,
        }));

        // Subscribe to online-account changes (future updates + initial reconciliation)
        crate::subscription::subscribe_entities::<OnlineAccount, _>(
            entity_store,
            accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
            {
                let state = state.clone();
                let cb = action_callback.clone();
                let nav = navigation_view.clone();
                move |online_accounts| {
                    log::debug!(
                        "[online-accounts-page] Reconciling: {} accounts",
                        online_accounts.len()
                    );
                    Self::reconcile(&state, &online_accounts, &cb, &nav);
                }
            },
        );

        Self { root }
    }

    /// Reconcile the account row list with current entity data.
    fn reconcile(
        state: &Rc<RefCell<OnlineAccountsPageState>>,
        accounts: &[(Urn, OnlineAccount)],
        action_callback: &EntityActionCallback,
        navigation_view: &adw::NavigationView,
    ) {
        let mut state = state.borrow_mut();

        // Build sorted list of account IDs for stable ordering
        let mut current_ids: Vec<String> = accounts.iter().map(|(_, a)| a.id.clone()).collect();
        current_ids.sort();
        current_ids.dedup();

        let mut seen = std::collections::HashSet::new();

        for (urn, account) in accounts {
            seen.insert(account.id.clone());

            let detail_props = AccountDetailProps {
                provider_name: account.provider_name.clone(),
                presentation_identity: account.presentation_identity.clone(),
                status: account.status.clone(),
                services: account
                    .services
                    .iter()
                    .map(|s| ServiceProps {
                        name: s.name.clone(),
                        enabled: s.enabled,
                    })
                    .collect(),
                locked: account.locked,
            };

            if let Some(detail) = state.account_details.get(&account.id) {
                // Update the existing detail page
                detail.update(&detail_props);
            }

            if let Some((existing, _, nav_fn)) = state.account_rows.get(&account.id) {
                let props = AccountRowProps {
                    id: account.id.clone(),
                    provider_name: account.provider_name.clone(),
                    presentation_identity: account.presentation_identity.clone(),
                    status: account.status.clone(),
                    services: detail_props.services.clone(),
                    locked: account.locked,
                    on_navigate: Some(nav_fn.clone()),
                };
                existing.update(&props);
            } else {
                // Create the detail page and sub-page wrapper first
                let detail_page = AccountDetailPage::new(&detail_props);

                let sub_page = SettingsSubPage::new(
                    &account.presentation_identity,
                    &detail_page.root,
                );
                let nav_page = sub_page.root.clone();

                // Wire detail output events to actions
                {
                    let cb = action_callback.clone();
                    let row_urn = urn.clone();
                    let nav_for_output = navigation_view.clone();
                    detail_page.connect_output(move |output| match output {
                        AccountDetailOutput::EnableService { service_name } => {
                            cb(
                                row_urn.clone(),
                                "enable-service".to_string(),
                                serde_json::json!({ "service_name": service_name }),
                            );
                        }
                        AccountDetailOutput::DisableService { service_name } => {
                            cb(
                                row_urn.clone(),
                                "disable-service".to_string(),
                                serde_json::json!({ "service_name": service_name }),
                            );
                        }
                        AccountDetailOutput::RemoveAccount => {
                            let cb_inner = cb.clone();
                            let urn_inner = row_urn.clone();
                            let nav_inner = nav_for_output.clone();
                            let confirm = adw::AlertDialog::builder()
                                .heading(t("online-accounts-remove-confirm-title"))
                                .body(t("online-accounts-remove-confirm-body"))
                                .close_response("cancel")
                                .default_response("cancel")
                                .build();
                            confirm.add_response("cancel", &t("notif-cancel"));
                            confirm.add_response(
                                "remove",
                                &t("online-accounts-remove-account"),
                            );
                            confirm.set_response_appearance(
                                "remove",
                                adw::ResponseAppearance::Destructive,
                            );
                            confirm.connect_response(None, move |_, response| {
                                if response == "remove" {
                                    cb_inner(
                                        urn_inner.clone(),
                                        "remove-account".to_string(),
                                        serde_json::Value::Null,
                                    );
                                    nav_inner.pop();
                                }
                            });
                            confirm.present(Some(&nav_for_output));
                        }
                    });
                }

                // Build the navigate callback that pushes the sub-page
                let nav_view = navigation_view.clone();
                let nav_fn: Rc<dyn Fn()> = Rc::new(move || {
                    nav_view.push(&nav_page);
                });

                let props = AccountRowProps {
                    id: account.id.clone(),
                    provider_name: account.provider_name.clone(),
                    presentation_identity: account.presentation_identity.clone(),
                    status: account.status.clone(),
                    services: detail_props.services.clone(),
                    locked: account.locked,
                    on_navigate: Some(nav_fn.clone()),
                };

                let row = AccountRow::build(&props);

                // Insert in sorted position
                state.list_group.insert_sorted(&row.widget(), &account.id, &current_ids);
                state
                    .account_rows
                    .insert(account.id.clone(), (row, urn.clone(), nav_fn));
                state.account_details.insert(account.id.clone(), detail_page);
            }
        }

        // Remove rows no longer present
        let to_remove: Vec<String> = state
            .account_rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some((row, _, _)) = state.account_rows.remove(&key) {
                state.list_group.list_box.remove(&row.widget());
            }
            state.account_details.remove(&key);
        }

        state.sorted_ids = current_ids;
        state.list_group.toggle_visibility(!state.account_rows.is_empty());
    }
}
