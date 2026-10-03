//! GOA smart container: authoritative rows, explicit readiness, bounded action feedback.
use crate::online_accounts::{
    account_detail::{AccountDetailOutput, AccountDetailPage, AccountDetailProps},
    account_row::{AccountRow, AccountRowProps, ServiceProps},
    add_account_dialog::show_add_account_dialog,
};
use crate::{
    display::settings_sub_page::SettingsSubPage, entity_list_group::EntityListGroup, i18n::t,
    search_index::SearchIndex,
};
use adw::prelude::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::{
    Urn,
    entity::accounts::{
        self, AccountSettingsLaunch, Availability, OnlineAccount, OnlineAccountProvider,
        OnlineAccountsStatus,
    },
};

type AccountRowEntry = (AccountRow, Rc<dyn Fn()>, adw::NavigationPage);
struct Pending {
    account: Option<String>,
    timeout: gtk::glib::SourceId,
}
pub struct OnlineAccountsPage {
    pub root: gtk::Box,
    #[cfg(test)]
    state: Rc<RefCell<PageState>>,
}
struct PageState {
    rows: HashMap<String, AccountRowEntry>,
    details: HashMap<String, AccountDetailPage>,
    group: EntityListGroup,
    search: Rc<RefCell<SearchIndex>>,
    add: gtk::Button,
    open: gtk::Button,
    health: adw::Banner,
    action: adw::Banner,
    launcher: adw::Banner,
    providers: Vec<(Urn, OnlineAccountProvider)>,
    status: Option<OnlineAccountsStatus>,
    status_fresh: bool,
    pending: HashMap<uuid::Uuid, Pending>,
}
impl OnlineAccountsPage {
    pub fn register_search(idx: &mut SearchIndex) {
        idx.add_section_deferred(
            "online-accounts",
            &t("settings-online-accounts"),
            &t("online-accounts-title"),
            "online-accounts-title",
        );
    }
    pub fn new(
        store: &Rc<EntityStore>,
        callback: &EntityActionCallback,
        search: &Rc<RefCell<SearchIndex>>,
        navigation: &adw::NavigationView,
    ) -> Self {
        let root = crate::page_layout::page_root();
        let health = adw::Banner::builder().revealed(false).build();
        let action = adw::Banner::builder().revealed(false).build();
        let launcher = adw::Banner::builder()
            .title(t("online-accounts-launch-failed"))
            .revealed(false)
            .build();
        root.append(&health);
        root.append(&launcher);
        root.append(&action);
        let group = EntityListGroup::new(
            &root,
            "contacts-symbolic",
            &t("online-accounts-no-accounts"),
            &t("online-accounts-no-accounts-desc"),
            &t("online-accounts-title"),
        );
        let add = gtk::Button::builder()
            .label(t("online-accounts-add-account"))
            .css_classes(["suggested-action", "pill"])
            .halign(gtk::Align::Start)
            .visible(false)
            .build();
        let open = gtk::Button::builder()
            .label(t("online-accounts-open-settings"))
            .halign(gtk::Align::Start)
            .visible(false)
            .build();
        root.append(&add);
        root.append(&open);
        search.borrow_mut().backfill_widget(
            "online-accounts",
            &t("online-accounts-title"),
            None,
            Some(&group.group),
        );
        let state = Rc::new(RefCell::new(PageState {
            rows: HashMap::new(),
            details: HashMap::new(),
            group,
            search: search.clone(),
            add: add.clone(),
            open: open.clone(),
            health,
            action,
            launcher,
            providers: Vec::new(),
            status: None,
            status_fresh: true,
            pending: HashMap::new(),
        }));
        // Subscribe to results before any action can be dispatched.
        let result_state = state.clone();
        store.on_action_success(move |id, _| Self::finish(&result_state, id, None, false));
        let result_state = state.clone();
        store.on_action_error_details(move |id, error| {
            Self::finish(&result_state, id, Some(error_label(&error.code)), false);
        });
        let result_state = state.clone();
        store.on_disconnect(move || {
            let ids: Vec<_> = result_state.borrow().pending.keys().copied().collect();
            for id in ids {
                Self::finish(
                    &result_state,
                    id,
                    Some(t("online-accounts-action-failed")),
                    false,
                );
            }
            let mut st = result_state.borrow_mut();
            st.status_fresh = false;
            for detail in st.details.values() {
                detail.set_available(false);
            }
            Self::health(&st);
        });
        let raw = callback.clone();
        // Details own this callback; a weak back-reference avoids retaining a
        // retired page through its own child widgets.
        let action_state = Rc::downgrade(&state);
        let tracked: EntityActionCallback = Rc::new(move |urn, action, params| {
            let action_state = action_state.upgrade()?;
            let account = (urn.entity_type() == accounts::ONLINE_ACCOUNT_ENTITY_TYPE)
                .then(|| urn.id().to_string());
            if action_state
                .borrow()
                .pending
                .values()
                .any(|p| account.is_some() && p.account == account)
            {
                return None;
            }
            let Some(id) = raw(urn, action, params) else {
                let st = action_state.borrow();
                st.action.set_title(&t("online-accounts-action-failed"));
                st.action.set_revealed(true);
                if let Some(account) = &account
                    && let Some(detail) = st.details.get(account)
                {
                    detail.restore();
                }
                return None;
            };
            let timeout_state = action_state.clone();
            let timeout =
                gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(6), move || {
                    Self::finish(
                        &timeout_state,
                        id,
                        Some(t("online-accounts-action-timeout")),
                        true,
                    );
                });
            {
                let mut st = action_state.borrow_mut();
                if let Some(account) = &account
                    && let Some(detail) = st.details.get(account)
                {
                    detail.set_pending(true);
                }
                st.pending.insert(id, Pending { account, timeout });
                st.action.set_title(&t("online-accounts-action-pending"));
                st.action.set_revealed(true);
                st.add.set_sensitive(false);
            }
            Some(id)
        });
        let add_state = state.clone();
        let add_callback = tracked.clone();
        let parent = root.clone();
        add.connect_clicked(move |_| {
            let providers = add_state.borrow().providers.clone();
            show_add_account_dialog(&parent, &providers, &add_callback);
        });
        let open_callback = tracked.clone();
        open.connect_clicked(move |_| {
            open_callback(
                Urn::new(
                    "gnome-online-accounts",
                    accounts::ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
                    "singleton",
                ),
                "open-account-settings".into(),
                serde_json::Value::Null,
            );
        });
        let reconcile = {
            let store = store.clone();
            let state = state.clone();
            let nav = navigation.clone();
            Rc::new(move || {
                let status = store
                    .get_entities_typed::<OnlineAccountsStatus>(
                        accounts::ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
                    )
                    .into_iter()
                    .next()
                    .map(|(_, s)| s);
                let providers = store.get_entities_typed::<OnlineAccountProvider>(
                    accounts::ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
                );
                let accounts =
                    store.get_entities_typed::<OnlineAccount>(accounts::ONLINE_ACCOUNT_ENTITY_TYPE);
                {
                    let mut st = state.borrow_mut();
                    st.status = status;
                    st.providers = providers;
                }
                Self::reconcile(&state, &accounts, &tracked, &nav);
            }) as Rc<dyn Fn()>
        };
        for entity_type in [
            accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
            accounts::ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
            accounts::ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
        ] {
            let reconcile = reconcile.clone();
            let fresh_state = state.clone();
            let compatibility_store = store.clone();
            store.subscribe_type(entity_type, move || {
                if entity_type == accounts::ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE
                    || compatibility_store
                        .get_entities_raw(accounts::ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE)
                        .is_empty()
                {
                    fresh_state.borrow_mut().status_fresh = true;
                }
                reconcile();
            });
        }
        gtk::glib::idle_add_local_once(move || reconcile());
        Self {
            root,
            #[cfg(test)]
            state,
        }
    }
    fn finish(state: &Rc<RefCell<PageState>>, id: uuid::Uuid, error: Option<String>, timer: bool) {
        let mut st = state.borrow_mut();
        let Some(pending) = st.pending.remove(&id) else {
            return;
        };
        if !timer {
            pending.timeout.remove();
        }
        if let Some(account) = pending.account
            && let Some(detail) = st.details.get(&account)
        {
            detail.set_pending(false);
            if error.is_some() {
                detail.restore();
            }
        }
        if let Some(error) = error {
            st.action.set_title(&error);
            st.action.set_revealed(true);
        } else {
            st.action.set_revealed(!st.pending.is_empty());
        }
        Self::health(&st);
    }
    fn health(st: &PageState) {
        let accounts = if st.status_fresh {
            st.status
                .as_ref()
                .map_or(Availability::Unknown, |s| s.accounts)
        } else {
            Availability::Recovering
        };
        let providers = st
            .status
            .as_ref()
            .map_or(Availability::Unknown, |s| s.providers);
        let message = match accounts {
            Availability::Starting => Some("online-accounts-loading"),
            Availability::Recovering => Some("online-accounts-recovering"),
            Availability::Unavailable => Some("online-accounts-goa-not-running"),
            Availability::Unsupported => Some("online-accounts-unsupported"),
            Availability::Unknown => Some("online-accounts-unknown"),
            Availability::Ready if providers == Availability::Unsupported => {
                Some("online-accounts-providers-unsupported")
            }
            Availability::Ready if providers == Availability::Starting => {
                Some("online-accounts-providers-loading")
            }
            Availability::Ready if providers != Availability::Ready => {
                Some("online-accounts-providers-unavailable")
            }
            Availability::Ready => None,
        };
        st.health.set_revealed(message.is_some());
        if let Some(message) = message {
            st.health.set_title(&t(message));
        }
        st.add.set_visible(!st.providers.is_empty());
        st.add.set_sensitive(
            st.status_fresh
                && st.pending.is_empty()
                && (st.status.is_none() || providers == Availability::Ready),
        );
        st.open.set_visible(st.status.is_some());
        st.launcher.set_revealed(
            st.status
                .as_ref()
                .is_some_and(|s| s.launch == AccountSettingsLaunch::Failed),
        );
    }
    fn reconcile(
        state: &Rc<RefCell<PageState>>,
        accounts: &[(Urn, OnlineAccount)],
        callback: &EntityActionCallback,
        nav: &adw::NavigationView,
    ) {
        let mut st = state.borrow_mut();
        let available = st.status_fresh
            && st
                .status
                .as_ref()
                .is_none_or(|s| s.accounts == Availability::Ready);
        let settings = st.status.is_some();
        let mut sorted: Vec<_> = accounts.iter().map(|(_, a)| a.id.clone()).collect();
        sorted.sort();
        sorted.dedup();
        for (urn, account) in accounts {
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
                available,
                settings,
            };
            if let Some(detail) = st.details.get(&account.id) {
                detail.update(&detail_props);
            }
            if let Some((row, navigate, page)) = st.rows.get(&account.id) {
                page.set_title(&account.presentation_identity);
                row.update(&AccountRowProps {
                    id: account.id.clone(),
                    provider_name: account.provider_name.clone(),
                    presentation_identity: account.presentation_identity.clone(),
                    status: account.status.clone(),
                    services: detail_props.services.clone(),
                    locked: account.locked,
                    on_navigate: Some(navigate.clone()),
                });
                row.root.set_tooltip_text(
                    (!available)
                        .then(|| t("online-accounts-recovering"))
                        .as_deref(),
                );
            } else {
                let detail = AccountDetailPage::new(&detail_props);
                let page = SettingsSubPage::new(&account.presentation_identity, &detail.root).root;
                let cb = callback.clone();
                let target = urn.clone();
                let parent = nav.clone();
                detail.connect_output(move |output| match output {
                    AccountDetailOutput::EnableService { service_name } => {
                        cb(
                            target.clone(),
                            "enable-service".into(),
                            serde_json::json!({"service_name":service_name}),
                        );
                    }
                    AccountDetailOutput::DisableService { service_name } => {
                        cb(
                            target.clone(),
                            "disable-service".into(),
                            serde_json::json!({"service_name":service_name}),
                        );
                    }
                    AccountDetailOutput::OpenAccountSettings => {
                        cb(
                            target.clone(),
                            "open-account-settings".into(),
                            serde_json::Value::Null,
                        );
                    }
                    AccountDetailOutput::RemoveAccount => {
                        let dialog = adw::AlertDialog::builder()
                            .heading(t("online-accounts-remove-confirm-title"))
                            .body(t("online-accounts-remove-confirm-body"))
                            .close_response("cancel")
                            .default_response("cancel")
                            .build();
                        dialog.add_response("cancel", &t("notif-cancel"));
                        dialog.add_response("remove", &t("online-accounts-remove-account"));
                        dialog.set_response_appearance(
                            "remove",
                            adw::ResponseAppearance::Destructive,
                        );
                        let cb = cb.clone();
                        let target = target.clone();
                        dialog.connect_response(None, move |_, response| {
                            if response == "remove" {
                                cb(
                                    target.clone(),
                                    "remove-account".into(),
                                    serde_json::Value::Null,
                                );
                            }
                        });
                        dialog.present(Some(&parent));
                    }
                });
                let navigation = nav.clone();
                let target_page = page.clone();
                let navigate: Rc<dyn Fn()> = Rc::new(move || {
                    navigation.push(&target_page);
                });
                let row = AccountRow::build(&AccountRowProps {
                    id: account.id.clone(),
                    provider_name: account.provider_name.clone(),
                    presentation_identity: account.presentation_identity.clone(),
                    status: account.status.clone(),
                    services: detail_props.services.clone(),
                    locked: account.locked,
                    on_navigate: Some(navigate.clone()),
                });
                st.group.insert_sorted(&row.widget(), &account.id, &sorted);
                st.rows.insert(account.id.clone(), (row, navigate, page));
                st.details.insert(account.id.clone(), detail);
            }
        }
        let removed: Vec<_> = st
            .rows
            .keys()
            .filter(|id| !sorted.contains(id))
            .cloned()
            .collect();
        for id in removed {
            if let Some((row, _, page)) = st.rows.remove(&id) {
                if nav.visible_page().as_ref() == Some(&page) {
                    nav.pop();
                }
                st.group.list_box.remove(&row.widget());
            }
            st.details.remove(&id);
            let cancelled: Vec<_> = st
                .pending
                .iter()
                .filter_map(|(uuid, p)| (p.account.as_ref() == Some(&id)).then_some(*uuid))
                .collect();
            for uuid in cancelled {
                if let Some(pending) = st.pending.remove(&uuid) {
                    pending.timeout.remove();
                }
            }
        }
        if st.pending.is_empty() && st.action.title() == t("online-accounts-action-pending") {
            st.action.set_revealed(false);
        }
        st.group.toggle_visibility(!st.rows.is_empty());
        st.group.empty_state.set_visible(
            st.rows.is_empty()
                && st
                    .status
                    .as_ref()
                    .is_some_and(|s| s.accounts == Availability::Ready),
        );
        Self::health(&st);
        let mut idx = st.search.borrow_mut();
        idx.remove_entries("online-accounts", &t("online-accounts-title"));
        idx.add_section(
            "online-accounts",
            &t("settings-online-accounts"),
            &t("online-accounts-title"),
            "online-accounts-title",
            &st.group.group,
        );
        for (_, account) in accounts {
            if let Some((row, _, _)) = st.rows.get(&account.id) {
                idx.add_input(
                    "online-accounts",
                    &t("settings-online-accounts"),
                    &t("online-accounts-title"),
                    &account.presentation_identity,
                    &account.presentation_identity,
                    &row.widget(),
                );
            }
        }
    }
}
fn error_label(code: &str) -> String {
    t(match code {
        "goa.launch-unavailable" | "goa.launch-failed" => "online-accounts-launch-failed",
        "action.timeout" => "online-accounts-action-timeout",
        "goa.unavailable" => "online-accounts-recovering",
        "goa.locked" => "online-accounts-locked",
        "goa.service-unsupported" | "goa.unsupported-api" => "online-accounts-unsupported",
        _ => "online-accounts-action-failed",
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::{
        AppNotification,
        error::{ProtocolError, ProtocolErrorScope},
    };
    fn updated(store: &EntityStore, urn: Urn, data: &impl serde::Serialize) {
        store.handle_notification(AppNotification::EntityUpdated {
            urn,
            entity_type: None,
            data: serde_json::to_value(data).expect("fixture payload"),
        });
    }
    fn drain() {
        let context = gtk::glib::MainContext::default();
        for _ in 0..100 {
            if !context.pending() {
                break;
            }
            context.iteration(false);
        }
    }
    #[test]
    #[ignore = "Requires an isolated GTK display; run explicitly under Broadway or a disposable Wayland session"]
    fn gtk_online_account_action_feedback_and_recovery() {
        gtk::init().expect("GTK display");
        adw::init().expect("Adwaita");
        crate::resources::register();
        AccountDetailPage::assert_gtk_contract();
        let store = Rc::new(EntityStore::new());
        let requests = Rc::new(RefCell::new(Vec::new()));
        let seen = requests.clone();
        let callback: EntityActionCallback = Rc::new(move |_, _, _| {
            let id = uuid::Uuid::new_v4();
            seen.borrow_mut().push(id);
            Some(id)
        });
        let nav = adw::NavigationView::new();
        let search = Rc::new(RefCell::new(SearchIndex::new()));
        let page = OnlineAccountsPage::new(&store, &callback, &search, &nav);
        nav.add(&adw::NavigationPage::new(&page.root, "Fixture accounts"));
        let urn = Urn::new(
            "gnome-online-accounts",
            accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
            "fixture",
        );
        let status_urn = Urn::new(
            "gnome-online-accounts",
            accounts::ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
            "singleton",
        );
        let status = OnlineAccountsStatus {
            accounts: Availability::Ready,
            providers: Availability::Ready,
            launch: AccountSettingsLaunch::Idle,
            ..Default::default()
        };
        updated(&store, status_urn.clone(), &status);
        let account = OnlineAccount {
            id: "fixture".into(),
            provider_type: "ms_graph".into(),
            provider_name: "Fixture".into(),
            presentation_identity: "fixture & <identity>".into(),
            status: accounts::AccountStatus::Active,
            services: vec![accounts::ServiceInfo {
                name: "calendar".into(),
                enabled: true,
            }],
            locked: false,
        };
        updated(&store, urn.clone(), &account);
        drain();
        let state = &page.state;
        let detail_navigation = state.borrow().rows["fixture"].2.clone();
        nav.push(&detail_navigation);
        let row = state.borrow().details["fixture"].test_service_row("calendar");
        let account_row = state.borrow().rows["fixture"].0.root.clone();
        assert!(!account_row.uses_markup());
        row.set_active(false);
        let id = *requests.borrow().last().expect("action");
        assert_eq!(state.borrow().pending.len(), 1);
        assert!(!row.is_sensitive());
        store.handle_notification(AppNotification::ActionError {
            action_id: id,
            error: "legacy error".into(),
            error_details: Some(ProtocolError::new(
                "goa.outcome-unconfirmed",
                "fixture",
                ProtocolErrorScope::Action,
                false,
            )),
        });
        assert!(state.borrow().pending.is_empty());
        assert!(row.is_active(), "restore authoritative state after failure");
        assert!(state.borrow().action.is_revealed());
        row.set_active(false);
        let disconnected = *requests.borrow().last().expect("second action");
        store.handle_disconnect();
        assert!(state.borrow().pending.is_empty());
        assert!(!row.is_sensitive());
        assert_eq!(
            store
                .get_entities_raw(accounts::ONLINE_ACCOUNT_ENTITY_TYPE)
                .len(),
            1,
            "disconnect retains stale rows"
        );
        updated(&store, status_urn.clone(), &status);
        assert!(row.is_sensitive());
        store.handle_notification(AppNotification::ActionSuccess {
            action_id: disconnected,
            data: None,
        });
        assert!(
            state.borrow().pending.is_empty(),
            "late acknowledgement ignored"
        );
        row.set_active(false);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while !state.borrow().pending.is_empty() && std::time::Instant::now() < until {
            drain();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            state.borrow().pending.is_empty(),
            "local deadline releases controls"
        );
        assert!(row.is_active());
        assert!(row.is_sensitive());
        row.set_active(false);
        store.handle_notification(AppNotification::EntityRemoved {
            urn,
            entity_type: None,
        });
        assert!(state.borrow().pending.is_empty());
        assert!(state.borrow().details.is_empty());
        assert_eq!(
            nav.visible_page().expect("return to accounts").title(),
            "Fixture accounts"
        );
        assert!(
            !state.borrow().action.is_revealed(),
            "deletion cannot strand pending banner"
        );
        // No EntityRemoved was delivered while the client was disconnected.
        // A completed empty broker snapshot must still close stale details.
        updated(
            &store,
            Urn::new(
                "gnome-online-accounts",
                accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
                "fixture",
            ),
            &account,
        );
        let stale_navigation = state.borrow().rows["fixture"].2.clone();
        nav.push(&stale_navigation);
        store.handle_disconnect();
        assert_eq!(
            state.borrow().details.len(),
            1,
            "transport loss retains membership until completion"
        );
        updated(&store, status_urn.clone(), &status);
        store.handle_notification(AppNotification::StatusComplete {
            entity_type: accounts::ONLINE_ACCOUNT_ENTITY_TYPE.into(),
        });
        assert!(state.borrow().details.is_empty());
        assert!(state.borrow().rows.is_empty());
        assert_eq!(
            nav.visible_page().expect("stale details retired").title(),
            "Fixture accounts"
        );

        let failed = OnlineAccountsStatus {
            launch: AccountSettingsLaunch::Failed,
            ..status.clone()
        };
        updated(&store, status_urn.clone(), &failed);
        assert!(state.borrow().launcher.is_revealed());
        updated(&store, status_urn, &status);
        assert!(
            !state.borrow().launcher.is_revealed(),
            "new launch clears old failure independently"
        );
        let rejected_store = Rc::new(EntityStore::new());
        let rejected_callback: EntityActionCallback = Rc::new(|_, _, _| None);
        let rejected_nav = adw::NavigationView::new();
        let rejected =
            OnlineAccountsPage::new(&rejected_store, &rejected_callback, &search, &rejected_nav);
        updated(
            &rejected_store,
            Urn::new(
                "gnome-online-accounts",
                accounts::ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
                "singleton",
            ),
            &status,
        );
        updated(
            &rejected_store,
            Urn::new(
                "gnome-online-accounts",
                accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
                "fixture",
            ),
            &account,
        );
        drain();
        let rejected_row = rejected.state.borrow().details["fixture"].test_service_row("calendar");
        rejected_row.set_active(false);
        assert!(rejected.state.borrow().pending.is_empty());
        assert!(
            rejected_row.is_active(),
            "failed dispatch restores the authoritative switch without acquiring an owner"
        );
        assert!(rejected.state.borrow().action.is_revealed());
    }
}
