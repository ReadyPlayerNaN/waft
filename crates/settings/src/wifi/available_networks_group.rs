//! Available WiFi networks preferences group.
//!
//! Dumb widget displaying WiFi networks found during scanning.
//! Always visible; scanning state controls spinner, search button,
//! and description text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::i18n::t;
use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::network::WiFiNetwork;

use super::network_row::{NetworkRow, NetworkRowOutput, NetworkRowProps};

/// Output events from the available networks group.
pub enum AvailableNetworksGroupOutput {
    /// Trigger a WiFi scan on the adapter.
    Scan,
    /// Request password for an unsaved secure network.
    ConnectWithPassword { urn: Urn, ssid: String },
}

/// Callback type for available networks group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AvailableNetworksGroupOutput)>>>>;

/// Group displaying available (discovered) WiFi networks.
pub struct AvailableNetworksGroup {
    pub root: adw::PreferencesGroup,
    spinner: gtk::Spinner,
    search_button: gtk::Button,
    rows: HashMap<String, NetworkRow>,
    ordered_keys: Vec<String>,
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
            rows: HashMap::new(),
            ordered_keys: Vec::new(),
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
        let mut seen = std::collections::HashSet::new();
        let mut ordered_keys = Vec::with_capacity(networks.len());

        for (urn, network) in networks {
            let urn_str = urn.as_str().to_string();
            ordered_keys.push(urn_str.clone());
            seen.insert(urn_str.clone());

            let props = NetworkRowProps {
                ssid: network.ssid.clone(),
                strength: network.strength,
                secure: network.secure,
                connected: false,
                connecting: network.connecting,
                on_navigate: None,
            };

            if let Some(existing) = self.rows.get(&urn_str) {
                existing.update(&props);
            } else {
                let row = NetworkRow::build(&props);
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                let output_cb = self.output_cb.clone();
                let is_secure = network.secure;
                let ssid_for_cb = network.ssid.clone();
                row.connect_output(move |output| match output {
                    NetworkRowOutput::Connect => {
                        if is_secure {
                            if let Some(ref callback) = *output_cb.borrow() {
                                callback(AvailableNetworksGroupOutput::ConnectWithPassword {
                                    urn: urn_clone.clone(),
                                    ssid: ssid_for_cb.clone(),
                                });
                            }
                        } else {
                            cb(
                                urn_clone.clone(),
                                "connect".to_string(),
                                serde_json::Value::Null,
                            );
                        }
                    }
                    NetworkRowOutput::Disconnect => {}
                });
                self.root.add(&row.widget());
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
                self.root.remove(&row.widget());
            }
        }

        self.ordered_keys = ordered_keys
            .into_iter()
            .filter(|key| self.rows.contains_key(key))
            .collect();
        let mut previous: Option<gtk::Widget> = None;
        for key in &self.ordered_keys {
            if let Some(widget) = self.rows.get(key).map(NetworkRow::widget) {
                widget.insert_after(&self.root, previous.as_ref());
                previous = Some(widget);
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
