//! Wired network settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `network-adapter` (wired) and `ethernet-connection`
//! entity types. On entity changes, reconciles adapter groups and connection lists.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
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
    adapter_groups: HashMap<String, WiredAdapterGroup>,
    adapters_box: gtk::Box,
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

        let state = Rc::new(RefCell::new(WiredPageState {
            adapter_groups: HashMap::new(),
            adapters_box,
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
                Self::reconcile_adapters(&state, &adapters, &cb);
                // Also reconcile connections when adapter state changes
                let connections: Vec<(Urn, EthernetConnection)> =
                    conn_store.get_entities_typed(EthernetConnection::ENTITY_TYPE);
                Self::reconcile_connections(&state, &connections, &adapters, &cb);
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
                Self::reconcile_connections(&state, &connections, &adapters, &cb);
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
                    Self::reconcile_adapters(&state_clone, &adapters, &cb_clone);
                    Self::reconcile_connections(&state_clone, &connections, &adapters, &cb_clone);
                }
            });
        }

        Self { root }
    }

    fn reconcile_adapters(
        state: &Rc<RefCell<WiredPageState>>,
        adapters: &[(Urn, NetworkAdapter)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();
        let mut seen = std::collections::HashSet::new();

        let wired_adapters: Vec<_> = adapters
            .iter()
            .filter(|(_, a)| a.kind == AdapterKind::Wired)
            .collect();

        for (urn, adapter) in &wired_adapters {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let props = WiredAdapterGroupProps {
                name: adapter.name.clone(),
                connected: adapter.connected,
                ip: adapter.ip.clone(),
                public_ip: adapter.public_ip.clone(),
            };

            if let Some(existing) = state.adapter_groups.get(&urn_str) {
                existing.apply_props(&props);
            } else {
                let group = WiredAdapterGroup::new(&props);
                let urn_clone = (*urn).clone();
                let cb = action_callback.clone();
                group.connect_output(move |output| {
                    let action = match output {
                        WiredAdapterGroupOutput::ToggleConnection => {
                            // Toggle based on current state — the daemon handles the logic
                            "activate"
                        }
                    };
                    cb(
                        urn_clone.clone(),
                        action.to_string(),
                        serde_json::Value::Null,
                    );
                });
                state.adapters_box.append(&group.root);
                state.adapter_groups.insert(urn_str, group);
            }
        }

        // Remove adapter groups no longer present
        let to_remove: Vec<String> = state
            .adapter_groups
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(group) = state.adapter_groups.remove(&key) {
                state.adapters_box.remove(&group.root);
            }
        }
    }

    fn reconcile_connections(
        state: &Rc<RefCell<WiredPageState>>,
        connections: &[(Urn, EthernetConnection)],
        adapters: &[(Urn, NetworkAdapter)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();

        // Group connections by parent adapter URN
        // Connection URN format: networkmanager/network-adapter/{adapter}/ethernet-connection/{uuid}
        // Adapter URN format: networkmanager/network-adapter/{adapter}
        for (adapter_urn, _adapter) in adapters
            .iter()
            .filter(|(_, a)| a.kind == AdapterKind::Wired)
        {
            let adapter_urn_str = adapter_urn.as_str();
            if let Some(group) = state.adapter_groups.get_mut(adapter_urn_str) {
                let adapter_connections: Vec<(Urn, EthernetConnection)> = connections
                    .iter()
                    .filter(|(urn, _)| urn.as_str().starts_with(adapter_urn_str))
                    .cloned()
                    .collect();
                group.reconcile_connections(&adapter_connections, action_callback);
            }
        }
    }
}
