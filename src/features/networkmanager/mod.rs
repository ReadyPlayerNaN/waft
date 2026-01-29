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
use crate::plugin::{Plugin, PluginId, WidgetFeatureToggle};
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
        _menu_store: Arc<MenuStore>,
    ) -> Result<()> {
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
                _menu_store.clone(),
            );

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
                _menu_store.clone(),
            );

            self.wifi_uis.borrow_mut().insert(path.clone(), widget);
        }

        Ok(())
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        let mut toggles = Vec::new();

        // Add WiFi toggles
        let wifi_uis = self.wifi_uis.borrow();
        for (_, widget) in wifi_uis.iter() {
            toggles.push(widget.widget());
        }

        // Add Ethernet toggles
        let ethernet_uis = self.ethernet_uis.borrow();
        for (_, widget) in ethernet_uis.iter() {
            toggles.push(widget.widget());
        }

        toggles
    }
}
