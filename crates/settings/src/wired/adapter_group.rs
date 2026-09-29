//! XML-backed wired adapter group with explicit keyed connection rows.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::Urn;
use waft_protocol::entity::network::{EthernetConnection, IpInfo};

use crate::i18n::t;

use super::connection_row::{ConnectionRowOutput, ConnectionRowProps, WiredConnectionRow};

/// Props for creating or updating a wired adapter group.
#[derive(Clone, PartialEq)]
pub struct WiredAdapterGroupProps {
    pub name: String,
    pub connected: bool,
    pub ip: Option<IpInfo>,
    pub public_ip: Option<String>,
    pub connections: Vec<(Urn, EthernetConnection)>,
}

/// Output events from a wired adapter group.
#[allow(clippy::enum_variant_names)]
pub enum WiredAdapterGroupOutput {
    ToggleConnection,
    ActivateConnection(Urn),
    DeactivateConnection(Urn),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(WiredAdapterGroupOutput)>>>>;

/// A wired adapter group with explicit keyed child management.
pub struct WiredAdapterGroup {
    pub root: adw::PreferencesGroup,
    connected_row: adw::SwitchRow,
    ip_row: adw::ActionRow,
    gateway_row: adw::ActionRow,
    public_ip_row: adw::ActionRow,
    connections_slot: gtk::Box,
    connections: RefCell<HashMap<String, WiredConnectionRow>>,
    ordered_keys: RefCell<Vec<String>>,
    updating: Rc<Cell<bool>>,
    output_cb: OutputCallback,
}

impl WiredAdapterGroup {
    pub fn build(props: &WiredAdapterGroupProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/wired-adapter-group.ui");
        let root: adw::PreferencesGroup = builder
            .object("root")
            .expect("wired-adapter-group.ui must contain root");
        let connected_row: adw::SwitchRow = builder
            .object("connected_row")
            .expect("wired-adapter-group.ui must contain connected_row");
        let ip_row: adw::ActionRow = builder
            .object("ip_row")
            .expect("wired-adapter-group.ui must contain ip_row");
        let gateway_row: adw::ActionRow = builder
            .object("gateway_row")
            .expect("wired-adapter-group.ui must contain gateway_row");
        let public_ip_row: adw::ActionRow = builder
            .object("public_ip_row")
            .expect("wired-adapter-group.ui must contain public_ip_row");
        let connections_slot: gtk::Box = builder
            .object("connections_slot")
            .expect("wired-adapter-group.ui must contain connections_slot");
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(Cell::new(false));

        connected_row.set_title(&t("wired-connected"));
        ip_row.set_title(&t("wired-ip-address"));
        gateway_row.set_title(&t("wired-gateway"));
        public_ip_row.set_title(&t("wired-public-ip"));
        {
            let output_cb = output_cb.clone();
            let updating = updating.clone();
            connected_row.connect_active_notify(move |_| {
                if updating.get() {
                    return;
                }
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(WiredAdapterGroupOutput::ToggleConnection);
                }
            });
        }

        let group = Self {
            root,
            connected_row,
            ip_row,
            gateway_row,
            public_ip_row,
            connections_slot,
            connections: RefCell::new(HashMap::new()),
            ordered_keys: RefCell::new(Vec::new()),
            updating,
            output_cb,
        };
        group.update(props);
        group
    }

    pub fn update(&self, props: &WiredAdapterGroupProps) {
        self.root.set_title(&props.name);
        self.updating.set(true);
        self.connected_row.set_active(props.connected);
        self.updating.set(false);
        if let Some(ip) = &props.ip {
            self.ip_row
                .set_subtitle(&format!("{}/{}", ip.address, ip.prefix));
            self.ip_row.set_visible(true);
            if let Some(gateway) = &ip.gateway {
                self.gateway_row.set_subtitle(gateway);
                self.gateway_row.set_visible(true);
            } else {
                self.gateway_row.set_visible(false);
            }
        } else {
            self.ip_row.set_visible(false);
            self.gateway_row.set_visible(false);
        }
        if let Some(public_ip) = &props.public_ip {
            self.public_ip_row.set_subtitle(public_ip);
            self.public_ip_row.set_visible(true);
        } else {
            self.public_ip_row.set_visible(false);
        }

        let ordered_keys: Vec<String> = props
            .connections
            .iter()
            .map(|(urn, _)| urn.as_str().to_string())
            .collect();
        let seen: std::collections::HashSet<String> = ordered_keys.iter().cloned().collect();
        for (urn, connection) in &props.connections {
            let key = urn.as_str().to_string();
            let row_props = ConnectionRowProps {
                name: connection.name.clone(),
                active: connection.active,
            };
            if let Some(row) = self.connections.borrow().get(&key) {
                row.update(&row_props);
            } else {
                let row = WiredConnectionRow::build(&row_props);
                let output_cb = self.output_cb.clone();
                let row_urn = urn.clone();
                row.connect_output(move |output| {
                    let event = match output {
                        ConnectionRowOutput::Activate => {
                            WiredAdapterGroupOutput::ActivateConnection(row_urn.clone())
                        }
                        ConnectionRowOutput::Deactivate => {
                            WiredAdapterGroupOutput::DeactivateConnection(row_urn.clone())
                        }
                    };
                    if let Some(callback) = output_cb.borrow().as_ref() {
                        callback(event);
                    }
                });
                self.connections_slot.append(&row.widget());
                // This method is called through interior GTK state ownership;
                // new rows are inserted by `rebuild_connections` below.
                self.connections.borrow_mut().insert(key, row);
            }
        }
        let stale: Vec<String> = self
            .connections
            .borrow()
            .keys()
            .filter(|key| !seen.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(row) = self.connections.borrow_mut().remove(&key) {
                self.connections_slot.remove(&row.widget());
            }
        }
        *self.ordered_keys.borrow_mut() = ordered_keys
            .into_iter()
            .filter(|key| self.connections.borrow().contains_key(key))
            .collect();
        let mut previous: Option<gtk::Widget> = None;
        for key in self.ordered_keys.borrow().iter() {
            if let Some(widget) = self
                .connections
                .borrow()
                .get(key)
                .map(WiredConnectionRow::widget)
            {
                widget.insert_after(&self.connections_slot, previous.as_ref());
                previous = Some(widget);
            }
        }
    }

    pub fn connect_output<F: Fn(WiredAdapterGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
