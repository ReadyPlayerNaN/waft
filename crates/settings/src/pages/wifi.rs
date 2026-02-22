//! WiFi settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `network-adapter` (wireless) and `wifi-network`
//! entity types. On entity changes, reconciles adapter groups and network lists.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::network::{
    ADAPTER_ENTITY_TYPE, AdapterKind, NetworkAdapter, WiFiNetwork,
};
use waft_ui_gtk::vdom::{Reconciler, VNode};

use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::wifi::adapter_group::{WifiAdapterGroup, WifiAdapterGroupOutput, WifiAdapterGroupProps};
use crate::wifi::available_networks_group::{AvailableNetworksGroup, AvailableNetworksGroupOutput};
use crate::wifi::known_networks_group::KnownNetworksGroup;

/// Smart container for the WiFi settings page.
pub struct WiFiPage {
    pub root: gtk::Box,
}

struct WiFiPageState {
    adapters_reconciler: Reconciler,
    known_group: KnownNetworksGroup,
    available_group: AvailableNetworksGroup,
}

impl WiFiPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let adapters_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();
        root.append(&adapters_box);

        let known_group = KnownNetworksGroup::new();
        root.append(&known_group.root);

        let available_group = AvailableNetworksGroup::new();
        root.append(&available_group.root);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-wifi");
            idx.add_section("wifi", &page_title, &t("wifi-known-networks"), "wifi-known-networks", &known_group.root);
            idx.add_section("wifi", &page_title, &t("wifi-available-networks"), "wifi-available-networks", &available_group.root);
        }

        // Wire available networks group scan button output
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            available_group.connect_output(move |output| {
                let adapters: Vec<(Urn, NetworkAdapter)> =
                    store.get_entities_typed(ADAPTER_ENTITY_TYPE);

                match output {
                    AvailableNetworksGroupOutput::Scan => {
                        for (urn, adapter) in &adapters {
                            if adapter.kind == AdapterKind::Wireless && adapter.enabled {
                                cb(
                                    urn.clone(),
                                    "scan".to_string(),
                                    serde_json::Value::Null,
                                );
                            }
                        }
                    }
                }
            });
        }

        let adapters_reconciler = Reconciler::new(adapters_box);

        let state = Rc::new(RefCell::new(WiFiPageState {
            adapters_reconciler,
            known_group,
            available_group,
        }));

        // Subscribe to adapter changes
        {
            let store = entity_store.clone();
            let network_store = entity_store.clone();
            let cb = action_callback.clone();
            let state = state.clone();
            entity_store.subscribe_type(ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, NetworkAdapter)> =
                    store.get_entities_typed(ADAPTER_ENTITY_TYPE);
                {
                    let mut st = state.borrow_mut();
                    st.adapters_reconciler.reconcile(
                        adapters.iter()
                            .filter(|(_, a)| a.kind == AdapterKind::Wireless)
                            .map(|(urn, adapter)| {
                                let urn_key = urn.as_str().to_string();
                                let urn = urn.clone();
                                let cb = cb.clone();
                                VNode::with_output::<WifiAdapterGroup>(
                                    WifiAdapterGroupProps {
                                        name:    adapter.name.clone(),
                                        enabled: adapter.enabled,
                                    },
                                    move |output| {
                                        let action = match output {
                                            WifiAdapterGroupOutput::Enable  => "activate",
                                            WifiAdapterGroupOutput::Disable => "deactivate",
                                        };
                                        cb(urn.clone(), action.to_string(), serde_json::Value::Null);
                                    },
                                )
                                .key(urn_key)
                            }),
                    );
                }
                // Also reconcile networks when adapter state changes (e.g. scanning state)
                let networks: Vec<(Urn, WiFiNetwork)> =
                    network_store.get_entities_typed(WiFiNetwork::ENTITY_TYPE);
                Self::reconcile_networks(&state, &networks, &adapters, &cb);
            });
        }

        // Subscribe to network changes
        {
            let store = entity_store.clone();
            let adapter_store = entity_store.clone();
            let cb = action_callback.clone();
            let state = state.clone();
            entity_store.subscribe_type(WiFiNetwork::ENTITY_TYPE, move || {
                let networks: Vec<(Urn, WiFiNetwork)> =
                    store.get_entities_typed(WiFiNetwork::ENTITY_TYPE);
                let adapters: Vec<(Urn, NetworkAdapter)> =
                    adapter_store.get_entities_typed(ADAPTER_ENTITY_TYPE);
                Self::reconcile_networks(&state, &networks, &adapters, &cb);
            });
        }

        // Trigger initial reconciliation with current cached data
        {
            let state_clone = state.clone();
            let cb_clone = action_callback.clone();
            let store_clone = entity_store.clone();

            gtk::glib::idle_add_local_once(move || {
                let adapters: Vec<(Urn, NetworkAdapter)> =
                    store_clone.get_entities_typed(ADAPTER_ENTITY_TYPE);
                let networks: Vec<(Urn, WiFiNetwork)> =
                    store_clone.get_entities_typed(WiFiNetwork::ENTITY_TYPE);

                if !adapters.is_empty() || !networks.is_empty() {
                    log::debug!(
                        "[wifi-page] Initial reconciliation: {} adapters, {} networks",
                        adapters.len(),
                        networks.len()
                    );
                    {
                        let mut st = state_clone.borrow_mut();
                        st.adapters_reconciler.reconcile(
                            adapters.iter()
                                .filter(|(_, a)| a.kind == AdapterKind::Wireless)
                                .map(|(urn, adapter)| {
                                    let urn_key = urn.as_str().to_string();
                                    let urn = urn.clone();
                                    let cb = cb_clone.clone();
                                    VNode::with_output::<WifiAdapterGroup>(
                                        WifiAdapterGroupProps {
                                            name:    adapter.name.clone(),
                                            enabled: adapter.enabled,
                                        },
                                        move |output| {
                                            let action = match output {
                                                WifiAdapterGroupOutput::Enable  => "activate",
                                                WifiAdapterGroupOutput::Disable => "deactivate",
                                            };
                                            cb(urn.clone(), action.to_string(), serde_json::Value::Null);
                                        },
                                    )
                                    .key(urn_key)
                                }),
                        );
                    }
                    Self::reconcile_networks(&state_clone, &networks, &adapters, &cb_clone);
                }
            });
        }

        Self { root }
    }

    fn reconcile_networks(
        state: &Rc<RefCell<WiFiPageState>>,
        networks: &[(Urn, WiFiNetwork)],
        adapters: &[(Urn, NetworkAdapter)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();

        let known: Vec<(Urn, WiFiNetwork)> =
            networks.iter().filter(|(_, n)| n.known).cloned().collect();

        let mut available: Vec<(Urn, WiFiNetwork)> = networks
            .iter()
            .filter(|(_, n)| !n.known && !n.connected)
            .cloned()
            .collect();

        // Sort available networks by strength descending
        available.sort_by(|(_, a), (_, b)| b.strength.cmp(&a.strength));

        let any_scanning = adapters
            .iter()
            .any(|(_, a)| a.kind == AdapterKind::Wireless && a.scanning);

        state.known_group.reconcile(&known, action_callback);
        state
            .available_group
            .reconcile(&available, any_scanning, action_callback);
    }
}
