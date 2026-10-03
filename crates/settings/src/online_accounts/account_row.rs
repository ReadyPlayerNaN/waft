//! XML-backed row for a GNOME Online Account.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::accounts::AccountStatus;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;

/// Input data for constructing or updating an account row.
#[derive(Clone)]
pub struct AccountRowProps {
    pub id: String,
    pub provider_name: String,
    pub presentation_identity: String,
    pub status: AccountStatus,
    pub services: Vec<ServiceProps>,
    pub locked: bool,
    pub on_navigate: Option<Rc<dyn Fn()>>,
}

impl PartialEq for AccountRowProps {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.provider_name == other.provider_name
            && self.presentation_identity == other.presentation_identity
            && self.status == other.status
            && self.services == other.services
            && self.locked == other.locked
    }
}

/// A single service within an account.
#[derive(Clone, PartialEq)]
pub struct ServiceProps {
    pub name: String,
    pub enabled: bool,
}

type NavigateCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// A single account row with XML-defined structure.
pub struct AccountRow {
    pub root: adw::ActionRow,
    status_label: gtk::Label,
    navigate_icon_slot: gtk::Box,
    navigate_callback: NavigateCallback,
    _provider_icon: IconWidget,
    _navigate_icon: IconWidget,
}

impl AccountRow {
    pub fn build(props: &AccountRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/account-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("account-row.ui must contain root");
        let provider_icon_slot: gtk::Box = builder
            .object("provider_icon_slot")
            .expect("account-row.ui must contain provider_icon_slot");
        let status_label: gtk::Label = builder
            .object("status_label")
            .expect("account-row.ui must contain status_label");
        let navigate_icon_slot: gtk::Box = builder
            .object("navigate_icon_slot")
            .expect("account-row.ui must contain navigate_icon_slot");

        let provider_icon = IconWidget::from_name(provider_icon(&props.provider_name), 32);
        let navigate_icon = IconWidget::from_name("go-next-symbolic", 16);
        provider_icon_slot.append(provider_icon.widget());
        navigate_icon_slot.append(navigate_icon.widget());

        let navigate_callback: NavigateCallback = Rc::new(RefCell::new(None));
        {
            let navigate_callback = navigate_callback.clone();
            root.connect_activated(move |_| {
                if let Some(callback) = navigate_callback.borrow().as_ref() {
                    callback();
                }
            });
        }

        let row = Self {
            root,
            status_label,
            navigate_icon_slot,
            navigate_callback,
            _provider_icon: provider_icon,
            _navigate_icon: navigate_icon,
        };
        row.update(props);
        row
    }

    pub fn update(&self, props: &AccountRowProps) {
        self.root.set_use_markup(false);
        let (status_text, status_css) = match &props.status {
            AccountStatus::Active => (t("online-accounts-status-active"), "success"),
            AccountStatus::CredentialsNeeded => {
                (t("online-accounts-status-credentials-needed"), "warning")
            }
            AccountStatus::NeedsAttention => (t("online-accounts-status-needs-attention"), "error"),
        };
        self.root.set_title(&props.presentation_identity);
        self.root.set_subtitle(&props.provider_name);
        self.status_label.set_label(&status_text);
        for class in ["success", "warning", "error"] {
            self.status_label.remove_css_class(class);
        }
        self.status_label.add_css_class(status_css);
        self.navigate_icon_slot
            .set_visible(props.on_navigate.is_some());
        self.root.set_activatable(props.on_navigate.is_some());
        *self.navigate_callback.borrow_mut() = props.on_navigate.clone();
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}

/// Map a provider name to a themed icon name.
fn provider_icon(provider_name: &str) -> &'static str {
    let lower = provider_name.to_lowercase();
    if lower.contains("google") {
        "web-browser-symbolic"
    } else if lower.contains("nextcloud") {
        "folder-remote-symbolic"
    } else if lower.contains("microsoft")
        || lower.contains("exchange")
        || lower.contains("imap")
        || lower.contains("smtp")
    {
        "mail-symbolic"
    } else {
        "contact-new-symbolic"
    }
}
