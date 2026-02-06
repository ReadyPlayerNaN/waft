//! Bluetooth plugin — bluetooth power toggle with device menu.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Creates one feature toggle per Bluetooth adapter.

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use waft_core::dbus::DbusHandle;
use waft_core::menu_state::MenuStore;
use waft_plugin_api::ui::feature_toggle::{
    FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget,
};
use waft_plugin_api::{
    ExpandCallback, OverviewPlugin, PluginId, PluginResources, WidgetFeatureToggle, WidgetRegistrar,
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
mod store;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::bluetooth", "Bluetooth", "0.1.0");
waft_plugin_api::export_overview_plugin!(BluetoothPlugin::new());

/// State for a single adapter's UI components.
struct AdapterUI {
    toggle: FeatureToggleWidget,
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
    dbus: Option<Arc<DbusHandle>>,
    tokio_handle: Option<tokio::runtime::Handle>,
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

impl Default for BluetoothPlugin {
    fn default() -> Self {
        Self {
            dbus: None,
            tokio_handle: None,
            stores: Rc::new(RefCell::new(HashMap::new())),
            adapter_uis: Rc::new(RefCell::new(HashMap::new())),
            property_channel: flume::unbounded(),
        }
    }
}

impl BluetoothPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    async fn start_monitoring(
        &self,
        dbus: Arc<DbusHandle>,
        tokio_handle: &tokio::runtime::Handle,
    ) -> Result<()> {
        let property_tx = self.property_channel.0.clone();

        // Listen for PropertiesChanged on any path under org.bluez
        let rule = "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',sender='org.bluez'";

        let mut rx = dbus.listen_signals_with_handle(rule, Some(tokio_handle)).await?;

        // Spawn monitoring task on a dedicated thread with block_on to ensure
        // the plugin's copy of tokio has full runtime context (needed for zbus
        // proxy Drop which calls tokio::spawn internally).
        let handle = tokio_handle.clone();
        std::thread::spawn(move || {
            handle.block_on(async move {
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
                            )
                                && let Some(ref obj_path) = path {
                                    if iface == IFACE_ADAPTER1 {
                                        if let Some(powered_val) = props.get("Powered")
                                            && let Ok(powered) = <bool>::try_from(powered_val.clone()) {
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
                                    } else if iface == IFACE_DEVICE1
                                        && let Some(connected_val) = props.get("Connected")
                                            && let Ok(connected) =
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
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                debug!("[bluetooth] property monitoring stopped");
            });
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

                store.emit(BluetoothOp::Devices(device_states));
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
        menu_store: Rc<MenuStore>,
        tokio_handle: tokio::runtime::Handle,
    ) -> AdapterUI {
        let state = store.get_state();
        let connected_count = state
            .devices
            .values()
            .filter(|d| d.connection == DeviceConnectionState::Connected)
            .count();

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: adapter.name.clone(),
                icon: "bluetooth-symbolic".into(),
                details: if connected_count > 0 {
                    Some(format!("{} connected", connected_count))
                } else {
                    None
                },
                active: adapter.powered,
                busy: false,
                expandable: true, // Bluetooth always shows device menu
            },
            Some(menu_store),
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
        let handle_for_menu = tokio_handle.clone();
        device_menu.connect_output(move |event| {
            debug!("[bluetooth/ui] Device menu event: {:?}", event);
            let dbus = dbus_for_menu.clone();
            let store = store_for_menu.clone();
            let handle = handle_for_menu.clone();

            match event {
                DeviceMenuOutput::Connect(path) => {
                    store.emit(BluetoothOp::DeviceConnection(
                        path.clone(),
                        DeviceConnectionState::Connecting,
                    ));
                    glib::spawn_future_local(async move {
                        let (tx, rx) = flume::bounded(1);
                        let h = handle.clone();
                        std::thread::spawn(move || {
                            let result = h.block_on(connect_device(dbus, &path));
                            let _ = tx.send((result, path));
                        });
                        if let Ok((Err(e), path)) = rx.recv_async().await {
                            error!("[bluetooth] Failed to connect device: {}", e);
                            store.emit(BluetoothOp::DeviceConnection(
                                path,
                                DeviceConnectionState::Disconnected,
                            ));
                        }
                    });
                }
                DeviceMenuOutput::Disconnect(path) => {
                    store.emit(BluetoothOp::DeviceConnection(
                        path.clone(),
                        DeviceConnectionState::Disconnecting,
                    ));
                    glib::spawn_future_local(async move {
                        let (tx, rx) = flume::bounded(1);
                        let h = handle.clone();
                        std::thread::spawn(move || {
                            let result = h.block_on(disconnect_device(dbus, &path));
                            let _ = tx.send((result, path));
                        });
                        if let Ok((Err(e), path)) = rx.recv_async().await {
                            error!("[bluetooth] Failed to disconnect device: {}", e);
                            store.emit(BluetoothOp::DeviceConnection(
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
        let handle_for_toggle = tokio_handle;

        toggle.connect_output(move |event| {
            debug!("[bluetooth/ui] Toggle event: {:?}", event);
            let dbus = dbus.clone();
            let store = store_for_toggle.clone();
            let adapter_path = adapter_path.clone();
            let handle = handle_for_toggle.clone();

            match event {
                FeatureToggleOutput::Activate | FeatureToggleOutput::Deactivate => {
                    let powered = matches!(event, FeatureToggleOutput::Activate);
                    store.emit(BluetoothOp::Busy(true));

                    glib::spawn_future_local(async move {
                        let (tx, rx) = flume::bounded(1);
                        let h = handle.clone();
                        std::thread::spawn(move || {
                            let result = h.block_on(set_powered(dbus, &adapter_path, powered));
                            let _ = tx.send(result);
                        });
                        match rx.recv_async().await {
                            Ok(Ok(())) => {}
                            Ok(Err(err)) => {
                                error!("[bluetooth] Failed to set bluetooth state: {}", err);
                            }
                            Err(err) => {
                                error!("[bluetooth] Set powered task failed: {}", err);
                            }
                        }
                        store.emit(BluetoothOp::Busy(false));
                    });
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
impl OverviewPlugin for BluetoothPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::bluetooth")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[bluetooth] init() called");

        // Use the system dbus connection provided by the host
        let dbus = resources
            .system_dbus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("system_dbus not provided"))?
            .clone();
        debug!("[bluetooth] Received dbus connection from host");

        // Save the tokio handle and enter runtime context for this plugin's copy of tokio
        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();
        self.tokio_handle = Some(tokio_handle.clone());

        // Find all adapters
        match find_all_adapters(&dbus).await {
            Ok(adapters) => {
                if adapters.is_empty() {
                    warn!("[bluetooth] No adapters found");
                    return Ok(());
                }

                info!("[bluetooth] Found {} adapter(s)", adapters.len());

                // Create store for each adapter
                for adapter in &adapters {
                    info!(
                        "[bluetooth] Adapter: {} at {} (powered: {})",
                        adapter.name, adapter.path, adapter.powered
                    );

                    let store = Rc::new(create_bluetooth_store());
                    store.emit(BluetoothOp::Available(true));
                    store.emit(BluetoothOp::Powered(adapter.powered));

                    // Load paired devices
                    Self::load_devices_for_adapter(&dbus, &store, &adapter.path).await;

                    // Only borrow for the insert
                    self.stores.borrow_mut().insert(adapter.path.clone(), store);
                }

                // Start monitoring for changes using tokio handle from PluginResources
                let tokio_handle = self
                    .tokio_handle
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
                self.start_monitoring(dbus.clone(), tokio_handle).await?;
            }
            Err(e) => {
                warn!("[bluetooth] Failed to find adapters: {}", e);
            }
        }

        self.dbus = Some(dbus);
        debug!("[bluetooth] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());
        let dbus = self
            .dbus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("dbus not initialized"))?;

        // Get adapters again to have their info
        let adapters = match find_all_adapters(dbus).await {
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
                    dbus.clone(),
                    menu_store.clone(),
                    self.tokio_handle.clone().expect("tokio_handle not initialized"),
                );

                // Subscribe to store for state changes
                let adapter_path = adapter.path.clone();
                let stores_ref = self.stores.clone();
                let uis_ref = self.adapter_uis.clone();

                store.subscribe(move || {
                    let stores = stores_ref.borrow();
                    let uis = uis_ref.borrow();

                    if let Some(store) = stores.get(&adapter_path)
                        && let Some(ui) = uis.get(&adapter_path) {
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
                });

                // Register the feature toggle
                registrar.register_feature_toggle(Rc::new(WidgetFeatureToggle {
                    id: format!("bluetooth:{}", adapter.path),
                    el: ui.toggle.root.clone().upcast::<gtk::Widget>(),
                    weight: 100,
                    menu: Some(ui.device_menu.root.clone().upcast::<gtk::Widget>()),
                    on_expand_toggled: Some(ui.expand_callback.clone()),
                    menu_id: ui.toggle.menu_id.clone(),
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
                            store.emit(BluetoothOp::Powered(powered));
                            store.emit(BluetoothOp::Busy(false));
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
                                store.emit(BluetoothOp::DeviceConnection(
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
