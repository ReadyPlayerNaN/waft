//! Online Accounts settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `online-account` entity type. On entity
//! changes, reconciles the list of account rows showing GOA accounts with
//! per-service toggles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::accounts::{self, OnlineAccount};
use waft_ui_gtk::vdom::Component;

use crate::i18n::t;
use crate::online_accounts::account_row::{
    AccountRow, AccountRowOutput, AccountRowProps, ServiceProps,
};
use crate::search_index::SearchIndex;

/// Smart container for the Online Accounts settings page.
pub struct OnlineAccountsPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Online Accounts page.
struct OnlineAccountsPageState {
    account_rows: HashMap<String, (AccountRow, Urn)>,
    sorted_ids: Vec<String>,
    list_box: gtk::ListBox,
    empty_state: adw::StatusPage,
    group: adw::PreferencesGroup,
}

impl OnlineAccountsPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let empty_state = adw::StatusPage::builder()
            .icon_name("contacts-symbolic")
            .title(t("online-accounts-no-accounts"))
            .description(t("online-accounts-no-accounts-desc"))
            .visible(false)
            .build();
        root.append(&empty_state);

        let group = adw::PreferencesGroup::builder()
            .title(t("online-accounts-title"))
            .visible(false)
            .build();

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        group.add(&list_box);
        root.append(&group);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-online-accounts");
            idx.add_section(
                "online-accounts",
                &page_title,
                &t("online-accounts-title"),
                "online-accounts-title",
                &group,
            );
        }

        let state = Rc::new(RefCell::new(OnlineAccountsPageState {
            account_rows: HashMap::new(),
            sorted_ids: Vec::new(),
            list_box,
            empty_state,
            group,
        }));

        // Subscribe to online-account changes (future updates + initial reconciliation)
        crate::subscription::subscribe_entities::<OnlineAccount, _>(
            entity_store,
            accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
            {
                let state = state.clone();
                let cb = action_callback.clone();
                move |online_accounts| {
                    log::debug!(
                        "[online-accounts-page] Reconciling: {} accounts",
                        online_accounts.len()
                    );
                    Self::reconcile(&state, &online_accounts, &cb);
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
    ) {
        let mut state = state.borrow_mut();

        // Build sorted list of account IDs for stable ordering
        let mut current_ids: Vec<String> = accounts.iter().map(|(_, a)| a.id.clone()).collect();
        current_ids.sort();
        current_ids.dedup();

        let mut seen = std::collections::HashSet::new();

        for (urn, account) in accounts {
            seen.insert(account.id.clone());

            let props = AccountRowProps {
                id: account.id.clone(),
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

            if let Some((existing, _)) = state.account_rows.get(&account.id) {
                existing.update(&props);
            } else {
                let row = AccountRow::build(&props);

                // Wire output events
                let cb = action_callback.clone();
                let row_urn = urn.clone();
                row.connect_output(move |output| {
                    let (action, service_name) = match output {
                        AccountRowOutput::EnableService { service_name } => {
                            ("enable-service", service_name)
                        }
                        AccountRowOutput::DisableService { service_name } => {
                            ("disable-service", service_name)
                        }
                    };
                    cb(
                        row_urn.clone(),
                        action.to_string(),
                        serde_json::json!({ "service_name": service_name }),
                    );
                });

                // Insert in sorted position
                let pos = current_ids
                    .iter()
                    .position(|id| id == &account.id)
                    .unwrap_or(0);
                state.list_box.insert(&row.widget(), pos as i32);
                state
                    .account_rows
                    .insert(account.id.clone(), (row, urn.clone()));
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
            if let Some((row, _)) = state.account_rows.remove(&key) {
                state.list_box.remove(&row.widget());
            }
        }

        state.sorted_ids = current_ids;

        // Toggle empty state vs list visibility
        let has_accounts = !state.account_rows.is_empty();
        state.group.set_visible(has_accounts);
        state.empty_state.set_visible(!has_accounts);
    }
}
