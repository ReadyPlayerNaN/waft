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
use crate::wifi::password_dialog::show_password_dialog;

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
        let root = crate::page_layout::page_root();

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
            let connect_cb = action_callback.clone();
            let root_for_dialog = root.clone();
            available_group.connect_output(move |output| {
                match output {
                    AvailableNetworksGroupOutput::Scan => {
                        let adapters: Vec<(Urn, NetworkAdapter)> =
                            store.get_entities_typed(ADAPTER_ENTITY_TYPE);
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
                    AvailableNetworksGroupOutput::ConnectWithPassword { urn, ssid } => {
                        let connect_cb = connect_cb.clone();
                        let urn = urn.clone();
                        show_password_dialog(&root_for_dialog, &ssid, move |password| {
                            connect_cb(
                                urn.clone(),
                                "connect".to_string(),
                                serde_json::json!({ "password": password }),
                            );
                        });
                    }
                }
            });
        }

        // Handle action errors (e.g., wrong password, enterprise not supported)
        {
            entity_store.on_action_error(move |_action_id, error| {
                match error.as_str() {
                    "password-required" => {
                        log::warn!("[wifi-page] unexpected password-required from plugin");
                    }
                    "enterprise-not-supported" => {
                        log::info!("[wifi-page] enterprise network not supported");
                    }
                    _ => {
                        log::warn!("[wifi-page] action error: {error}");
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

        // Subscribe to both adapter and network changes
        crate::subscription::subscribe_dual_entities::<NetworkAdapter, WiFiNetwork, _>(
            entity_store,
            ADAPTER_ENTITY_TYPE,
            WiFiNetwork::ENTITY_TYPE,
            {
                let state = state.clone();
                let cb = action_callback.clone();
                move |adapters, networks| {
                    log::debug!(
                        "[wifi-page] Reconciling: {} adapters, {} networks",
                        adapters.len(),
                        networks.len()
                    );
                    Self::reconcile_adapters(&state, &adapters, &cb);
                    Self::reconcile_networks(&state, &networks, &adapters, &cb);
                }
            },
        );

        Self { root }
    }

    /// Reconcile adapter groups with current adapter data.
    fn reconcile_adapters(
        state: &Rc<RefCell<WiFiPageState>>,
        adapters: &[(Urn, NetworkAdapter)],
        action_callback: &EntityActionCallback,
    ) {
        let mut st = state.borrow_mut();
        st.adapters_reconciler.reconcile(
            adapters.iter()
                .filter(|(_, a)| a.kind == AdapterKind::Wireless)
                .map(|(urn, adapter)| {
                    let urn_key = urn.as_str().to_string();
                    let urn = urn.clone();
                    let cb = action_callback.clone();
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
