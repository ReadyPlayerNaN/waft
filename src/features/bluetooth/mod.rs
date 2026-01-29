//! Bluetooth plugin - bluetooth power toggle with device menu.
//!
//! Creates one feature toggle per Bluetooth adapter.

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use crate::dbus::DbusHandle;
use crate::menu_state::MenuStore;
use crate::plugin::{ExpandCallback, Plugin, PluginId, WidgetFeatureToggle, WidgetRegistrar};
use crate::ui::feature_toggle_expandable::{
    FeatureToggleExpandableOutput, FeatureToggleExpandableProps, FeatureToggleExpandableWidget,
};

use self::dbus::{
    BluetoothAdapter, IFACE_ADAPTER1, IFACE_DEVICE1, connect_device, disconnect_device,
    find_all_adapters, get_paired_devices, set_powered,
};
use self::device_menu::{DeviceMenuOutput, DeviceMenuWidget};
use self::store::{
    BluetoothOp, BluetoothStore, DeviceConnectionState, DeviceState, create_bluetooth_store,
};

mod dbus;
mod device_menu;
pub mod store;

/// State for a single adapter's UI components.
struct AdapterUI {
    toggle: FeatureToggleExpandableWidget,
    device_menu: DeviceMenuWidget,
    expand_callback: ExpandCallback,
}

/// Represents a property change from DBus.
#[derive(Clone, Debug)]
enum PropertyChange {
    AdapterPowered(String, bool),
    DeviceConnected(String, bool),
}

pub struct BluetoothPlugin {
    dbus: Arc<DbusHandle>,
    /// Store per adapter path
    stores: Rc<RefCell<HashMap<String, Rc<BluetoothStore>>>>,
    /// UI components per adapter path
    adapter_uis: Rc<RefCell<HashMap<String, AdapterUI>>>,
    /// Channel for property changes
    property_channel: (
        flume::Sender<PropertyChange>,
        flume::Receiver<PropertyChange>,
    ),
}

