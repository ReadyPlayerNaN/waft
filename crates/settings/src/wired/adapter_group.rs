//! Per-adapter Wired preferences group.
//!
//! Dumb widget displaying wired adapter status, IP information,
//! and child connection profile rows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_ui_gtk::vdom::Component;

use crate::i18n::t;
use waft_protocol::Urn;
use waft_protocol::entity::network::{EthernetConnection, IpInfo};

use super::connection_row::{ConnectionRowOutput, ConnectionRowProps, WiredConnectionRow};

/// Props for creating or updating a wired adapter group.
#[derive(Clone, PartialEq)]
pub struct WiredAdapterGroupProps {
    pub name: String,
    pub connected: bool,
    pub ip: Option<IpInfo>,
    pub public_ip: Option<String>,
}

/// Output events from a wired adapter group.
pub enum WiredAdapterGroupOutput {
    /// Toggle the wired connection on/off.
    ToggleConnection,
}

/// Callback type for adapter group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(WiredAdapterGroupOutput)>>>>;

/// Per-adapter wired preferences group with connection details.
pub struct WiredAdapterGroup {
    pub root: adw::PreferencesGroup,
    connected_row: adw::SwitchRow,
    ip_row: adw::ActionRow,
    gateway_row: adw::ActionRow,
    public_ip_row: adw::ActionRow,
    /// Guard against feedback loops when programmatically updating switch state.
    updating: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
    connection_rows: HashMap<String, WiredConnectionRow>,
}

impl Component for WiredAdapterGroup {
    type Props = WiredAdapterGroupProps;
    type Output = WiredAdapterGroupOutput;

    fn build(props: &Self::Props) -> Self {
        let group = adw::PreferencesGroup::builder().title(&props.name).build();

        let connected_row = adw::SwitchRow::builder().title(t("wired-connected")).build();
        group.add(&connected_row);

        let ip_row = adw::ActionRow::builder()
            .title(t("wired-ip-address"))
            .visible(false)
            .build();
        group.add(&ip_row);

        let gateway_row = adw::ActionRow::builder()
            .title(t("wired-gateway"))
            .visible(false)
            .build();
        group.add(&gateway_row);

        let public_ip_row = adw::ActionRow::builder()
            .title(t("wired-public-ip"))
            .visible(false)
            .build();
        group.add(&public_ip_row);

        let updating = Rc::new(RefCell::new(false));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        let cb = output_cb.clone();
        let guard = updating.clone();
        connected_row.connect_active_notify(move |_row| {
            if *guard.borrow() {
                return;
            }
            if let Some(ref callback) = *cb.borrow() {
                callback(WiredAdapterGroupOutput::ToggleConnection);
            }
        });

        let adapter = Self {
            root: group,
            connected_row,
            ip_row,
            gateway_row,
            public_ip_row,
            updating,
            output_cb,
            connection_rows: HashMap::new(),
        };

        adapter.update(props);
        adapter
    }

    fn update(&self, props: &Self::Props) {
        *self.updating.borrow_mut() = true;

        self.root.set_title(&props.name);
        self.connected_row.set_active(props.connected);

        if let Some(ref ip) = props.ip {
            self.ip_row
                .set_subtitle(&format!("{}/{}", ip.address, ip.prefix));
            self.ip_row.set_visible(true);

            if let Some(ref gw) = ip.gateway {
                self.gateway_row.set_subtitle(gw);
                self.gateway_row.set_visible(true);
            } else {
                self.gateway_row.set_visible(false);
            }
        } else {
            self.ip_row.set_visible(false);
            self.gateway_row.set_visible(false);
        }

        if let Some(ref public_ip) = props.public_ip {
            self.public_ip_row.set_subtitle(public_ip);
            self.public_ip_row.set_visible(true);
        } else {
            self.public_ip_row.set_visible(false);
        }

        *self.updating.borrow_mut() = false;
    }

    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}

impl WiredAdapterGroup {
    /// Reconcile connection profile rows with new data.
    ///
    /// Adds, updates, or removes connection rows to match the provided list.
    pub fn reconcile_connections(
        &mut self,
        connections: &[(Urn, EthernetConnection)],
        action_callback: &EntityActionCallback,
    ) {
        let mut seen = std::collections::HashSet::new();

        for (urn, connection) in connections {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let props = ConnectionRowProps {
                name: connection.name.clone(),
                active: connection.active,
            };

            if let Some(existing) = self.connection_rows.get(&urn_str) {
                existing.update(&props);
            } else {
                let row = WiredConnectionRow::build(&props);
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                row.connect_output(move |output| {
                    let action = match output {
                        ConnectionRowOutput::Activate => "activate",
                        ConnectionRowOutput::Deactivate => "deactivate",
                    };
                    cb(
                        urn_clone.clone(),
                        action.to_string(),
                        serde_json::Value::Null,
                    );
                });
                self.root.add(&row.root);
                self.connection_rows.insert(urn_str, row);
            }
        }

        let to_remove: Vec<String> = self
            .connection_rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(row) = self.connection_rows.remove(&key) {
                self.root.remove(&row.root);
            }
        }
    }
}
