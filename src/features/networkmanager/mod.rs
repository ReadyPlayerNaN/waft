mod dbus;
mod ethernet_menu;
mod store;
mod vpn_menu;
mod vpn_toggle;
mod wifi_adapter_widget;
mod wifi_menu;
mod wifi_toggle;
mod wired_adapter_widget;
mod wired_toggle_widget;

use anyhow::Result;
use async_trait::async_trait;
use crate::dbus::DbusHandle;
use crate::menu_state::MenuStore;
use crate::plugin::{Plugin, PluginId, WidgetRegistrar};
use log::{debug, error, info};
use nmrs::NetworkManager;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use store::{
    create_network_store, EthernetAdapterState, NetworkOp, NetworkStore, WiFiAdapterState,
};
use wifi_adapter_widget::WiFiAdapterWidget;
use wired_adapter_widget::WiredAdapterWidget;

pub struct NetworkManagerPlugin {
    dbus: Arc<DbusHandle>,
    nm: Option<NetworkManager>,
    store: Arc<NetworkStore>,
    menu_store: Option<Arc<MenuStore>>,
    registrar: Option<Rc<dyn WidgetRegistrar>>,
    ethernet_uis: Rc<RefCell<HashMap<String, WiredAdapterWidget>>>,
    wifi_uis: Rc<RefCell<HashMap<String, WiFiAdapterWidget>>>,
}

impl NetworkManagerPlugin {
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        let store = Arc::new(create_network_store());
        Self {
            dbus,
            nm: None,
            store,
            menu_store: None,
            registrar: None,
            ethernet_uis: Rc::new(RefCell::new(HashMap::new())),
            wifi_uis: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

#[async_trait(?Send)]
impl Plugin for NetworkManagerPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::networkmanager")
    }

