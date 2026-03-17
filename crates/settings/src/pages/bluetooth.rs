//! Bluetooth settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `bluetooth-adapter` and `bluetooth-device`
//! entity types. On entity changes, reconciles adapter groups and device lists.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::bluetooth::{BluetoothAdapter, BluetoothDevice};
use waft_ui_gtk::vdom::{Reconciler, VNode};

use crate::bluetooth::adapter_group::{AdapterGroup, AdapterGroupOutput, AdapterGroupProps};
use crate::bluetooth::discovered_devices_group::{
    DiscoveredDevicesGroup, DiscoveredDevicesGroupOutput,
};
use crate::bluetooth::paired_devices_group::PairedDevicesGroup;
use crate::i18n::t;
use crate::search_index::SearchIndex;

/// Smart container for the Bluetooth settings page.
///
/// Owns adapter groups and device groups. Subscribes to EntityStore
/// and updates widgets when entity data changes.
pub struct BluetoothPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Bluetooth page.
struct BluetoothPageState {
    adapters_reconciler: Reconciler,
    paired_group: PairedDevicesGroup,
    discovered_group: DiscoveredDevicesGroup,
}

impl BluetoothPage {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-bluetooth");
        idx.add_section_deferred("bluetooth", &page_title, &t("bt-paired-devices"), "bt-paired-devices");
        idx.add_section_deferred("bluetooth", &page_title, &t("bt-available-devices"), "bt-available-devices");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        // Container for adapter groups (one per adapter)
        let adapters_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();
        root.append(&adapters_box);
        let adapters_reconciler = Reconciler::new(adapters_box);

        // Paired devices group
        let paired_group = PairedDevicesGroup::new();
        root.append(&paired_group.root);

        // Discovered devices group
        let discovered_group = DiscoveredDevicesGroup::new();
        root.append(&discovered_group.root);

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            idx.backfill_widget("bluetooth", &t("bt-paired-devices"), None, Some(&paired_group.root));
            idx.backfill_widget("bluetooth", &t("bt-available-devices"), None, Some(&discovered_group.root));
        }

        // Wire discovered group search button output
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            discovered_group.connect_output(move |output| {
                let adapters: Vec<(Urn, BluetoothAdapter)> =
                    store.get_entities_typed(BluetoothAdapter::ENTITY_TYPE);

                match output {
                    DiscoveredDevicesGroupOutput::StartDiscovery => {
                        for (urn, adapter) in &adapters {
                            if adapter.powered && !adapter.discovering {
                                cb(
                                    urn.clone(),
                                    "start-discovery".to_string(),
                                    serde_json::Value::Null,
                                );
                            }
                        }
                    }
                    DiscoveredDevicesGroupOutput::StopDiscovery => {
                        for (urn, adapter) in &adapters {
                            if adapter.discovering {
                                cb(
                                    urn.clone(),
                                    "stop-discovery".to_string(),
                                    serde_json::Value::Null,
                                );
                            }
                        }
                    }
                }
            });
        }

        let state = Rc::new(RefCell::new(BluetoothPageState {
            adapters_reconciler,
            paired_group,
            discovered_group,
        }));

        // Subscribe to both adapter and device changes
        crate::subscription::subscribe_dual_entities::<BluetoothAdapter, BluetoothDevice, _>(
            entity_store,
            BluetoothAdapter::ENTITY_TYPE,
            BluetoothDevice::ENTITY_TYPE,
            {
                let state = state.clone();
                let cb = action_callback.clone();
                move |adapters, devices| {
                    log::debug!(
                        "[bluetooth-page] Reconciling: {} adapters, {} devices",
                        adapters.len(),
                        devices.len()
                    );
                    Self::reconcile_adapters(&state, &adapters, &cb);
                    Self::reconcile_devices(&state, &devices, &adapters, &cb);
                }
            },
        );

        Self { root }
    }

    /// Reconcile adapter groups with current adapter data.
    fn reconcile_adapters(
        state: &Rc<RefCell<BluetoothPageState>>,
        adapters: &[(Urn, BluetoothAdapter)],
        action_callback: &EntityActionCallback,
    ) {
        let mut st = state.borrow_mut();
        st.adapters_reconciler.reconcile(adapters.iter().map(|(urn, adapter)| {
            let key = urn.as_str().to_string();
            let urn = urn.clone();
            let cb = action_callback.clone();
            VNode::with_output::<AdapterGroup>(
                AdapterGroupProps {
                    name:         adapter.name.clone(),
                    powered:      adapter.powered,
                    discoverable: adapter.discoverable,
                },
                move |output| {
                    let (action, params) = match output {
                        AdapterGroupOutput::TogglePower =>
                            ("toggle-power", serde_json::Value::Null),
                        AdapterGroupOutput::ToggleDiscoverable =>
                            ("toggle-discoverable", serde_json::Value::Null),
                        AdapterGroupOutput::SetAlias(alias) =>
                            ("set-alias", serde_json::json!({ "alias": alias })),
                    };
                    cb(urn.clone(), action.to_string(), params);
                },
            )
            .key(key)
        }));
    }

    /// Reconcile device lists with current device data.
    fn reconcile_devices(
        state: &Rc<RefCell<BluetoothPageState>>,
        devices: &[(Urn, BluetoothDevice)],
        adapters: &[(Urn, BluetoothAdapter)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();

        // Partition devices into paired and discovered
        let paired: Vec<(Urn, BluetoothDevice)> =
            devices.iter().filter(|(_, d)| d.paired).cloned().collect();

        let discovered: Vec<(Urn, BluetoothDevice)> =
            devices.iter().filter(|(_, d)| !d.paired).cloned().collect();

        let any_discovering = adapters.iter().any(|(_, a)| a.discovering);
        let any_powered = adapters.iter().any(|(_, a)| a.powered);

        log::debug!(
            "[bluetooth-page] reconcile_devices: {} total, {} paired, {} discovered, discovering={}",
            devices.len(),
            paired.len(),
            discovered.len(),
            any_discovering,
        );

        state.paired_group.reconcile(&paired, action_callback);
        state
            .discovered_group
            .reconcile(&discovered, any_discovering, any_powered, action_callback);
    }
}
