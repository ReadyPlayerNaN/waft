mod dbus;
mod ethernet_menu;
mod store;
mod vpn_menu;
mod vpn_toggle;
mod wifi_menu;
mod wifi_toggle;

use anyhow::Result;
use async_trait::async_trait;
use crate::dbus::DbusHandle;
use crate::menu_state::MenuStore;
use crate::plugin::{ExpandCallback, Plugin, PluginId, WidgetFeatureToggle};
use crate::ui::feature_toggle_expandable::{
    FeatureToggleExpandableOutput, FeatureToggleExpandableProps, FeatureToggleExpandableWidget,
};
use ethernet_menu::{ConnectionDetails, EthernetMenuWidget};
use log::{debug, error, info};
use nmrs::NetworkManager;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use store::{
    create_network_store, AccessPointState, EthernetAdapterState, NetworkOp, NetworkStore,
    WiFiAdapterState,
};
use wifi_menu::{WiFiMenuOutput, WiFiMenuWidget};

struct EthernetAdapterUI {
    toggle: FeatureToggleExpandableWidget,
    menu: EthernetMenuWidget,
    expand_callback: ExpandCallback,
}

struct WiFiAdapterUI {
    toggle: FeatureToggleExpandableWidget,
    menu: WiFiMenuWidget,
    expand_callback: ExpandCallback,
}

pub struct NetworkManagerPlugin {
    dbus: Arc<DbusHandle>,
    nm: Option<NetworkManager>,
    store: Arc<NetworkStore>,
    ethernet_uis: Rc<RefCell<HashMap<String, EthernetAdapterUI>>>,
    wifi_uis: Rc<RefCell<HashMap<String, WiFiAdapterUI>>>,
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

