//! Wired network settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `network-adapter` (wired) and `ethernet-connection`
//! entity types. On entity changes, reconciles adapter groups and connection lists.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::wired::adapter_group::{
    WiredAdapterGroup, WiredAdapterGroupOutput, WiredAdapterGroupProps,
};
use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::network::{
    ADAPTER_ENTITY_TYPE, AdapterKind, EthernetConnection, NetworkAdapter,
};

/// Smart container for the Wired network settings page.
pub struct WiredPage {
    pub root: gtk::Box,
}

struct WiredPageState {
    adapters_box: gtk::Box,
    adapters: HashMap<String, WiredAdapterGroup>,
}

impl WiredPage {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-wired");
        idx.add_section_deferred(
            "wired",
            &page_title,
            &t("wired-ip-address"),
            "wired-ip-address",
        );
    }

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

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            idx.backfill_widget("wired", &t("wired-ip-address"), None, Some(&adapters_box));
        }

        let state = Rc::new(RefCell::new(WiredPageState {
            adapters_box,
            adapters: HashMap::new(),
        }));

        // Subscribe to both adapter and connection changes
        crate::subscription::subscribe_dual_entities::<NetworkAdapter, EthernetConnection, _>(
            entity_store,
            ADAPTER_ENTITY_TYPE,
            EthernetConnection::ENTITY_TYPE,
            {
                let state = state.clone();
                let cb = action_callback.clone();
                move |adapters, connections| {
                    log::debug!(
                        "[wired-page] Reconciling: {} adapters, {} connections",
                        adapters.len(),
                        connections.len()
                    );
                    Self::reconcile(&state, &adapters, &connections, &cb);
                }
            },
        );

        Self { root }
    }

    fn reconcile(
        state: &Rc<RefCell<WiredPageState>>,
        adapters: &[(Urn, NetworkAdapter)],
        connections: &[(Urn, EthernetConnection)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();
        let wired: Vec<_> = adapters
            .iter()
            .filter(|(_, adapter)| adapter.kind == AdapterKind::Wired)
            .collect();
        let ordered_keys: Vec<String> = wired
            .iter()
            .map(|(urn, _)| urn.as_str().to_string())
            .collect();
        let seen: std::collections::HashSet<String> = ordered_keys.iter().cloned().collect();

        for (urn, adapter) in wired {
            let key = urn.as_str().to_string();
            let adapter_connections: Vec<(Urn, EthernetConnection)> = connections
                .iter()
                .filter(|(conn_urn, _)| conn_urn.as_str().starts_with(key.as_str()))
                .cloned()
                .collect();
            let props = WiredAdapterGroupProps {
                name: adapter.name.clone(),
                connected: adapter.connected,
                ip: adapter.ip.clone(),
                public_ip: adapter.public_ip.clone(),
                connections: adapter_connections,
            };
            if let Some(existing) = state.adapters.get(&key) {
                existing.update(&props);
            } else {
                let group = WiredAdapterGroup::build(&props);
                let adapter_urn = urn.clone();
                let cb = action_callback.clone();
                group.connect_output(move |output| match output {
                    WiredAdapterGroupOutput::ToggleConnection => {
                        cb(
                            adapter_urn.clone(),
                            "activate".to_string(),
                            serde_json::Value::Null,
                        );
                    }
                    WiredAdapterGroupOutput::ActivateConnection(conn_urn) => {
                        cb(conn_urn, "activate".to_string(), serde_json::Value::Null);
                    }
                    WiredAdapterGroupOutput::DeactivateConnection(conn_urn) => {
                        cb(conn_urn, "deactivate".to_string(), serde_json::Value::Null);
                    }
                });
                state.adapters_box.append(&group.widget());
                state.adapters.insert(key, group);
            }
        }

        let stale: Vec<String> = state
            .adapters
            .keys()
            .filter(|key| !seen.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(group) = state.adapters.remove(&key) {
                state.adapters_box.remove(&group.widget());
            }
        }
        let mut previous: Option<gtk::Widget> = None;
        for key in ordered_keys {
            if let Some(widget) = state.adapters.get(&key).map(WiredAdapterGroup::widget) {
                widget.insert_after(&state.adapters_box, previous.as_ref());
                previous = Some(widget);
            }
        }
    }
}
