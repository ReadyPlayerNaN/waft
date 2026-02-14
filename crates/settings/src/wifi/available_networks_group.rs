//! Available WiFi networks preferences group.
//!
//! Dumb widget displaying WiFi networks found during scanning.
//! Visible only when scanning is active.

use std::collections::HashMap;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::network::WiFiNetwork;

use super::network_row::{NetworkRow, NetworkRowOutput, NetworkRowProps};

/// Group displaying available (discovered) WiFi networks.
pub struct AvailableNetworksGroup {
    pub root: adw::PreferencesGroup,
    spinner: gtk::Spinner,
    rows: HashMap<String, NetworkRow>,
}

impl AvailableNetworksGroup {
    pub fn new() -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Available Networks")
            .visible(false)
            .build();

        let spinner = gtk::Spinner::new();
        group.set_header_suffix(Some(&spinner));

        Self {
            root: group,
            spinner,
            rows: HashMap::new(),
        }
    }

    /// Reconcile the available network list with new data.
    ///
    /// Adds, updates, or removes network rows to match the provided list.
    /// The `scanning` flag controls group visibility.
    pub fn reconcile(
        &mut self,
        networks: &[(Urn, WiFiNetwork)],
        scanning: bool,
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
                connected: false,
            };

            if let Some(existing) = self.rows.get(&urn_str) {
                existing.apply_props(&props);
            } else {
                let row = NetworkRow::new(&props);
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                row.connect_output(move |output| {
                    let action = match output {
                        NetworkRowOutput::Connect => "connect",
                        NetworkRowOutput::Disconnect => return,
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

        if scanning {
            self.root.set_visible(true);
            self.spinner.start();
            if self.rows.is_empty() {
                self.root
                    .set_description(Some("Searching for networks\u{2026}"));
            } else {
                self.root.set_description(None::<&str>);
            }
        } else {
            self.root.set_visible(false);
            self.spinner.stop();
            self.root.set_description(None::<&str>);
        }
    }
}
