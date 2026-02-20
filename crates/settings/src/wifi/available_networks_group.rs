//! Available WiFi networks preferences group.
//!
//! Dumb widget displaying WiFi networks found during scanning.
//! Always visible; scanning state controls spinner, search button,
//! and description text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::network::WiFiNetwork;

use crate::i18n::t;

use super::network_row::{NetworkRow, NetworkRowOutput, NetworkRowProps};

/// Output events from the available networks group.
pub enum AvailableNetworksGroupOutput {
    /// Trigger a WiFi scan on the adapter.
    Scan,
}

/// Callback type for available networks group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AvailableNetworksGroupOutput)>>>>;

/// Group displaying available (discovered) WiFi networks.
pub struct AvailableNetworksGroup {
    pub root: adw::PreferencesGroup,
    spinner: gtk::Spinner,
    search_button: gtk::Button,
    scanning: Rc<RefCell<bool>>,
    rows: HashMap<String, NetworkRow>,
    output_cb: OutputCallback,
}

impl AvailableNetworksGroup {
    pub fn new() -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("wifi-available-networks"))
            .build();

        let spinner = gtk::Spinner::new();

        let search_button = gtk::Button::builder()
            .icon_name("system-search-symbolic")
            .css_classes(["flat"])
            .tooltip_text(t("wifi-adapter-scan"))
            .build();

        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .valign(gtk::Align::Center)
            .build();
        header_box.append(&spinner);
        header_box.append(&search_button);

        group.set_header_suffix(Some(&header_box));

        let scanning = Rc::new(RefCell::new(false));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Wire search button click
        let cb = output_cb.clone();
        search_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                callback(AvailableNetworksGroupOutput::Scan);
            }
        });

        Self {
            root: group,
            spinner,
            search_button,
            scanning,
            rows: HashMap::new(),
            output_cb,
        }
    }

    /// Register a callback for available networks group output events.
    pub fn connect_output<F: Fn(AvailableNetworksGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Reconcile the available network list with new data.
    ///
    /// Adds, updates, or removes network rows to match the provided list.
    /// The `scanning` flag controls the spinner, button icon, and description text.
    /// The group is always visible.
    pub fn reconcile(
        &mut self,
        networks: &[(Urn, WiFiNetwork)],
        scanning: bool,
        action_callback: &EntityActionCallback,
    ) {
        *self.scanning.borrow_mut() = scanning;

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

        // Update spinner, button icon, and description
        if scanning {
            self.spinner.start();
            self.search_button.set_icon_name("process-stop-symbolic");
            self.search_button
                .set_tooltip_text(Some(&t("wifi-adapter-scan")));
            if self.rows.is_empty() {
                self.root
                    .set_description(Some(&t("wifi-searching-networks")));
            } else {
                self.root.set_description(None::<&str>);
            }
        } else {
            self.spinner.stop();
            self.search_button.set_icon_name("system-search-symbolic");
            self.search_button
                .set_tooltip_text(Some(&t("wifi-adapter-scan")));
            if self.rows.is_empty() {
                self.root
                    .set_description(Some(&t("wifi-no-available-networks")));
            } else {
                self.root.set_description(None::<&str>);
            }
        }
    }
}