impl BluetoothPlugin {
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        Self {
            dbus,
            stores: Rc::new(RefCell::new(HashMap::new())),
            adapter_uis: Rc::new(RefCell::new(HashMap::new())),
            property_channel: flume::unbounded(),
        }
    }

    async fn start_monitoring(&self) -> Result<()> {
        let property_tx = self.property_channel.0.clone();

        // Listen for PropertiesChanged on any path under org.bluez
        let rule = "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',sender='org.bluez'";

        let mut rx = self.dbus.listen_signals(rule).await?;

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        let path = msg.header().path().map(|p| p.to_string());

                        // PropertiesChanged(interface_name, changed_properties, invalidated)
                        if let Ok((iface, props, _invalidated)) = msg.body().deserialize::<(
                            String,
                            std::collections::HashMap<String, zvariant::OwnedValue>,
                            Vec<String>,
                        )>(
                        ) {
                            if let Some(ref obj_path) = path {
                                if iface == IFACE_ADAPTER1 {
                                    if let Some(powered_val) = props.get("Powered") {
                                        if let Ok(powered) = <bool>::try_from(powered_val.clone()) {
                                            let _ =
                                                property_tx.send(PropertyChange::AdapterPowered(
                                                    obj_path.clone(),
                                                    powered,
                                                ));
                                            info!(
                                                "[bluetooth/dbus] Adapter {} powered: {}",
                                                obj_path, powered
                                            );
                                        }
                                    }
                                } else if iface == IFACE_DEVICE1 {
                                    if let Some(connected_val) = props.get("Connected") {
                                        if let Ok(connected) =
                                            <bool>::try_from(connected_val.clone())
                                        {
                                            let _ =
                                                property_tx.send(PropertyChange::DeviceConnected(
                                                    obj_path.clone(),
                                                    connected,
                                                ));
                                            info!(
                                                "[bluetooth/dbus] Device {} connected: {}",
                                                obj_path, connected
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            debug!("[bluetooth] property monitoring stopped");
        });

        Ok(())
    }

    async fn load_devices_for_adapter(
        dbus: &DbusHandle,
        store: &BluetoothStore,
        adapter_path: &str,
    ) {
        match get_paired_devices(dbus, adapter_path).await {
            Ok(devices) => {
                let device_states: Vec<DeviceState> = devices
                    .into_iter()
                    .map(|d| DeviceState {
                        path: d.path,
                        name: d.name,
                        icon: d.icon,
                        connection: if d.connected {
                            DeviceConnectionState::Connected
                        } else {
                            DeviceConnectionState::Disconnected
                        },
                    })
                    .collect();

                store.emit(BluetoothOp::SetDevices(device_states));
            }
            Err(e) => {
                warn!(
                    "[bluetooth] Failed to get paired devices for {}: {}",
                    adapter_path, e
                );
            }
        }
    }

    fn create_adapter_ui(
        adapter: &BluetoothAdapter,
        store: Rc<BluetoothStore>,
        dbus: Arc<DbusHandle>,
        menu_store: Arc<MenuStore>,
    ) -> AdapterUI {
        let state = store.get_state();
        let connected_count = state
            .devices
            .values()
            .filter(|d| d.connection == DeviceConnectionState::Connected)
            .count();

        let toggle = FeatureToggleExpandableWidget::new(
            FeatureToggleExpandableProps {
                title: adapter.name.clone(),
                icon: "bluetooth-symbolic".into(),
                details: if connected_count > 0 {
                    Some(format!("{} connected", connected_count))
                } else {
                    None
                },
                active: adapter.powered,
                busy: false,
                expanded: false,
            },
            menu_store,
        );

        // Create device menu
        let device_menu = DeviceMenuWidget::new();

        // Set initial devices
        let devices: Vec<_> = state
            .devices
            .values()
            .map(|d| {
                (
                    d.path.clone(),
                    d.name.clone(),
                    d.icon.clone(),
                    d.connection.clone(),
                )
            })
            .collect();
        device_menu.set_devices(devices);

        // Connect device menu output handler
        let dbus_for_menu = dbus.clone();
        let store_for_menu = store.clone();
        device_menu.connect_output(move |event| {
            debug!("[bluetooth/ui] Device menu event: {:?}", event);
            let dbus = dbus_for_menu.clone();
            let store = store_for_menu.clone();

            match event {
                DeviceMenuOutput::Connect(path) => {
                    store.emit(BluetoothOp::SetDeviceConnection(
                        path.clone(),
                        DeviceConnectionState::Connecting,
                    ));
                    glib::spawn_future_local(async move {
                        if let Err(e) = connect_device(dbus, &path).await {
                            error!("[bluetooth] Failed to connect device: {}", e);
                            store.emit(BluetoothOp::SetDeviceConnection(
                                path,
                                DeviceConnectionState::Disconnected,
                            ));
                        }
                    });
                }
                DeviceMenuOutput::Disconnect(path) => {
                    store.emit(BluetoothOp::SetDeviceConnection(
                        path.clone(),
                        DeviceConnectionState::Disconnecting,
                    ));
                    glib::spawn_future_local(async move {
                        if let Err(e) = disconnect_device(dbus, &path).await {
                            error!("[bluetooth] Failed to disconnect device: {}", e);
                            store.emit(BluetoothOp::SetDeviceConnection(
                                path,
                                DeviceConnectionState::Connected,
                            ));
                        }
                    });
                }
            }
        });

        let expand_callback: ExpandCallback = Rc::new(RefCell::new(None));

        // Connect toggle output handler
        let adapter_path = adapter.path.clone();
        let store_for_toggle = store.clone();

        toggle.connect_output(move |event| {
            debug!("[bluetooth/ui] Toggle event: {:?}", event);
            let dbus = dbus.clone();
            let store = store_for_toggle.clone();
            let adapter_path = adapter_path.clone();

            match event {
                FeatureToggleExpandableOutput::Activate
                | FeatureToggleExpandableOutput::Deactivate => {
                    let powered = matches!(event, FeatureToggleExpandableOutput::Activate);
                    store.emit(BluetoothOp::SetBusy(true));

                    glib::spawn_future_local(async move {
                        if let Err(err) = set_powered(dbus, &adapter_path, powered).await {
                            error!("Failed to set bluetooth state: {}", err);
                            store.emit(BluetoothOp::SetBusy(false));
                        }
                    });
                }
                FeatureToggleExpandableOutput::ToggleExpand => {
                    // ToggleExpand is deprecated - expand state is managed by widget
                }
            }
        });

        // Set up expand callback for grid revealer
        toggle.set_expand_callback({
            let expand_callback = expand_callback.clone();
            move |will_be_open| {
                if let Some(ref cb) = *expand_callback.borrow() {
                    cb(will_be_open);
                }
            }
        });

        AdapterUI {
            toggle,
            device_menu,
            expand_callback,
        }
    }
}

#[async_trait(?Send)]
impl Plugin for BluetoothPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::bluetooth")
    }

    async fn init(&mut self) -> Result<()> {
        // Find all adapters
        match find_all_adapters(&self.dbus).await {
            Ok(adapters) => {
                if adapters.is_empty() {
                    warn!("[bluetooth] No adapters found");
                    return Ok(());
                }

                info!("[bluetooth] Found {} adapter(s)", adapters.len());

                // Create store for each adapter
                let mut stores = self.stores.borrow_mut();
                for adapter in &adapters {
                    info!(
                        "[bluetooth] Adapter: {} at {} (powered: {})",
                        adapter.name, adapter.path, adapter.powered
                    );

                    let store = Rc::new(create_bluetooth_store());
                    store.emit(BluetoothOp::SetAvailable(true));
                    store.emit(BluetoothOp::SetPowered(adapter.powered));

                    // Load paired devices
                    Self::load_devices_for_adapter(&self.dbus, &store, &adapter.path).await;

                    stores.insert(adapter.path.clone(), store);
                }

                // Start monitoring for changes
                self.start_monitoring().await?;
            }
            Err(e) => {
                warn!("[bluetooth] Failed to find adapters: {}", e);
            }
        }

        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Arc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        // Get adapters again to have their info
        let adapters = match find_all_adapters(&self.dbus).await {
            Ok(adapters) => adapters,
            Err(e) => {
                warn!("[bluetooth] Failed to find adapters: {}", e);
                return Ok(());
            }
        };

        let stores = self.stores.borrow();
        let mut adapter_uis = self.adapter_uis.borrow_mut();

        for adapter in adapters {
            if let Some(store) = stores.get(&adapter.path) {
                let ui = Self::create_adapter_ui(
                    &adapter,
                    store.clone(),
                    self.dbus.clone(),
                    menu_store.clone(),
                );

                // Subscribe to store for state changes
                let adapter_path = adapter.path.clone();
                let stores_ref = self.stores.clone();
                let uis_ref = self.adapter_uis.clone();

                store.subscribe(move || {
                    let stores = stores_ref.borrow();
                    let uis = uis_ref.borrow();

                    if let Some(store) = stores.get(&adapter_path) {
                        if let Some(ui) = uis.get(&adapter_path) {
                            let state = store.get_state();

                            // Update toggle state
                            ui.toggle.set_active(state.powered);
                            ui.toggle.set_busy(state.busy);

                            let connected_count = state
                                .devices
                                .values()
                                .filter(|d| d.connection == DeviceConnectionState::Connected)
                                .count();

                            ui.toggle.set_details(if connected_count > 0 {
                                Some(format!("{} connected", connected_count))
                            } else {
                                None
                            });

                            // Update device menu
                            let devices: Vec<_> = state
                                .devices
                                .values()
                                .map(|d| {
                                    (
                                        d.path.clone(),
                                        d.name.clone(),
                                        d.icon.clone(),
                                        d.connection.clone(),
                                    )
                                })
                                .collect();
                            ui.device_menu.set_devices(devices);
                        }
                    }
                });

                // Register the feature toggle
                registrar.register_feature_toggle(Arc::new(WidgetFeatureToggle {
                    id: format!("bluetooth:{}", adapter.path),
                    el: ui.toggle.widget(),
                    weight: 100,
                    menu: Some(ui.device_menu.root.clone().upcast::<gtk::Widget>()),
                    on_expand_toggled: Some(ui.expand_callback.clone()),
                    menu_id: Some(ui.toggle.menu_id.clone()),
                }));

                adapter_uis.insert(adapter.path.clone(), ui);
            }
        }

        // Handle property changes from DBus monitoring
        let stores_for_props = self.stores.clone();
        let property_rx = self.property_channel.1.clone();

        glib::spawn_future_local(async move {
            while let Ok(change) = property_rx.recv_async().await {
                let stores = stores_for_props.borrow();

                match change {
                    PropertyChange::AdapterPowered(path, powered) => {
                        if let Some(store) = stores.get(&path) {
                            store.emit(BluetoothOp::SetPowered(powered));
                            store.emit(BluetoothOp::SetBusy(false));
                        }
                    }
                    PropertyChange::DeviceConnected(device_path, connected) => {
                        // Find which adapter this device belongs to
                        for (adapter_path, store) in stores.iter() {
                            if device_path.starts_with(adapter_path) {
                                let connection = if connected {
                                    DeviceConnectionState::Connected
                                } else {
                                    DeviceConnectionState::Disconnected
                                };
                                store.emit(BluetoothOp::SetDeviceConnection(
                                    device_path.clone(),
                                    connection,
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }
}
