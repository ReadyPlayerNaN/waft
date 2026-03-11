//! Dumb widget for a single online account row.
//!
//! Renders account provider name, identity, status badge. When `on_navigate`
//! is set the row is activatable and shows a chevron; clicking navigates to a
//! detail sub-page.

use std::rc::Rc;

use waft_protocol::entity::accounts::AccountStatus;
use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::primitives::{VActionRow, VBox, VIcon, VLabel};
use waft_ui_gtk::vdom::{RenderCallback, RenderFn, VNode};

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

/// Output events from an account row (none currently — navigation is via `on_navigate`).
#[derive(Debug, Clone)]
pub enum AccountRowOutput {}

pub(crate) struct AccountRowRender;

impl RenderFn for AccountRowRender {
    type Props = AccountRowProps;
    type Output = AccountRowOutput;

    fn render(props: &Self::Props, _emit: &RenderCallback<AccountRowOutput>) -> VNode {
        let (status_text, status_css) = match props.status {
            AccountStatus::Active => (t("online-accounts-status-active"), "success"),
            AccountStatus::CredentialsNeeded => {
                (t("online-accounts-status-credentials-needed"), "warning")
            }
            AccountStatus::NeedsAttention => {
                (t("online-accounts-status-needs-attention"), "error")
            }
        };

        let icon_name = provider_icon(&props.provider_name);

        // Build the header row: identity as title, provider as subtitle
        let mut row = VActionRow::new(&props.presentation_identity)
            .subtitle(&props.provider_name)
            .prefix(VNode::icon(VIcon::new(
                vec![Icon::Themed(icon_name.to_string())],
                32,
            )))
            .suffix(VNode::vbox(
                VBox::horizontal(4)
                    .valign(gtk::Align::Center)
                    .child(VNode::label(
                        VLabel::new(&status_text).css_class(status_css),
                    )),
            ));

        // When a navigation callback is provided, make the row activatable
        // and append a chevron to indicate drill-down.
        if let Some(navigate) = props.on_navigate.clone() {
            row = row
                .suffix(VNode::icon(VIcon::new(
                    vec![Icon::Themed("go-next-symbolic".to_string())],
                    16,
                )))
                .on_activate(move || navigate());
        }

        VNode::vbox(VBox::vertical(0).child(VNode::action_row(row)))
    }
}

/// Map a provider name to a themed icon name.
fn provider_icon(provider_name: &str) -> &'static str {
    let lower = provider_name.to_lowercase();
    if lower.contains("google") {
        "web-browser-symbolic"
    } else if lower.contains("nextcloud") {
        "folder-remote-symbolic"
    } else if lower.contains("microsoft") || lower.contains("exchange") {
        "mail-symbolic"
    } else if lower.contains("imap") || lower.contains("smtp") {
        "mail-symbolic"
    } else {
        "contact-new-symbolic"
    }
}

pub type AccountRow = waft_ui_gtk::vdom::RenderComponent<AccountRowRender>;