        // Create Ethernet UIs
        for (path, adapter) in &state.ethernet_adapters {
            info!(
                "Creating UI for ethernet {}: enabled={}, carrier={}, state={}, active_conn={:?}",
                adapter.interface_name, adapter.enabled, adapter.carrier, adapter.device_state, adapter.active_connection
            );

            // Device states: 100 = activated/connected, 30 = disconnected, 20 = unavailable
            let is_connected = adapter.device_state == 100;

            let initial_details = if adapter.enabled {
                if is_connected {
                    Some(crate::i18n::t("network-connected"))
                } else if adapter.carrier {
                    Some(crate::i18n::t("network-disconnected"))
                } else {
                    Some(crate::i18n::t("network-disconnected"))
                }
            } else {
                Some(crate::i18n::t("network-disabled"))
            };

            let icon = if adapter.enabled {
                if is_connected {
                    "network-wired-symbolic"
                } else if adapter.carrier {
                    "network-wired-disconnected-symbolic"
                } else {
                    "network-wired-disconnected-symbolic"
                }
            } else {
                "network-wired-offline-symbolic"
            };

            let toggle = FeatureToggleExpandableWidget::new(
                FeatureToggleExpandableProps {
                    title: format!("Wired ({})", adapter.interface_name),
                    icon: icon.into(),
                    details: initial_details,
                    active: adapter.enabled,
                    busy: false,
                    expanded: false,
                },
                _menu_store.clone(),
            );

            let menu = EthernetMenuWidget::new();
            let expand_callback: ExpandCallback = Rc::new(RefCell::new(None));

            // Connect toggle output handler
            let device_path = path.clone();
            let store_clone = self.store.clone();
            let nm_clone = self.nm.clone();
            toggle.connect_output(move |event| {
                debug!("Ethernet toggle event: {:?}", event);
                let device_path = device_path.clone();
                let _store = store_clone.clone();
                let nm = nm_clone.clone();

                match event {
                    FeatureToggleExpandableOutput::Activate
                    | FeatureToggleExpandableOutput::Deactivate => {
                        let enabled = matches!(event, FeatureToggleExpandableOutput::Activate);

                        info!("Ethernet toggle: enabled={}, device={}", enabled, device_path);

                        // Use separate thread with tokio runtime for nmrs work
                        let (tx, rx) = std::sync::mpsc::channel();
                        std::thread::spawn(move || {
                            tokio::runtime::Runtime::new()
                                .unwrap()
                                .block_on(async move {
                                    if let Some(nm) = nm {
                                        if enabled {
                                            // Activate wired connection using nmrs
                                            match dbus::connect_wired_nmrs(&nm).await {
                                                Ok(_) => {
                                                    info!("Successfully activated ethernet device");
                                                }
                                                Err(e) => {
                                                    error!("Failed to activate ethernet device: {}", e);
                                                }
                                            }
                                        } else {
                                            // Disconnect using nmrs
                                            match dbus::disconnect_nmrs(&nm).await {
                                                Ok(_) => {
                                                    info!("Successfully disconnected ethernet device");
                                                }
                                                Err(e) => {
                                                    error!("Failed to disconnect ethernet device: {}", e);
                                                }
                                            }
                                        }
                                    } else {
                                        error!("NetworkManager not available");
                                    }
                                    let _ = tx.send(enabled);
                                });
                        });

                        // Poll for results in glib main loop
                        let rx = std::rc::Rc::new(std::cell::RefCell::new(Some(rx)));
                        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                            let receiver_opt = rx.borrow_mut().take();
                            if let Some(receiver) = receiver_opt {
                                match receiver.try_recv() {
                                    Ok(_enabled) => {
                                        // Operation completed
                                        return glib::ControlFlow::Break;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                                        *rx.borrow_mut() = Some(receiver);
                                        return glib::ControlFlow::Continue;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                        return glib::ControlFlow::Break;
                                    }
                                }
                            }
                            glib::ControlFlow::Break
                        });
                    }
                    FeatureToggleExpandableOutput::ToggleExpand => {
                        // Expand is handled by the menu system automatically
                    }
                }
            });

            // Setup expand callback to fetch connection details
            let menu_clone = menu.clone();
            let device_path_clone = path.clone();
            let dbus_clone = self.dbus.clone();
            let expand_cb = move |expanded: bool| {
                if expanded {
                    debug!("Fetching ethernet connection details for {}", device_path_clone);
                    let menu = menu_clone.clone();
                    let device_path = device_path_clone.clone();
                    let dbus = dbus_clone.clone();

                    // Fetch connection details using separate thread with tokio
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        tokio::runtime::Runtime::new()
                            .unwrap()
                            .block_on(async move {
                                let mut details = ConnectionDetails::default();

                                // Get link speed
                                if let Ok(Some(speed)) = dbus::get_link_speed(&dbus, &device_path).await {
                                    if speed >= 1000 {
                                        details.link_speed = Some(format!("{} Gbps", speed / 1000));
                                    } else {
                                        details.link_speed = Some(format!("{} Mbps", speed));
                                    }
                                }

                                // Note: IP address/gateway display removed from scope (nmrs doesn't expose these directly)

                                let _ = tx.send(details);
                            });
                    });

                    // Poll for results in glib main loop
                    let rx = Rc::new(RefCell::new(Some(rx)));
                    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                        let receiver_opt = rx.borrow_mut().take();
                        if let Some(receiver) = receiver_opt {
                            match receiver.try_recv() {
                                Ok(details) => {
                                    menu.set_connection_details(Some(details));
                                    return glib::ControlFlow::Break;
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    *rx.borrow_mut() = Some(receiver);
                                    return glib::ControlFlow::Continue;
                                }
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    return glib::ControlFlow::Break;
                                }
                            }
                        }
                        glib::ControlFlow::Break
                    });
                } else {
                    menu_clone.clear();
                }
            };
            *expand_callback.borrow_mut() = Some(Box::new(expand_cb));

            self.ethernet_uis.borrow_mut().insert(
                path.clone(),
                EthernetAdapterUI {
                    toggle,
                    menu,
                    expand_callback,
                },
            );
        }

        // Create WiFi UIs
        for (path, adapter) in &state.wifi_adapters {
            // Prepare initial details text
            let initial_details = if let Some(ref ssid) = adapter.active_connection {
                Some(ssid.clone())
            } else {
                let count = adapter.access_points.len();
                if count > 0 {
                    Some(format!("{} network{} available", count, if count == 1 { "" } else { "s" }))
                } else {
                    None
                }
            };

            let toggle = FeatureToggleExpandableWidget::new(
                FeatureToggleExpandableProps {
                    title: format!("WiFi ({})", adapter.interface_name),
                    icon: "network-wireless-symbolic".into(),
                    details: initial_details,
                    active: adapter.enabled,
                    busy: false,
                    expanded: false,
                },
                _menu_store.clone(),
            );

            let menu = WiFiMenuWidget::new();

            // Set initial networks in menu
            let networks: Vec<(String, u8, bool)> = adapter
                .access_points
                .values()
                .map(|ap| (ap.ssid.clone(), ap.strength, ap.secure))
                .collect();
            menu.set_networks(networks);
            menu.set_active_ssid(adapter.active_connection.clone());

            let expand_callback: ExpandCallback = Rc::new(RefCell::new(None));

            // Connect toggle output handler
            let device_path = path.clone();
            let store_clone = self.store.clone();
            let nm_clone = self.nm.clone();
            toggle.connect_output(move |event| {
                debug!("WiFi toggle event: {:?}", event);
                let device_path = device_path.clone();
                let store = store_clone.clone();
                let nm = nm_clone.clone();

                match event {
                    FeatureToggleExpandableOutput::Activate
                    | FeatureToggleExpandableOutput::Deactivate => {
                        let enabled = matches!(event, FeatureToggleExpandableOutput::Activate);
                        store.emit(NetworkOp::SetWiFiBusy(device_path.clone(), true));

                        // Use separate thread with tokio runtime for nmrs work
                        let (tx, rx) = std::sync::mpsc::channel();
                        let _device_path_clone = device_path.clone();

                        std::thread::spawn(move || {
                            tokio::runtime::Runtime::new()
                                .unwrap()
                                .block_on(async move {
                                    if let Some(nm) = nm {
                                        if let Err(e) = dbus::set_wifi_enabled_nmrs(&nm, enabled).await
                                        {
                                            error!("Failed to set WiFi enabled state: {}", e);
                                        }
                                    }
                                    let _ = tx.send(enabled);
                                });
                        });

                        // Poll for completion in glib main loop
                        let rx = std::rc::Rc::new(std::cell::RefCell::new(Some(rx)));
                        let device_path_for_poll = device_path.clone();
                        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                            // Take the receiver out to avoid holding the borrow
                            let receiver_opt = rx.borrow_mut().take();

                            if let Some(receiver) = receiver_opt {
                                match receiver.try_recv() {
                                    Ok(enabled) => {
                                        store.emit(NetworkOp::SetWiFiEnabled(device_path_for_poll.clone(), enabled));
                                        store.emit(NetworkOp::SetWiFiBusy(device_path_for_poll.clone(), false));
                                        return glib::ControlFlow::Break;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                                        // Put receiver back and continue polling
                                        *rx.borrow_mut() = Some(receiver);
                                        return glib::ControlFlow::Continue;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                        store.emit(NetworkOp::SetWiFiBusy(device_path_for_poll.clone(), false));
                                        return glib::ControlFlow::Break;
                                    }
                                }
                            }
                            glib::ControlFlow::Break
                        });
                    }
                    FeatureToggleExpandableOutput::ToggleExpand => {
                        // ToggleExpand is deprecated - expand state is managed by widget
                    }
                }
            });

            // Set up expand callback for grid revealer and auto-scan
            let device_path_for_expand = path.clone();
            let store_for_expand = self.store.clone();
            let menu_for_expand = menu.clone();
            let toggle_for_expand = toggle.clone();
            let nm_for_expand = self.nm.clone();
            let dbus_for_expand = self.dbus.clone();
            toggle.set_expand_callback({
                let expand_callback = expand_callback.clone();
                move |will_be_open| {
                    // Notify grid callback for menu positioning
                    if let Some(ref cb) = *expand_callback.borrow() {
                        cb(will_be_open);
                    }

                    // Auto-scan when menu opens
                    if will_be_open {
                        debug!("WiFi menu opening, scanning for networks");
                        let (tx, rx) = std::sync::mpsc::channel();
                        let nm = nm_for_expand.clone();
                        let dbus = dbus_for_expand.clone();

                        std::thread::spawn(move || {
                            tokio::runtime::Runtime::new()
                                .unwrap()
                                .block_on(async move {
                                    // Use nmrs for scan and network listing
                                    let Some(ref nm) = nm else {
                                        error!("NetworkManager not available");
                                        let _ = tx.send(None);
                                        return;
                                    };

                                    if let Err(e) = dbus::scan_networks_nmrs(nm).await {
                                        error!("Failed to request WiFi scan: {}", e);
                                        let _ = tx.send(None);
                                        return;
                                    }

                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                                    // Use nmrs for listing networks
                                    match dbus::list_networks_nmrs(nm).await {
                                        Ok(aps) => {
                                            // Deduplicate by SSID, keeping strongest signal
                                            let mut networks_by_ssid: std::collections::HashMap<String, AccessPointState> = std::collections::HashMap::new();

                                            for ap in aps {
                                                match dbus::get_connections_for_ssid(&dbus, &ap.ssid).await {
                                                    Ok(connections) if !connections.is_empty() => {
                                                        let secure = ap.is_secure();
                                                        let network = AccessPointState {
                                                            path: ap.path.clone(),
                                                            ssid: ap.ssid.clone(),
                                                            strength: ap.strength,
                                                            secure,
                                                            connecting: false,
                                                        };

                                                        // Keep this network if it's the first or has stronger signal
                                                        match networks_by_ssid.get(&ap.ssid) {
                                                            Some(existing) if existing.strength >= ap.strength => {
                                                                // Keep existing (stronger or equal)
                                                            }
                                                            _ => {
                                                                // Replace with this one (stronger)
                                                                networks_by_ssid.insert(ap.ssid.clone(), network);
                                                            }
                                                        }
                                                    }
                                                    _ => {
                                                        debug!("Skipping network {} (no saved profile)", ap.ssid);
                                                    }
                                                }
                                            }

                                            let known_networks: Vec<AccessPointState> = networks_by_ssid.into_values().collect();
                                            let _ = tx.send(Some(known_networks));
                                        }
                                        Err(e) => {
                                            error!("Failed to get access points after scan: {}", e);
                                            let _ = tx.send(None);
                                        }
                                    }
                                });
                        });

                        let rx = std::rc::Rc::new(std::cell::RefCell::new(Some(rx)));
                        let menu_clone = menu_for_expand.clone();
                        let toggle_clone = toggle_for_expand.clone();
                        let store_clone = store_for_expand.clone();
                        let device_path_clone = device_path_for_expand.clone();
                        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                            let receiver_opt = rx.borrow_mut().take();

                            if let Some(receiver) = receiver_opt {
                                match receiver.try_recv() {
                                    Ok(Some(access_points)) => {
                                        store_clone.emit(NetworkOp::SetWiFiAccessPoints(
                                            device_path_clone.clone(),
                                            access_points.clone(),
                                        ));

                                        // Get active connection from store
                                        let active_ssid = {
                                            let state = store_clone.get_state();
                                            state.wifi_adapters.get(&device_path_clone)
                                                .and_then(|adapter| adapter.active_connection.clone())
                                        };

                                        let networks: Vec<(String, u8, bool)> = access_points
                                            .iter()
                                            .map(|ap| (ap.ssid.clone(), ap.strength, ap.secure))
                                            .collect();
                                        menu_clone.set_networks(networks);
                                        menu_clone.set_active_ssid(active_ssid.clone());

                                        // Update toggle details
                                        let count = access_points.len();
                                        if let Some(ref ssid) = active_ssid {
                                            // Show connected SSID
                                            toggle_clone.set_details(Some(ssid.clone()));
                                        } else if count > 0 {
                                            toggle_clone.set_details(Some(format!("{} network{} available", count, if count == 1 { "" } else { "s" })));
                                        } else {
                                            toggle_clone.set_details(Some("No networks found".to_string()));
                                        }

                                        return glib::ControlFlow::Break;
                                    }
                                    Ok(None) => {
                                        return glib::ControlFlow::Break;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                                        *rx.borrow_mut() = Some(receiver);
                                        return glib::ControlFlow::Continue;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                        return glib::ControlFlow::Break;
                                    }
                                }
                            }
                            glib::ControlFlow::Break
                        });
                    }
                }
            });

            // Connect menu output handler
            let device_path = path.clone();
            let store_clone = self.store.clone();
            let menu_clone = menu.clone();
            let toggle_clone2 = toggle.clone();
            let dbus_clone2 = self.dbus.clone();
            menu.connect_output(move |output| {
                debug!("WiFi menu output: {:?}", output);
                let device_path = device_path.clone();
                let store = store_clone.clone();
                let menu = menu_clone.clone();
                let toggle = toggle_clone2.clone();
                let dbus = dbus_clone2.clone();

                match output {
                    WiFiMenuOutput::Connect(ssid) => {
                        debug!("Connecting to WiFi network: {}", ssid);
                        menu.set_connecting(&ssid, true);

                        // Use separate thread with tokio runtime for DBus work
                        let (tx, rx) = std::sync::mpsc::channel();
                        let ssid_clone = ssid.clone();
                        let device_path_clone = device_path.clone();

                        std::thread::spawn(move || {
                            tokio::runtime::Runtime::new()
                                .unwrap()
                                .block_on(async move {
                                    // Find connection for this SSID
                                    match dbus::get_connections_for_ssid(&dbus, &ssid_clone).await {
                                        Ok(connections) => {
                                            if let Some(conn_path) = connections.first() {
                                                // Activate existing connection
                                                match dbus::activate_connection(
                                                    &dbus,
                                                    Some(conn_path),
                                                    &device_path_clone,
                                                    None,
                                                )
                                                .await
                                                {
                                                    Ok(_) => {
                                                        let _ = tx.send(Ok(ssid_clone.clone()));
                                                    }
                                                    Err(e) => {
                                                        let _ = tx.send(Err(format!("Failed to activate connection: {}", e)));
                                                    }
                                                }
                                            } else {
                                                let _ = tx.send(Err(format!("No saved connection found for SSID: {}", ssid_clone)));
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Err(format!("Failed to get connections: {}", e)));
                                        }
                                    }
                                });
                        });

                        // Poll for results in glib main loop
                        let rx = std::rc::Rc::new(std::cell::RefCell::new(Some(rx)));
                        let ssid_for_cleanup = ssid.clone();
                        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                            // Take the receiver out to avoid holding the borrow
                            let receiver_opt = rx.borrow_mut().take();

                            if let Some(receiver) = receiver_opt {
                                match receiver.try_recv() {
                                    Ok(Ok(connected_ssid)) => {
                                        info!("Successfully activated WiFi connection");
                                        store.emit(NetworkOp::SetActiveWiFiConnection(
                                            device_path.clone(),
                                            Some(connected_ssid.clone()),
                                        ));
                                        // Update toggle details to show connected SSID
                                        toggle.set_details(Some(connected_ssid.clone()));
                                        menu.set_active_ssid(Some(connected_ssid.clone()));
                                        menu.set_connecting(&ssid_for_cleanup, false);
                                        return glib::ControlFlow::Break;
                                    }
                                    Ok(Err(err_msg)) => {
                                        error!("{}", err_msg);
                                        menu.set_connecting(&ssid_for_cleanup, false);
                                        return glib::ControlFlow::Break;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                                        // Put receiver back and continue polling
                                        *rx.borrow_mut() = Some(receiver);
                                        return glib::ControlFlow::Continue;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                        // Thread died
                                        menu.set_connecting(&ssid_for_cleanup, false);
                                        return glib::ControlFlow::Break;
                                    }
                                }
                            }
                            glib::ControlFlow::Break
                        });
                    }
                    WiFiMenuOutput::Disconnect => {
                        debug!("Disconnecting from WiFi");
                    }
                    WiFiMenuOutput::Scan => {
                        // Scan is now automatic when menu opens, this shouldn't be reached
                        debug!("Scan button clicked (shouldn't happen - auto-scan on menu open)");
                    }
                }
            });

            self.wifi_uis.borrow_mut().insert(
                path.clone(),
                WiFiAdapterUI {
                    toggle,
                    menu,
                    expand_callback,
                },
            );
        }

        Ok(())
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        let mut toggles = Vec::new();

        // Add WiFi toggles
        let wifi_uis = self.wifi_uis.borrow();
        for (_, ui) in wifi_uis.iter() {
            toggles.push(Arc::new(WidgetFeatureToggle {
                el: ui.toggle.widget(),
                weight: 100,
                menu: Some(ui.menu.widget()),
                on_expand_toggled: Some(ui.expand_callback.clone()),
                menu_id: Some(ui.toggle.menu_id.clone()),
            }));
        }

        // Add Ethernet toggles
        let ethernet_uis = self.ethernet_uis.borrow();
        for (_, ui) in ethernet_uis.iter() {
            toggles.push(Arc::new(WidgetFeatureToggle {
                el: ui.toggle.widget(),
                weight: 101,
                menu: Some(ui.menu.widget()),
                on_expand_toggled: Some(ui.expand_callback.clone()),
                menu_id: Some(ui.toggle.menu_id.clone()),
            }));
        }

        toggles
    }
}
