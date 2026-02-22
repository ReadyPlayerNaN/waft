//! Wired network settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `network-adapter` (wired) and `ethernet-connection`
//! entity types. On entity changes, reconciles adapter groups and connection lists.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_ui_gtk::vdom::{Reconciler, VNode};
use waft_protocol::Urn;
use waft_protocol::entity::network::{
    ADAPTER_ENTITY_TYPE, AdapterKind, EthernetConnection, NetworkAdapter,
};

use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::wired::adapter_group::{
    WiredAdapterGroup, WiredAdapterGroupOutput, WiredAdapterGroupProps,
};

/// Smart container for the Wired network settings page.
pub struct WiredPage {
    pub root: gtk::Box,
}

struct WiredPageState {
    adapters_reconciler: Reconciler,
}

impl WiredPage {
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

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-wired");
            idx.add_section("wired", &page_title, &t("wired-ip-address"), "wired-ip-address", &adapters_box);
        }

        let adapters_reconciler = Reconciler::new(adapters_box);

        let state = Rc::new(RefCell::new(WiredPageState {
            adapters_reconciler,
        }));

        // Subscribe to adapter changes
        {
            let store = entity_store.clone();
            let conn_store = entity_store.clone();
            let cb = action_callback.clone();
            let state = state.clone();
            entity_store.subscribe_type(ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, NetworkAdapter)> =
                    store.get_entities_typed(ADAPTER_ENTITY_TYPE);
                let connections: Vec<(Urn, EthernetConnection)> =
                    conn_store.get_entities_typed(EthernetConnection::ENTITY_TYPE);
                Self::reconcile(&state, &adapters, &connections, &cb);
            });
        }

        // Subscribe to connection changes
        {
            let store = entity_store.clone();
            let adapter_store = entity_store.clone();
            let cb = action_callback.clone();
            let state = state.clone();
            entity_store.subscribe_type(EthernetConnection::ENTITY_TYPE, move || {
                let connections: Vec<(Urn, EthernetConnection)> =
                    store.get_entities_typed(EthernetConnection::ENTITY_TYPE);
                let adapters: Vec<(Urn, NetworkAdapter)> =
                    adapter_store.get_entities_typed(ADAPTER_ENTITY_TYPE);
                Self::reconcile(&state, &adapters, &connections, &cb);
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
                let connections: Vec<(Urn, EthernetConnection)> =
                    store_clone.get_entities_typed(EthernetConnection::ENTITY_TYPE);

                if !adapters.is_empty() || !connections.is_empty() {
                    log::debug!(
                        "[wired-page] Initial reconciliation: {} adapters, {} connections",
                        adapters.len(),
                        connections.len()
                    );
                    Self::reconcile(&state_clone, &adapters, &connections, &cb_clone);
                }
            });
        }

        Self { root }
    }

    fn reconcile(
        state: &Rc<RefCell<WiredPageState>>,
        adapters: &[(Urn, NetworkAdapter)],
        connections: &[(Urn, EthernetConnection)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();

        state.adapters_reconciler.reconcile(
            adapters
                .iter()
                .filter(|(_, a)| a.kind == AdapterKind::Wired)
                .map(|(urn, adapter)| {
                    let urn_key    = urn.as_str().to_string();
                    let urn_clone  = urn.clone();
                    let cb         = action_callback.clone();
                    let adapter_urn_str = urn.as_str().to_string();

                    // Collect connection profiles that belong to this adapter.
                    // Connection URN format:
                    //   networkmanager/network-adapter/{adapter}/ethernet-connection/{uuid}
                    // Adapter URN format:
                    //   networkmanager/network-adapter/{adapter}
                    let adapter_connections: Vec<(Urn, EthernetConnection)> = connections
                        .iter()
                        .filter(|(conn_urn, _)| conn_urn.as_str().starts_with(&adapter_urn_str))
                        .cloned()
                        .collect();

                    VNode::with_output::<WiredAdapterGroup>(
                        WiredAdapterGroupProps {
                            name:        adapter.name.clone(),
                            connected:   adapter.connected,
                            ip:          adapter.ip.clone(),
                            public_ip:   adapter.public_ip.clone(),
                            connections: adapter_connections,
                        },
                        move |output| {
                            match output {
                                WiredAdapterGroupOutput::ToggleConnection => {
                                    cb(
                                        urn_clone.clone(),
                                        "activate".to_string(),
                                        serde_json::Value::Null,
                                    );
                                }
                                WiredAdapterGroupOutput::ActivateConnection(conn_urn) => {
                                    cb(
                                        conn_urn,
                                        "activate".to_string(),
                                        serde_json::Value::Null,
                                    );
                                }
                                WiredAdapterGroupOutput::DeactivateConnection(conn_urn) => {
                                    cb(
                                        conn_urn,
                                        "deactivate".to_string(),
                                        serde_json::Value::Null,
                                    );
                                }
                            }
                        },
                    )
                    .key(urn_key)
                }),
        );
    }
}
