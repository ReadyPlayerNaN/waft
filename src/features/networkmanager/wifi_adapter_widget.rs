use crate::dbus::DbusHandle;
use crate::menu_state::MenuStore;
use crate::plugin::WidgetFeatureToggle;
use crate::ui::feature_toggle_expandable::FeatureToggleExpandableOutput;
use log::{debug, error, info};
use nmrs::NetworkManager;
use std::sync::Arc;

use super::dbus;
use super::store::{AccessPointState, NetworkOp, NetworkStore, WiFiAdapterState};
use super::wifi_menu::{WiFiMenuOutput, WiFiMenuWidget};
use super::wifi_toggle::WiFiToggleWidget;

#[derive(Clone)]
pub struct WiFiAdapterWidget {
    path: String,
    store: Arc<NetworkStore>,
    nm: Option<NetworkManager>,
    dbus: Arc<DbusHandle>,
    toggle: WiFiToggleWidget,
    menu: WiFiMenuWidget,
}

impl WiFiAdapterWidget {
    pub fn new(
        adapter: &WiFiAdapterState,
        store: Arc<NetworkStore>,
        nm: Option<NetworkManager>,
        dbus: Arc<DbusHandle>,
        menu_store: Arc<MenuStore>,
    ) -> Self {
        let toggle = WiFiToggleWidget::new(
            adapter.interface_name.clone(),
            adapter.enabled,
            adapter.active_connection.clone(),
            adapter.access_points.len(),
            menu_store,
        );

        let menu = WiFiMenuWidget::new();

        let networks: Vec<(String, u8, bool)> = adapter
            .access_points
            .values()
            .map(|ap| (ap.ssid.clone(), ap.strength, ap.secure))
            .collect();
        menu.set_networks(networks);
        menu.set_active_ssid(adapter.active_connection.clone());

        let mut widget = Self {
            path: adapter.path.clone(),
            store,
            nm,
            dbus,
            toggle,
            menu,
        };

        widget.setup_toggle_handlers();
        widget.setup_expand_callback();
        widget.setup_menu_handlers();

        widget
    }

    pub fn widget(&self) -> Arc<WidgetFeatureToggle> {
        Arc::new(WidgetFeatureToggle {
            id: format!("networkmanager:wifi:{}", self.path),
            el: self.toggle.widget(),
            weight: 100,
            menu: Some(self.menu.widget()),
            on_expand_toggled: Some(self.toggle.expand_callback()),
            menu_id: Some(self.toggle.menu_id()),
        })
    }

    fn setup_toggle_handlers(&mut self) {
        let device_path = self.path.clone();
        let store_clone = self.store.clone();
        let nm_clone = self.nm.clone();

        self.toggle.connect_output(move |event| {
            debug!("WiFi toggle event: {:?}", event);
            let device_path = device_path.clone();
            let store = store_clone.clone();
            let nm = nm_clone.clone();

            match event {
                FeatureToggleExpandableOutput::Activate
                | FeatureToggleExpandableOutput::Deactivate => {
                    let enabled = matches!(event, FeatureToggleExpandableOutput::Activate);
                    store.emit(NetworkOp::SetWiFiBusy(device_path.clone(), true));

                    let (tx, rx) = std::sync::mpsc::channel();

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

                    let rx = std::rc::Rc::new(std::cell::RefCell::new(Some(rx)));
                    let device_path_for_poll = device_path.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                        let receiver_opt = rx.borrow_mut().take();

                        if let Some(receiver) = receiver_opt {
                            match receiver.try_recv() {
                                Ok(enabled) => {
                                    store.emit(NetworkOp::SetWiFiEnabled(device_path_for_poll.clone(), enabled));
                                    store.emit(NetworkOp::SetWiFiBusy(device_path_for_poll.clone(), false));
                                    return glib::ControlFlow::Break;
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
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
    }

    fn setup_expand_callback(&mut self) {
        let device_path_for_expand = self.path.clone();
        let store_for_expand = self.store.clone();
        let menu_for_expand = self.menu.clone();
        let toggle_for_expand = self.toggle.clone();
        let nm_for_expand = self.nm.clone();
        let dbus_for_expand = self.dbus.clone();

        self.toggle.set_expand_callback(move |will_be_open: bool| {
            if will_be_open {
                debug!("WiFi menu opening, scanning for networks");
                let (tx, rx) = std::sync::mpsc::channel();
                let nm = nm_for_expand.clone();
                let dbus = dbus_for_expand.clone();

                std::thread::spawn(move || {
                    tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(async move {
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

                            match dbus::list_networks_nmrs(nm).await {
                                Ok(aps) => {
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

                                let count = access_points.len();
                                if let Some(ref ssid) = active_ssid {
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
        });
    }

    fn setup_menu_handlers(&mut self) {
        let device_path = self.path.clone();
        let store_clone = self.store.clone();
        let menu_clone = self.menu.clone();
        let toggle_clone = self.toggle.clone();
        let dbus_clone = self.dbus.clone();

        self.menu.connect_output(move |output| {
            debug!("WiFi menu output: {:?}", output);
            let device_path = device_path.clone();
            let store = store_clone.clone();
            let menu = menu_clone.clone();
            let toggle = toggle_clone.clone();
            let dbus = dbus_clone.clone();

            match output {
                WiFiMenuOutput::Connect(ssid) => {
                    debug!("Connecting to WiFi network: {}", ssid);
                    menu.set_connecting(&ssid, true);

                    let (tx, rx) = std::sync::mpsc::channel();
                    let ssid_clone = ssid.clone();
                    let device_path_clone = device_path.clone();

                    std::thread::spawn(move || {
                        tokio::runtime::Runtime::new()
                            .unwrap()
                            .block_on(async move {
                                match dbus::get_connections_for_ssid(&dbus, &ssid_clone).await {
                                    Ok(connections) => {
                                        if let Some(conn_path) = connections.first() {
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

                    let rx = std::rc::Rc::new(std::cell::RefCell::new(Some(rx)));
                    let ssid_for_cleanup = ssid.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                        let receiver_opt = rx.borrow_mut().take();

                        if let Some(receiver) = receiver_opt {
                            match receiver.try_recv() {
                                Ok(Ok(connected_ssid)) => {
                                    info!("Successfully activated WiFi connection");
                                    store.emit(NetworkOp::SetActiveWiFiConnection(
                                        device_path.clone(),
                                        Some(connected_ssid.clone()),
                                    ));
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
                                    *rx.borrow_mut() = Some(receiver);
                                    return glib::ControlFlow::Continue;
                                }
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
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
                    debug!("Scan button clicked (shouldn't happen - auto-scan on menu open)");
                }
            }
        });
    }

    #[allow(dead_code)]
    pub fn sync_state(&self, state: &WiFiAdapterState) {
        self.toggle.set_active(state.enabled);
        self.toggle.set_busy(state.busy);
        self.toggle.update_state(state.enabled, state.active_connection.clone(), state.access_points.len());

        let networks: Vec<(String, u8, bool)> = state
            .access_points
            .values()
            .map(|ap| (ap.ssid.clone(), ap.strength, ap.secure))
            .collect();
        self.menu.set_networks(networks);
        self.menu.set_active_ssid(state.active_connection.clone());
    }
}
