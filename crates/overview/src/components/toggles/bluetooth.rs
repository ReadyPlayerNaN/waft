//! Bluetooth adapter toggle components.
//!
//! Subscribes to the `bluetooth-adapter` entity type and dynamically creates
//! one FeatureToggleWidget per adapter. Adapters that appear or disappear are
//! tracked and the toggle set is kept in sync.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::menu_state::menu_id_for_widget;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};
use waft_ui_gtk::widgets::icon::IconWidget;

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// A tracked toggle entry for a single Bluetooth adapter.
struct ToggleEntry {
    urn_str: String,
    toggle: Rc<FeatureToggleWidget>,
    menu: gtk::Box,
    device_rows: RefCell<Vec<DeviceRow>>,
}

/// A single device row in the menu with device type icon, battery icon,
/// name label, spinner for transitioning state, and a connection switch.
struct DeviceRow {
    urn_str: String,
    root: gtk::Button,
    name_label: gtk::Label,
    device_icon: IconWidget,
    battery_icon: IconWidget,
    battery_icon_widget: gtk::Widget,
    spinner: gtk::Spinner,
    switch: gtk::Switch,
    /// Last known connected state, used to detect transition completion.
    last_connected: RefCell<bool>,
}

/// Resolve device_type string to a themed icon name.
fn device_type_icon(device_type: &str) -> &'static str {
    match device_type {
        "audio-headphones" => "audio-headphones-symbolic",
        "audio-headset" => "audio-headset-symbolic",
        "input-mouse" => "input-mouse-symbolic",
        "input-keyboard" => "input-keyboard-symbolic",
        "phone" => "phone-symbolic",
        "computer" => "computer-symbolic",
        _ => "bluetooth-symbolic",
    }
}

/// Pick a battery icon name based on percentage.
fn battery_icon_name(pct: u8) -> &'static str {
    match pct {
        0..=10 => "battery-level-0-symbolic",
        11..=30 => "battery-caution-symbolic",
        31..=50 => "battery-level-30-symbolic",
        51..=70 => "battery-level-50-symbolic",
        71..=90 => "battery-level-70-symbolic",
        _ => "battery-full-symbolic",
    }
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
        // URNs currently transitioning (connecting or disconnecting).
        let transitioning: Rc<RefCell<HashSet<String>>> =
            Rc::new(RefCell::new(HashSet::new()));

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
        let transitioning_devices = transitioning.clone();
        store.subscribe_type(entity::bluetooth::BluetoothDevice::ENTITY_TYPE, move || {
            Self::update_device_menus(
                &entries_ref_devices,
                &store_ref_devices,
                &cb_devices,
                &transitioning_devices,
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
        transitioning: &Rc<RefCell<HashSet<String>>>,
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
            let connected_count = adapter_devices.iter().filter(|(_, d)| d.connected).count();
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
                    transitioning.borrow_mut().remove(&row.urn_str);
                    row.root.unparent();
                    false
                }
            });

            // Update or create rows for each device
            for (device_urn, device) in &adapter_devices {
                let device_urn_str = device_urn.as_str().to_string();

                if let Some(row) = device_rows.iter().find(|r| r.urn_str == device_urn_str) {
                    // Check if connected state changed while transitioning
                    let prev_connected = *row.last_connected.borrow();
                    if prev_connected != device.connected {
                        transitioning.borrow_mut().remove(&device_urn_str);
                    }
                    *row.last_connected.borrow_mut() = device.connected;

                    let is_transitioning = transitioning.borrow().contains(&device_urn_str);

                    // Update existing row
                    row.name_label.set_label(&device.name);
                    row.device_icon.set_icon(device_type_icon(&device.device_type));

                    // Battery icon
                    if let Some(pct) = device.battery_percentage {
                        row.battery_icon.set_icon(battery_icon_name(pct));
                        row.battery_icon_widget.set_visible(true);
                    } else {
                        row.battery_icon_widget.set_visible(false);
                    }

                    // Spinner and switch visibility based on transitioning state
                    row.spinner.set_visible(is_transitioning);
                    row.spinner.set_spinning(is_transitioning);
                    row.switch.set_visible(!is_transitioning);
                    row.switch.set_active(device.connected);
                    row.root.set_sensitive(!is_transitioning);
                } else {
                    // Create new device row
                    let row = Self::create_device_row(
                        device_urn,
                        device,
                        action_callback,
                        transitioning,
                    );
                    entry.menu.append(&row.root);
                    device_rows.push(row);
                }
            }
        }
    }

    /// Build a single device row widget.
    fn create_device_row(
        device_urn: &Urn,
        device: &entity::bluetooth::BluetoothDevice,
        action_callback: &EntityActionCallback,
        transitioning: &Rc<RefCell<HashSet<String>>>,
    ) -> DeviceRow {
        let device_urn_str = device_urn.as_str().to_string();

        let inner = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        // Left box: device type icon + battery icon
        let icon_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let device_icon = IconWidget::from_name(device_type_icon(&device.device_type), 16);
        icon_box.append(device_icon.widget());

        let battery_icon = IconWidget::from_name(
            battery_icon_name(device.battery_percentage.unwrap_or(0)),
            16,
        );
        let battery_icon_widget = battery_icon.widget().clone().upcast::<gtk::Widget>();
        battery_icon_widget.set_visible(device.battery_percentage.is_some());
        icon_box.append(&battery_icon_widget);

        inner.append(&icon_box);

        // Center: device name (expands to fill)
        let name_label = gtk::Label::builder()
            .label(&device.name)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        inner.append(&name_label);

        // Right box: spinner (hidden by default) + connection switch
        let right_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let spinner = gtk::Spinner::builder()
            .visible(false)
            .spinning(false)
            .build();
        right_box.append(&spinner);

        let switch = gtk::Switch::builder()
            .active(device.connected)
            .sensitive(false) // display-only
            .valign(gtk::Align::Center)
            .css_classes(["device-switch"])
            .build();
        right_box.append(&switch);

        inner.append(&right_box);

        let button = gtk::Button::builder()
            .child(&inner)
            .css_classes(["flat", "device-row"])
            .build();

        let action_cb = action_callback.clone();
        let urn_for_click = device_urn.clone();
        let urn_str_for_click = device_urn_str.clone();
        let transitioning_for_click = transitioning.clone();
        button.connect_clicked(move |btn| {
            // Ignore clicks while transitioning
            if transitioning_for_click.borrow().contains(&urn_str_for_click) {
                return;
            }
            transitioning_for_click
                .borrow_mut()
                .insert(urn_str_for_click.clone());
            btn.set_sensitive(false);
            action_cb(
                urn_for_click.clone(),
                "toggle-connect".to_string(),
                serde_json::Value::Null,
            );
        });

        DeviceRow {
            urn_str: device_urn_str,
            root: button,
            name_label,
            device_icon,
            battery_icon,
            battery_icon_widget,
            spinner,
            switch,
            last_connected: RefCell::new(device.connected),
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
