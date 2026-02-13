//! Bluetooth adapter toggle components.
//!
//! Subscribes to the `bluetooth-adapter` entity type and dynamically creates
//! one FeatureToggleWidget per adapter. Adapters that appear or disappear are
//! tracked and the toggle set is kept in sync.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::menu_state::menu_id_for_widget;
use waft_ui_gtk::bluetooth::device_row::{
    battery_icon_name, device_type_icon, BluetoothDeviceRow, BluetoothDeviceRowOutput,
    BluetoothDeviceRowProps,
};
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// A tracked toggle entry for a single Bluetooth adapter.
struct ToggleEntry {
    urn_str: String,
    toggle: Rc<FeatureToggleWidget>,
    menu: gtk::Box,
    device_rows: RefCell<Vec<DeviceRow>>,
}

/// A single device row wrapper holding the URN and the extracted widget.
struct DeviceRow {
    urn_str: String,
    row: Rc<BluetoothDeviceRow>,
}

/// Dynamic set of toggles for Bluetooth adapters (0..N).
///
/// Maintains one FeatureToggleWidget per adapter entity. When the entity set
/// changes, existing toggles are updated in place and new ones are created
/// or stale ones removed.
pub struct BluetoothToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    store: Rc<EntityStore>,
    action_callback: EntityActionCallback,
    menu_store: Rc<waft_core::menu_state::MenuStore>,
}

