//! Known WiFi networks preferences group.
//!
//! Dumb widget displaying a list of known (saved) WiFi networks.
//! Each known network has a settings chevron that navigates to a
//! detail sub-page with connection settings and forget button.
//! Always visible, shows a placeholder when empty.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::network::WiFiNetwork;
use waft_ui_gtk::vdom::Component;

use crate::display::settings_sub_page::SettingsSubPage;
use crate::i18n::t;
use crate::wifi::network_detail::{NetworkDetailOutput, NetworkDetailPage, NetworkDetailProps};

use super::network_row::{NetworkRow, NetworkRowOutput, NetworkRowProps};

type NavCallback = std::rc::Rc<dyn Fn()>;

struct KnownNetworkEntry {
    row: NetworkRow,
    detail_page: NetworkDetailPage,
    #[allow(dead_code)] // Kept for sub-page ownership
    sub_page: SettingsSubPage,
    nav_fn: NavCallback,
}

/// Group displaying known (saved) WiFi networks.
pub struct KnownNetworksGroup {
    pub root: adw::PreferencesGroup,
    entries: HashMap<String, KnownNetworkEntry>,
}

impl KnownNetworksGroup {
    pub fn new() -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("wifi-known-networks"))
            .visible(true)
            .description(t("wifi-no-known-networks"))
            .build();

        Self {
            root: group,
            entries: HashMap::new(),
        }
    }

    /// Reconcile the known network list with new data.
    ///
    /// Adds, updates, or removes network rows to match the provided list.
    /// `pending_share_ssid` is set when the user clicks Share, so the WiFi page
    /// can show the QR dialog when the action response arrives.
    pub fn reconcile(
        &mut self,
        networks: &[(Urn, WiFiNetwork)],
        action_callback: &EntityActionCallback,
        navigation_view: &adw::NavigationView,
        pending_share_ssid: &Rc<RefCell<Option<String>>>,
    ) {
        let mut seen = std::collections::HashSet::new();

        for (urn, network) in networks {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let detail_props = NetworkDetailProps::from(network);

            if let Some(existing) = self.entries.get(&urn_str) {
                // Update existing row and detail page
                let props = NetworkRowProps {
                    ssid: network.ssid.clone(),
                    strength: network.strength,
                    secure: network.secure,
                    connected: network.connected,
                    connecting: network.connecting,
                    on_navigate: Some(existing.nav_fn.clone()),
                };
                existing.row.update(&props);
                existing.detail_page.update(&detail_props);
            } else {
                // Create new detail page and sub-page
                let detail_page = NetworkDetailPage::new(&detail_props);
                let sub_page = SettingsSubPage::new(&network.ssid, &detail_page.root);
                let nav_page = sub_page.root.clone();

                // Wire detail output events
                {
                    let cb = action_callback.clone();
                    let row_urn = urn.clone();
                    let nav_for_output = navigation_view.clone();
                    let ssid_for_share = network.ssid.clone();
                    let pending_share = pending_share_ssid.clone();
                    detail_page.connect_output(move |output| match output {
                        NetworkDetailOutput::Forget => {
                            let cb_inner = cb.clone();
                            let urn_inner = row_urn.clone();
                            let nav_inner = nav_for_output.clone();
                            let confirm = adw::AlertDialog::builder()
                                .heading(t("wifi-detail-forget-confirm-title"))
                                .body(t("wifi-detail-forget-confirm-body"))
                                .close_response("cancel")
                                .default_response("cancel")
                                .build();
                            confirm.add_response("cancel", &t("notif-cancel"));
                            confirm.add_response("forget", &t("wifi-detail-forget"));
                            confirm.set_response_appearance(
                                "forget",
                                adw::ResponseAppearance::Destructive,
                            );
                            confirm.connect_response(None, move |_, response| {
                                if response == "forget" {
                                    cb_inner(
                                        urn_inner.clone(),
                                        "forget".to_string(),
                                        serde_json::Value::Null,
                                    );
                                    nav_inner.pop();
                                }
                            });
                            confirm.present(Some(&nav_for_output));
                        }
                        NetworkDetailOutput::Share => {
                            *pending_share.borrow_mut() = Some(ssid_for_share.clone());
                            cb(
                                row_urn.clone(),
                                "share".to_string(),
                                serde_json::Value::Null,
                            );
                        }
                        NetworkDetailOutput::UpdateSettings { settings } => {
                            cb(
                                row_urn.clone(),
                                "update-settings".to_string(),
                                settings,
                            );
                        }
                    });
                }

                // Build the navigate callback that pushes the sub-page
                let nav_view = navigation_view.clone();
                let nav_fn: NavCallback = std::rc::Rc::new(move || {
                    nav_view.push(&nav_page);
                });

                let props = NetworkRowProps {
                    ssid: network.ssid.clone(),
                    strength: network.strength,
                    secure: network.secure,
                    connected: network.connected,
                    connecting: network.connecting,
                    on_navigate: Some(nav_fn.clone()),
                };

                let row = NetworkRow::build(&props);
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                row.connect_output(move |output| {
                    let action = match output {
                        NetworkRowOutput::Connect => "connect",
                        NetworkRowOutput::Disconnect => "disconnect",
                    };
                    cb(
                        urn_clone.clone(),
                        action.to_string(),
                        serde_json::Value::Null,
                    );
                });
                self.root.add(&row.widget());
                self.entries.insert(
                    urn_str,
                    KnownNetworkEntry {
                        row,
                        detail_page,
                        sub_page,
                        nav_fn,
                    },
                );
            }
        }

        // Remove rows for networks no longer present
        let to_remove: Vec<String> = self
            .entries
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(entry) = self.entries.remove(&key) {
                self.root.remove(&entry.row.widget());
            }
        }

        if self.entries.is_empty() {
            self.root.set_description(Some(&t("wifi-no-known-networks")));
        } else {
            self.root.set_description(None::<&str>);
        }
    }
}
