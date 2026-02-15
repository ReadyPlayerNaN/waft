//! Bluetooth settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `bluetooth-adapter` and `bluetooth-device`
//! entity types. On entity changes, reconciles adapter groups and device lists.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::bluetooth::{BluetoothAdapter, BluetoothDevice};

use crate::bluetooth::adapter_group::{AdapterGroup, AdapterGroupOutput, AdapterGroupProps};
use crate::bluetooth::discovered_devices_group::DiscoveredDevicesGroup;
use crate::bluetooth::paired_devices_group::PairedDevicesGroup;

/// Smart container for the Bluetooth settings page.
///
/// Owns adapter groups and device groups. Subscribes to EntityStore
/// and updates widgets when entity data changes.
pub struct BluetoothPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Bluetooth page.
struct BluetoothPageState {
    adapter_groups: HashMap<String, AdapterGroup>,
    paired_group: PairedDevicesGroup,
    discovered_group: DiscoveredDevicesGroup,
    adapters_box: gtk::Box,
}

impl BluetoothPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Container for adapter groups (one per adapter)
        let adapters_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();
        root.append(&adapters_box);

        // Paired devices group
        let paired_group = PairedDevicesGroup::new();
        root.append(&paired_group.root);

        // Discovered devices group
        let discovered_group = DiscoveredDevicesGroup::new();
        root.append(&discovered_group.root);

        let state = Rc::new(RefCell::new(BluetoothPageState {
            adapter_groups: HashMap::new(),
            paired_group,
            discovered_group,
            adapters_box,
        }));

        // Subscribe to adapter changes
        {
            let store = entity_store.clone();
            let device_store = entity_store.clone();
            let cb = action_callback.clone();
            let state = state.clone();
            entity_store.subscribe_type(BluetoothAdapter::ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, BluetoothAdapter)> =
                    store.get_entities_typed(BluetoothAdapter::ENTITY_TYPE);
                log::debug!(
                    "[bluetooth-page] Adapter subscription triggered: {} adapters",
                    adapters.len()
                );
                Self::reconcile_adapters(&state, &adapters, &cb);
                let devices: Vec<(Urn, BluetoothDevice)> =
                    device_store.get_entities_typed(BluetoothDevice::ENTITY_TYPE);
                Self::reconcile_devices(&state, &devices, &adapters, &cb);
            });
        }

        // Subscribe to device changes
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let adapter_store = entity_store.clone();
            let state = state.clone();
            entity_store.subscribe_type(BluetoothDevice::ENTITY_TYPE, move || {
                let devices: Vec<(Urn, BluetoothDevice)> =
                    store.get_entities_typed(BluetoothDevice::ENTITY_TYPE);
                log::debug!(
                    "[bluetooth-page] Device subscription triggered: {} devices",
                    devices.len()
                );
                let adapters: Vec<(Urn, BluetoothAdapter)> =
                    adapter_store.get_entities_typed(BluetoothAdapter::ENTITY_TYPE);
                Self::reconcile_devices(&state, &devices, &adapters, &cb);
            });
        }

        // Trigger initial reconciliation with current cached data.
        // EntityStore::subscribe_type() only fires on changes, not on initial
        // subscription. If EntityUpdated notifications arrived before subscriptions
        // were registered, the UI never reconciles with cached data.
        {
            let state_clone = state.clone();
            let cb_clone = action_callback.clone();
            let store_clone = entity_store.clone();

            gtk::glib::idle_add_local_once(move || {
                let adapters: Vec<(Urn, BluetoothAdapter)> =
                    store_clone.get_entities_typed(BluetoothAdapter::ENTITY_TYPE);
                let devices: Vec<(Urn, BluetoothDevice)> =
                    store_clone.get_entities_typed(BluetoothDevice::ENTITY_TYPE);

                if !adapters.is_empty() || !devices.is_empty() {
                    log::debug!(
                        "[bluetooth-page] Initial reconciliation: {} adapters, {} devices",
                        adapters.len(),
                        devices.len()
                    );
                    Self::reconcile_adapters(&state_clone, &adapters, &cb_clone);
                    Self::reconcile_devices(&state_clone, &devices, &adapters, &cb_clone);
                }
            });
        }

        Self { root }
    }

    /// Reconcile adapter groups with current adapter data.
    fn reconcile_adapters(
        state: &Rc<RefCell<BluetoothPageState>>,
        adapters: &[(Urn, BluetoothAdapter)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();
        let mut seen = std::collections::HashSet::new();

        for (urn, adapter) in adapters {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let props = AdapterGroupProps {
                name: adapter.name.clone(),
                powered: adapter.powered,
                discoverable: adapter.discoverable,
                discovering: adapter.discovering,
            };

            if let Some(existing) = state.adapter_groups.get(&urn_str) {
                existing.apply_props(&props);
            } else {
                let group = AdapterGroup::new(&props);
                let urn_clone = urn.clone();
                let cb = action_callback.clone();
                group.connect_output(move |output| {
                    let (action, params) = match output {
                        AdapterGroupOutput::TogglePower => {
                            ("toggle-power", serde_json::Value::Null)
                        }
                        AdapterGroupOutput::ToggleDiscoverable => {
                            ("toggle-discoverable", serde_json::Value::Null)
                        }
                        AdapterGroupOutput::SetAlias(alias) => {
                            ("set-alias", serde_json::json!({ "alias": alias }))
                        }
                        AdapterGroupOutput::StartDiscovery => {
                            ("start-discovery", serde_json::Value::Null)
                        }
                        AdapterGroupOutput::StopDiscovery => {
                            ("stop-discovery", serde_json::Value::Null)
                        }
                    };
                    cb(urn_clone.clone(), action.to_string(), params);
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
            .reconcile(&discovered, any_discovering, action_callback);
    }
}
