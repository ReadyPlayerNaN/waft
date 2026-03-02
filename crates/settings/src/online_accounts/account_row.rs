//! Dumb widget for a single online account row.
//!
//! Renders account provider name, identity, status badge, and per-service
//! toggle switches. Service toggles are disabled when the account status
//! is not Active or when the account is locked.

use waft_protocol::entity::accounts::AccountStatus;
use waft_ui_gtk::vdom::primitives::{VActionRow, VBox, VLabel, VSwitchRow};
use waft_ui_gtk::vdom::{RenderCallback, RenderFn, VNode};

use crate::i18n::t;

/// Input data for constructing or updating an account row.
#[derive(Clone, PartialEq)]
pub struct AccountRowProps {
    pub id: String,
    pub provider_name: String,
    pub presentation_identity: String,
    pub status: AccountStatus,
    pub services: Vec<ServiceProps>,
    pub locked: bool,
}

/// A single service within an account.
#[derive(Clone, PartialEq)]
pub struct ServiceProps {
    pub name: String,
    pub enabled: bool,
}

/// Output events from an account row.
#[derive(Debug, Clone)]
pub enum AccountRowOutput {
    EnableService { service_name: String },
    DisableService { service_name: String },
}

pub(crate) struct AccountRowRender;

impl RenderFn for AccountRowRender {
    type Props = AccountRowProps;
    type Output = AccountRowOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<AccountRowOutput>) -> VNode {
        let controllable = props.status == AccountStatus::Active && !props.locked;

        let (status_text, status_css) = match props.status {
            AccountStatus::Active => (t("online-accounts-status-active"), "success"),
            AccountStatus::CredentialsNeeded => {
                (t("online-accounts-status-credentials-needed"), "warning")
            }
            AccountStatus::NeedsAttention => {
                (t("online-accounts-status-needs-attention"), "error")
            }
        };

        // Account header row with status
        let header = VNode::action_row(
            VActionRow::new(&props.provider_name)
                .subtitle(&props.presentation_identity)
                .suffix(VNode::vbox(
                    VBox::horizontal(4)
                        .valign(gtk::Align::Center)
                        .child(VNode::label(
                            VLabel::new(&status_text).css_class(status_css),
                        )),
                )),
        );

        let mut container = VBox::vertical(0).child(header);

        // Per-service switch rows
        for service in &props.services {
            let service_label = service_display_name(&service.name);

            let emit_clone = emit.clone();
            let svc_name = service.name.clone();
            let svc_enabled = service.enabled;

            container = container.child(VNode::switch_row(
                VSwitchRow::new(&service_label, service.enabled)
                    .sensitive(controllable)
                    .on_toggle(move |new_state| {
                        if let Some(ref cb) = *emit_clone.borrow() {
                            if new_state != svc_enabled {
                                if new_state {
                                    cb(AccountRowOutput::EnableService {
                                        service_name: svc_name.clone(),
                                    });
                                } else {
                                    cb(AccountRowOutput::DisableService {
                                        service_name: svc_name.clone(),
                                    });
                                }
                            }
                        }
                    }),
            ));
        }

        VNode::vbox(container)
    }
}

/// Map a service identifier to its i18n display name.
fn service_display_name(service_id: &str) -> String {
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

pub type AccountRow = waft_ui_gtk::vdom::RenderComponent<AccountRowRender>;
