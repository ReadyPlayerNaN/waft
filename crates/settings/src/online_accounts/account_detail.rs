//! Account detail props flow down; actions flow up; service rows reconcile in place.
use crate::{i18n::t, online_accounts::account_row::ServiceProps};
use adw::prelude::*;
use gtk::glib;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use waft_protocol::entity::accounts::AccountStatus;
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AccountDetailOutput)>>>>;

#[derive(Clone, PartialEq)]
pub struct AccountDetailProps {
    pub provider_name: String,
    pub presentation_identity: String,
    pub status: AccountStatus,
    pub services: Vec<ServiceProps>,
    pub locked: bool,
    pub available: bool,
    pub settings: bool,
}
#[derive(Debug, Clone)]
pub enum AccountDetailOutput {
    EnableService { service_name: String },
    DisableService { service_name: String },
    RemoveAccount,
    OpenAccountSettings,
}
pub struct AccountDetailPage {
    pub root: gtk::Box,
    group: adw::PreferencesGroup,
    switch_rows: RefCell<HashMap<String, (adw::SwitchRow, glib::SignalHandlerId)>>,
    remove_button: gtk::Button,
    settings_button: gtk::Button,
    locked_notice: gtk::Label,
    output_cb: OutputCallback,
    props: RefCell<AccountDetailProps>,
    pending: Cell<bool>,
}
impl AccountDetailPage {
    pub fn new(props: &AccountDetailProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        let group = adw::PreferencesGroup::new();
        root.append(&group);
        let hint = gtk::Label::builder()
            .label(t("online-accounts-additional-services"))
            .wrap(true)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        root.append(&hint);
        let locked_notice = gtk::Label::builder()
            .label(t("online-accounts-locked"))
            .wrap(true)
            .xalign(0.0)
            .build();
        root.append(&locked_notice);
        let settings_button = gtk::Button::builder()
            .label(t("online-accounts-open-settings"))
            .halign(gtk::Align::Start)
            .build();
        root.append(&settings_button);
        let remove_button = gtk::Button::builder()
            .label(t("online-accounts-remove-account"))
            .css_classes(["destructive-action", "pill"])
            .halign(gtk::Align::Start)
            .build();
        root.append(&remove_button);
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        for (button, output) in [
            (&remove_button, AccountDetailOutput::RemoveAccount),
            (&settings_button, AccountDetailOutput::OpenAccountSettings),
        ] {
            let cb = output_cb.clone();
            button.connect_clicked(move |_| {
                if let Some(callback) = cb.borrow().as_ref() {
                    callback(output.clone());
                }
            });
        }
        let page = Self {
            root,
            group,
            switch_rows: RefCell::new(HashMap::new()),
            remove_button,
            settings_button,
            locked_notice,
            output_cb,
            props: RefCell::new(props.clone()),
            pending: Cell::new(false),
        };
        page.update(props);
        page
    }
    pub fn connect_output(&self, callback: impl Fn(AccountDetailOutput) + 'static) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
    pub fn set_pending(&self, pending: bool) {
        self.pending.set(pending);
        self.update_controls();
    }
    pub fn set_available(&self, available: bool) {
        self.props.borrow_mut().available = available;
        self.update_controls();
    }
    pub fn restore(&self) {
        let props = self.props.borrow().clone();
        self.update(&props);
    }
    pub fn update(&self, props: &AccountDetailProps) {
        *self.props.borrow_mut() = props.clone();
        self.group.set_title(&props.provider_name);
        self.group.set_description(Some(&glib::markup_escape_text(
            &props.presentation_identity,
        )));
        let mut rows = self.switch_rows.borrow_mut();
        let removed: Vec<_> = rows
            .keys()
            .filter(|name| !props.services.iter().any(|s| &s.name == *name))
            .cloned()
            .collect();
        for name in removed {
            if let Some((row, _)) = rows.remove(&name) {
                self.group.remove(&row);
            }
        }
        for service in &props.services {
            if let Some((row, handler)) = rows.get(&service.name) {
                row.block_signal(handler);
                row.set_active(service.enabled);
                row.unblock_signal(handler);
            } else {
                let row = adw::SwitchRow::builder()
                    .title(service_display_name(&service.name))
                    .active(service.enabled)
                    .build();
                let cb = self.output_cb.clone();
                let name = service.name.clone();
                let handler = row.connect_active_notify(move |row| {
                    if let Some(callback) = cb.borrow().as_ref() {
                        callback(if row.is_active() {
                            AccountDetailOutput::EnableService {
                                service_name: name.clone(),
                            }
                        } else {
                            AccountDetailOutput::DisableService {
                                service_name: name.clone(),
                            }
                        });
                    }
                });
                self.group.add(&row);
                rows.insert(service.name.clone(), (row, handler));
            }
        }
        drop(rows);
        self.update_controls();
    }
    fn update_controls(&self) {
        let props = self.props.borrow();
        let controls = props.available
            && !props.locked
            && props.status == AccountStatus::Active
            && !self.pending.get();
        self.locked_notice.set_visible(props.locked);
        self.group.set_sensitive(controls);
        self.remove_button
            .set_sensitive(props.available && !props.locked && !self.pending.get());
        self.settings_button.set_visible(props.settings);
        self.settings_button.set_sensitive(!self.pending.get());
    }
}
#[cfg(test)]
impl AccountDetailPage {
    pub(crate) fn test_service_row(&self, name: &str) -> adw::SwitchRow {
        self.switch_rows.borrow()[name].0.clone()
    }
    pub(crate) fn assert_gtk_contract() {
        let mut props = AccountDetailProps {
            provider_name: "Fixture".into(),
            presentation_identity: "fixture identity".into(),
            status: AccountStatus::Active,
            services: vec![ServiceProps {
                name: "calendar".into(),
                enabled: true,
            }],
            locked: false,
            available: true,
            settings: true,
        };
        let page = Self::new(&props);
        let row = page.test_service_row("calendar");
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        page.connect_output(move |_| seen.set(seen.get() + 1));
        props.services[0].enabled = false;
        page.update(&props);
        assert_eq!(calls.get(), 0, "props cannot dispatch writes");
        assert_eq!(
            row,
            page.test_service_row("calendar"),
            "row identity stable"
        );
        row.set_active(true);
        assert_eq!(calls.get(), 1);
        page.set_pending(true);
        assert!(!page.group.is_sensitive());
        page.restore();
        assert!(!row.is_active());
        assert_eq!(calls.get(), 1, "restoration cannot dispatch writes");
        page.set_pending(false);
        props.locked = true;
        page.update(&props);
        assert!(page.locked_notice.is_visible());
        assert!(!page.remove_button.is_sensitive());
        assert!(!page.group.is_sensitive());
        props.locked = false;
        props.available = false;
        page.update(&props);
        assert!(!page.group.is_sensitive());
        assert!(page.settings_button.is_visible());
        assert!(page.settings_button.is_sensitive());
        props.services.push(ServiceProps {
            name: "files".into(),
            enabled: false,
        });
        page.update(&props);
        assert_eq!(row, page.test_service_row("calendar"));
        props.services.retain(|s| s.name == "files");
        page.update(&props);
        assert!(!page.switch_rows.borrow().contains_key("calendar"));
    }
}
pub fn service_display_name(service_id: &str) -> String {
    let key = match service_id {
        "mail" => "online-accounts-service-mail",
        "calendar" => "online-accounts-service-calendar",
        "contacts" => "online-accounts-service-contacts",
        "chat" => "online-accounts-service-chat",
        "files" => "online-accounts-service-files",
        "music" => "online-accounts-service-music",
        "photos" => "online-accounts-service-photos",
        "ticketing" => "online-accounts-service-ticketing",
        _ => return service_id.to_string(),
    };
    t(key)
}
