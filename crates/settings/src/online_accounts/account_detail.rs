//! Detail sub-page for a single online account.
//!
//! Stateful GTK4 widget (not VDOM) showing per-service toggle switches.
//! Created once per account and pushed onto the NavigationView on demand.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use waft_protocol::entity::accounts::AccountStatus;

use crate::i18n::t;
use crate::online_accounts::account_row::ServiceProps;

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AccountDetailOutput)>>>>;

/// Props for the account detail page.
#[derive(Clone, PartialEq)]
pub struct AccountDetailProps {
    pub provider_name: String,
    pub presentation_identity: String,
    pub status: AccountStatus,
    pub services: Vec<ServiceProps>,
    pub locked: bool,
}

/// Output events from the account detail page.
#[derive(Debug, Clone)]
pub enum AccountDetailOutput {
    EnableService { service_name: String },
    DisableService { service_name: String },
    RemoveAccount,
}

/// Stateful detail page for a single account.
pub struct AccountDetailPage {
    pub root: gtk::Box,
    group: adw::PreferencesGroup,
    switch_rows: Vec<(String, adw::SwitchRow, glib::SignalHandlerId)>,
    remove_button: gtk::Button,
    output_cb: OutputCallback,
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

        let group = adw::PreferencesGroup::builder()
            .title(&props.provider_name)
            .description(&props.presentation_identity)
            .build();
        root.append(&group);

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        let mut switch_rows = Vec::new();
        let controllable = props.status == AccountStatus::Active && !props.locked;

        for service in &props.services {
            let label = service_display_name(&service.name);
            let switch_row = adw::SwitchRow::builder()
                .title(&label)
                .active(service.enabled)
                .sensitive(controllable)
                .build();

            let cb_ref = output_cb.clone();
            let svc_name = service.name.clone();
            // The handler_id is stored so update() can block/unblock the signal
            // around set_active() calls, preventing spurious EnableService/DisableService
            // actions when the widget is updated from entity data.
            let handler_id = switch_row.connect_active_notify(move |row| {
                if let Some(ref cb) = *cb_ref.borrow() {
                    if row.is_active() {
                        cb(AccountDetailOutput::EnableService {
                            service_name: svc_name.clone(),
                        });
                    } else {
                        cb(AccountDetailOutput::DisableService {
                            service_name: svc_name.clone(),
                        });
                    }
                }
            });
            group.add(&switch_row);
            switch_rows.push((service.name.clone(), switch_row, handler_id));
        }

        let remove_group = adw::PreferencesGroup::builder().margin_top(24).build();
        let remove_button = gtk::Button::builder()
            .label(t("online-accounts-remove-account"))
            .css_classes(["destructive-action", "pill"])
            .halign(gtk::Align::Start)
            .sensitive(!props.locked)
            .build();
        {
            let cb_ref = output_cb.clone();
            remove_button.connect_clicked(move |_| {
                if let Some(ref cb) = *cb_ref.borrow() {
                    cb(AccountDetailOutput::RemoveAccount);
                }
            });
        }
        remove_group.add(&remove_button);
        root.append(&remove_group);

        Self {
            root,
            group,
            switch_rows,
            remove_button,
            output_cb,
        }
    }

    pub fn connect_output(&self, cb: impl Fn(AccountDetailOutput) + 'static) {
        *self.output_cb.borrow_mut() = Some(Box::new(cb));
    }

    pub fn update(&self, props: &AccountDetailProps) {
        let controllable = props.status == AccountStatus::Active && !props.locked;
        self.group.set_title(&props.provider_name);
        self.group.set_description(Some(&props.presentation_identity));
        for (svc_name, row, handler_id) in &self.switch_rows {
            if let Some(svc) = props.services.iter().find(|s| &s.name == svc_name) {
                // Block the signal to prevent spurious EnableService/DisableService
                // actions when syncing the widget state from incoming entity data.
                row.block_signal(handler_id);
                row.set_active(svc.enabled);
                row.unblock_signal(handler_id);
                row.set_sensitive(controllable);
            }
        }
        self.remove_button.set_sensitive(!props.locked);
    }
}

/// Map a service identifier to its i18n display name.
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