    async fn init(&mut self) -> Result<()> {
        // Try to create nmrs NetworkManager instance
        let nm = match NetworkManager::new().await {
            Ok(nm) => {
                info!("NetworkManager available: true");
                self.nm = Some(nm);
                self.store.emit(NetworkOp::SetAvailable(true));
                self.nm.as_ref().unwrap()
            }
            Err(e) => {
                info!("NetworkManager not available: {}", e);
                self.store.emit(NetworkOp::SetAvailable(false));
                return Ok(());
            }
        };

        match dbus::get_all_devices_nmrs(nm).await {
            Ok(devices) => {
                info!("Found {} network devices", devices.len());

                for device in devices {
                    debug!(
                        "Device: {} ({}) type={}",
                        device.interface_name, device.path, device.device_type
                    );

                    match device.device_type {
                        1 => {
                            // Ethernet - device state is included in DeviceInfo from nmrs
                            let device_state = device.device_state;
                            debug!("Device state for {}: {}", device.interface_name, device_state);

                            // Derive carrier from device state:
                            // - Unavailable (20) = no carrier (cable not connected)
                            // - Disconnected (30) or higher = carrier present
                            let carrier = device_state >= 30;
                            debug!("Carrier for {} (derived from state): {}", device.interface_name, carrier);

                            // Derive active connection presence from state (100 = Activated)
                            // nmrs doesn't expose the active connection path directly
                            let active_connection: Option<String> = if device_state == 100 {
                                Some(device.path.clone()) // Use device path as placeholder
                            } else {
                                None
                            };
                            debug!("Active connection for {}: {:?}", device.interface_name, active_connection);

                            // Device state: 20 = unavailable, 30 = disconnected, 100 = activated, etc.
                            let enabled = device_state >= 20; // Not unmanaged (10) or unknown (0)

                            info!(
                                "Ethernet {} initialized: enabled={}, carrier={}, state={}, active_conn={:?}",
                                device.interface_name, enabled, carrier, device_state, active_connection
                            );

                            let adapter = EthernetAdapterState {
                                path: device.path.clone(),
                                interface_name: device.interface_name.clone(),
                                enabled,
                                carrier,
                                device_state,
                                active_connection,
                                available_connections: vec![],
                            };
                            self.store.emit(NetworkOp::AddEthernetAdapter(adapter));
                        }
                        2 => {
                            // WiFi
                            let adapter = WiFiAdapterState {
                                path: device.path.clone(),
                                interface_name: device.interface_name.clone(),
                                enabled: true,
                                busy: false,
                                active_connection: None,
                                access_points: HashMap::new(),
                                scanning: false,
                            };
                            self.store.emit(NetworkOp::AddWiFiAdapter(adapter.clone()));
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                error!("Failed to get network devices: {}", e);
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
        // Store registrar and menu_store for runtime use (device add/remove)
        self.menu_store = Some(menu_store.clone());
        self.registrar = Some(registrar.clone());

        let state = self.store.get_state();

        // Create Ethernet UIs using WiredAdapterWidget
        for (path, adapter) in &state.ethernet_adapters {
            info!(
                "Creating UI for ethernet {}: enabled={}, carrier={}, state={}, active_conn={:?}",
                adapter.interface_name, adapter.enabled, adapter.carrier, adapter.device_state, adapter.active_connection
            );

            let widget = WiredAdapterWidget::new(
                adapter,
                self.store.clone(),
                self.nm.clone(),
                self.dbus.clone(),
                menu_store.clone(),
            );

            // Register the feature toggle
            registrar.register_feature_toggle(widget.widget());

            self.ethernet_uis.borrow_mut().insert(path.clone(), widget);
        }

        // Create WiFi UIs using WiFiAdapterWidget
        for (path, adapter) in &state.wifi_adapters {
            info!(
                "Creating UI for WiFi {}: enabled={}, active_conn={:?}",
                adapter.interface_name, adapter.enabled, adapter.active_connection
            );

            let widget = WiFiAdapterWidget::new(
                adapter,
                self.store.clone(),
                self.nm.clone(),
                self.dbus.clone(),
                menu_store.clone(),
            );

            // Register the feature toggle
            registrar.register_feature_toggle(widget.widget());

            self.wifi_uis.borrow_mut().insert(path.clone(), widget);
        }

        // Subscribe to device add/remove signals
        self.subscribe_device_signals();

        Ok(())
    }
}

impl NetworkManagerPlugin {
    /// Subscribe to NetworkManager device add/remove signals for dynamic widget updates.
    fn subscribe_device_signals(&self) {
        // Clone all required state for the async handlers
        let dbus = self.dbus.clone();
        let store = self.store.clone();
        let nm = self.nm.clone();
        let menu_store = self.menu_store.clone();
        let registrar = self.registrar.clone();
        let ethernet_uis = self.ethernet_uis.clone();
        let wifi_uis = self.wifi_uis.clone();

        // Device added subscription - callback will be called on each signal
        let dbus_for_add = dbus.clone();
        let store_for_add = store.clone();
        let nm_for_add = nm.clone();
        let menu_store_for_add = menu_store.clone();
        let registrar_for_add = registrar.clone();
        let ethernet_uis_for_add = ethernet_uis.clone();
        let wifi_uis_for_add = wifi_uis.clone();

        glib::spawn_future_local(async move {
            if let Err(e) = dbus::subscribe_device_added(dbus_for_add.clone(), move |device_path| {
                debug!("Device added signal: {}", device_path);

                // Clone state for use in the spawned future
                let dbus = dbus_for_add.clone();
                let store = store_for_add.clone();
                let nm = nm_for_add.clone();
                let menu_store = menu_store_for_add.clone();
                let registrar = registrar_for_add.clone();
                let ethernet_uis = ethernet_uis_for_add.clone();
                let wifi_uis = wifi_uis_for_add.clone();

                // Spawn a future to handle device info lookup (can't be async in callback)
                glib::spawn_future_local(async move {
                    // Get device info using spawn_on_tokio to avoid CPU spin.
                    // zbus D-Bus calls are tokio-dependent and must run on the tokio runtime.
                    let device_info = match crate::runtime::spawn_on_tokio(
                        dbus::get_device_info_sendable(dbus.clone(), device_path.clone())
                    ).await {
                        Ok(Some(info)) => info,
                        Ok(None) => {
                            debug!("Ignoring non-managed device: {}", device_path);
                            return;
                        }
                        Err(e) => {
                            error!("Failed to get device info: {}", e);
                            return;
                        }
                    };

                    let registrar = match &registrar {
                        Some(r) => r.clone(),
                        None => return,
                    };
                    let menu_store = match &menu_store {
                        Some(m) => m.clone(),
                        None => return,
                    };

                    match device_info.device_type {
                        1 => {
                            // Ethernet
                            let carrier = device_info.device_state >= 30;
                            let active_connection = if device_info.device_state == 100 {
                                Some(device_info.path.clone())
                            } else {
                                None
                            };
                            let enabled = device_info.device_state >= 20;

                            info!(
                                "Hot-plugged Ethernet {}: enabled={}, carrier={}, state={}",
                                device_info.interface_name, enabled, carrier, device_info.device_state
                            );

                            let adapter = EthernetAdapterState {
                                path: device_info.path.clone(),
                                interface_name: device_info.interface_name.clone(),
                                enabled,
                                carrier,
                                device_state: device_info.device_state,
                                active_connection,
                                available_connections: vec![],
                            };

                            store.emit(NetworkOp::AddEthernetAdapter(adapter.clone()));

                            let widget = WiredAdapterWidget::new(
                                &adapter,
                                store.clone(),
                                nm.clone(),
                                dbus.clone(),
                                menu_store,
                            );

                            registrar.register_feature_toggle(widget.widget());
                            ethernet_uis.borrow_mut().insert(device_info.path.clone(), widget);
                        }
                        2 => {
                            // WiFi
                            info!(
                                "Hot-plugged WiFi {}: state={}",
                                device_info.interface_name, device_info.device_state
                            );

                            let adapter = WiFiAdapterState {
                                path: device_info.path.clone(),
                                interface_name: device_info.interface_name.clone(),
                                enabled: true,
                                busy: false,
                                active_connection: None,
                                access_points: HashMap::new(),
                                scanning: false,
                            };

                            store.emit(NetworkOp::AddWiFiAdapter(adapter.clone()));

                            let widget = WiFiAdapterWidget::new(
                                &adapter,
                                store.clone(),
                                nm.clone(),
                                dbus.clone(),
                                menu_store,
                            );

                            registrar.register_feature_toggle(widget.widget());
                            wifi_uis.borrow_mut().insert(device_info.path.clone(), widget);
                        }
                        _ => {}
                    }
                });
            }).await {
                error!("Failed to subscribe to DeviceAdded signal: {}", e);
            }
        });

        // Device removed subscription
        glib::spawn_future_local(async move {
            if let Err(e) = dbus::subscribe_device_removed(dbus.clone(), move |device_path| {
                debug!("Device removed signal: {}", device_path);

                let registrar = match &registrar {
                    Some(r) => r.clone(),
                    None => return,
                };

                // Check ethernet adapters
                if let Some(widget) = ethernet_uis.borrow_mut().remove(&device_path) {
                    info!("Removing Ethernet adapter widget: {}", device_path);
                    let id = widget.widget().id.clone();
                    registrar.unregister_feature_toggle(&id);
                    store.emit(NetworkOp::RemoveEthernetAdapter(device_path.clone()));
                }

                // Check wifi adapters
                if let Some(widget) = wifi_uis.borrow_mut().remove(&device_path) {
                    info!("Removing WiFi adapter widget: {}", device_path);
                    let id = widget.widget().id.clone();
                    registrar.unregister_feature_toggle(&id);
                    store.emit(NetworkOp::RemoveWiFiAdapter(device_path.clone()));
                }
            }).await {
                error!("Failed to subscribe to DeviceRemoved signal: {}", e);
            }
        });
    }
}
