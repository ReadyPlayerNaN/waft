//! Known WiFi networks preferences group.
//!
//! Dumb widget displaying a list of known (saved) WiFi networks.
//! Always visible, shows a placeholder when empty.

use std::collections::HashMap;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::network::WiFiNetwork;
use waft_ui_gtk::vdom::Component;

use crate::i18n::t;

use super::network_row::{NetworkRow, NetworkRowOutput, NetworkRowProps};

/// Group displaying known (saved) WiFi networks.
pub struct KnownNetworksGroup {
    pub root: adw::PreferencesGroup,
    rows: HashMap<String, NetworkRow>,
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
            rows: HashMap::new(),
        }
    }

    /// Reconcile the known network list with new data.
    ///
    /// Adds, updates, or removes network rows to match the provided list.
    pub fn reconcile(
        &mut self,
        networks: &[(Urn, WiFiNetwork)],
        action_callback: &EntityActionCallback,
    ) {
        let mut seen = std::collections::HashSet::new();

        for (urn, network) in networks {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let props = NetworkRowProps {
                ssid: network.ssid.clone(),
                strength: network.strength,
                secure: network.secure,
                connected: network.connected,
            };

            if let Some(existing) = self.rows.get(&urn_str) {
                existing.update(&props);
            } else {
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
                self.root.add(&row.root);
                self.rows.insert(urn_str, row);
            }
        }

        // Remove rows for networks no longer present
        let to_remove: Vec<String> = self
            .rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(row) = self.rows.remove(&key) {
                self.root.remove(&row.root);
            }
        }

        if self.rows.is_empty() {
            self.root.set_description(Some(&t("wifi-no-known-networks")));
        } else {
            self.root.set_description(None::<&str>);
        }
    }
}