impl BluetoothToggles {
    /// Create a new BluetoothToggles that subscribes to the entity store.
    ///
    /// `rebuild_callback` is invoked whenever the set of toggles changes
    /// (adapter added or removed) so the parent grid can rebuild.
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));

        let store_ref = store.clone();
        let entries_ref = entries.clone();
        let cb = action_callback.clone();
        let rebuild = rebuild_callback.clone();
        let menu_store_ref = menu_store.clone();

        store.subscribe_type(entity::bluetooth::BluetoothAdapter::ENTITY_TYPE, move || {
            let adapters: Vec<(Urn, entity::bluetooth::BluetoothAdapter)> =
                store_ref.get_entities_typed(entity::bluetooth::BluetoothAdapter::ENTITY_TYPE);

            let mut entries_mut = entries_ref.borrow_mut();
            let mut changed = false;

            // Build a set of current URN strings for quick lookup
            let current_urns: Vec<String> = adapters
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();

            // Remove toggles for adapters that no longer exist
            let before_len = entries_mut.len();
            entries_mut.retain(|entry| current_urns.contains(&entry.urn_str));
            if entries_mut.len() != before_len {
                changed = true;
            }

            // Update existing or create new toggles
            for (urn, adapter) in &adapters {
                let urn_str = urn.as_str().to_string();
                let icon = if adapter.powered {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-disabled-symbolic"
                };

                if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == urn_str) {
                    // Update existing toggle
                    entry.toggle.set_active(adapter.powered);
                    entry.toggle.set_icon(icon);
                    entry.toggle.set_details(Some(adapter.name.clone()));
                } else {
                    // Create new toggle for this adapter
                    let widget_id = format!("bluetooth-toggle-{}", urn_str);
                    let menu_id = menu_id_for_widget(&widget_id);

                    // Create menu container for devices
                    let menu = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(0)
                        .css_classes(["menu-content"])
                        .build();

                    let toggle = Rc::new(FeatureToggleWidget::new(
                        FeatureToggleProps {
                            active: adapter.powered,
                            busy: false,
                            details: Some(adapter.name.clone()),
                            expandable: false, // Will be updated based on device count
                            icon: icon.to_string(),
                            title: "Bluetooth".to_string(),
                            menu_id: Some(menu_id.clone()),
                        },
                        Some(menu_store_ref.clone()),
                    ));

                    let action_cb = cb.clone();
                    let action_urn = urn.clone();
                    toggle.connect_output(move |_output| {
                        action_cb(
                            action_urn.clone(),
                            "toggle-power".to_string(),
                            serde_json::Value::Null,
                        );
                    });

                    entries_mut.push(ToggleEntry {
                        urn_str,
                        toggle,
                        menu,
                        device_rows: RefCell::new(Vec::new()),
                    });
                    changed = true;
                }
            }

            // Notify the parent grid if the toggle set changed
            if changed {
                drop(entries_mut);
                rebuild();
            }
        });

        // Subscribe to device entity changes
        let entries_ref_devices = entries.clone();
        let store_ref_devices = store.clone();
        let cb_devices = action_callback.clone();
        store.subscribe_type(entity::bluetooth::BluetoothDevice::ENTITY_TYPE, move || {
            Self::update_device_menus(
                &entries_ref_devices,
                &store_ref_devices,
                &cb_devices,
            );
        });

        Self {
            entries,
            store: store.clone(),
            action_callback: action_callback.clone(),
            menu_store: menu_store.clone(),
        }
    }

    /// Update device menus for all adapters based on current device entities.
    fn update_device_menus(
        entries: &Rc<RefCell<Vec<ToggleEntry>>>,
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
    ) {
        let devices: Vec<(Urn, entity::bluetooth::BluetoothDevice)> =
            store.get_entities_typed(entity::bluetooth::BluetoothDevice::ENTITY_TYPE);

        let entries_mut = entries.borrow();

        for entry in entries_mut.iter() {
            // Find devices for this adapter by checking URN prefix
            let adapter_urn_prefix = format!("{}/", entry.urn_str);
            let adapter_devices: Vec<_> = devices
                .iter()
                .filter(|(urn, _)| urn.as_str().starts_with(&adapter_urn_prefix))
                .collect();

            // Update toggle expandable state based on device count
            entry.toggle.set_expandable(!adapter_devices.is_empty());

            // Update details text
            let connected_count = adapter_devices.iter().filter(|(_, d)| d.connected()).count();
            if connected_count > 0 {
                entry
                    .toggle
                    .set_details(Some(format!("{} connected", connected_count)));
            } else if !adapter_devices.is_empty() {
                entry
                    .toggle
                    .set_details(Some(format!("{} paired", adapter_devices.len())));
            } else {
                entry.toggle.set_details(None);
            }

            // Update device rows
            let mut device_rows = entry.device_rows.borrow_mut();

            // Remove rows for devices that no longer exist
            let current_device_urns: Vec<String> = adapter_devices
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();
            device_rows.retain(|row| {
                if current_device_urns.contains(&row.urn_str) {
                    true
                } else {
                    row.row.root.unparent();
                    false
                }
            });

            // Update or create rows for each device
            for (device_urn, device) in &adapter_devices {
                let device_urn_str = device_urn.as_str().to_string();

                if let Some(row) = device_rows.iter().find(|r| r.urn_str == device_urn_str) {
                    // Update existing row via setters
                    row.row.set_name(&device.name);
                    row.row.set_device_icon(device_type_icon(&device.device_type));
                    row.row.set_battery_icon(
                        device.battery_percentage.map(battery_icon_name),
                    );
                    row.row.set_connected(device.connected());
                    row.row.set_transitioning(device.transitioning());
                } else {
                    // Create new device row
                    let bt_row = Rc::new(BluetoothDeviceRow::new(BluetoothDeviceRowProps {
                        device_icon: device_type_icon(&device.device_type).to_string(),
                        name: device.name.clone(),
                        battery_icon: device
                            .battery_percentage
                            .map(|pct| battery_icon_name(pct).to_string()),
                        connected: device.connected(),
                        transitioning: device.transitioning(),
                    }));

                    let action_cb = action_callback.clone();
                    let urn_for_click = (*device_urn).clone();
                    bt_row.connect_output(move |BluetoothDeviceRowOutput::ToggleConnect| {
                        action_cb(
                            urn_for_click.clone(),
                            "toggle-connect".to_string(),
                            serde_json::Value::Null,
                        );
                    });

                    entry.menu.append(&bt_row.root);
                    device_rows.push(DeviceRow {
                        urn_str: device_urn_str,
                        row: bt_row,
                    });
                }
            }
        }
    }

    /// Return all current toggles as feature toggle widgets for the grid.
    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        self.entries
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                Rc::new(WidgetFeatureToggle {
                    id: format!("bluetooth-toggle-{}", entry.urn_str),
                    weight: 500 + i as i32,
                    toggle: (*entry.toggle).clone(),
                    menu: Some(entry.menu.clone().upcast::<gtk::Widget>()),
                })
            })
            .collect()
    }
}
