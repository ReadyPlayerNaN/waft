//! VPN adapter widget.
//!
//! Coordinates the VPN toggle and menu widgets, handling state synchronization
//! and D-Bus operations for VPN connections.

#![allow(dead_code)] // NetworkManager plugin is under development

use crate::dbus::DbusHandle;
use crate::menu_state::{MenuOp, MenuStore};
use crate::plugin::WidgetFeatureToggle;
use crate::ui::feature_toggle::FeatureToggleOutput;
use log::{debug, error, info};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc; // DbusHandle is Arc

use super::dbus;
use super::store::{NetworkOp, NetworkStore, VpnConnectionState, VpnState};
use super::vpn_menu::{VpnMenuOutput, VpnMenuWidget};
use super::vpn_toggle::VpnToggleWidget;

#[derive(Clone)]
pub struct VpnWidget {
    store: Rc<NetworkStore>,
    dbus: Arc<DbusHandle>,
    menu_store: Rc<MenuStore>,
    toggle: VpnToggleWidget,
    menu: VpnMenuWidget,
    // Maps connection path -> active connection path when connected
    active_connections: Rc<RefCell<HashMap<String, String>>>,
}

impl VpnWidget {
    pub fn new(
        connections: &HashMap<String, VpnConnectionState>,
        store: Rc<NetworkStore>,
        dbus: Arc<DbusHandle>,
        menu_store: Rc<MenuStore>,
    ) -> Self {
        // Find if any VPN is connected
        let (connected_name, current_state) = Self::derive_overall_state(connections);

        let toggle = VpnToggleWidget::new(connected_name, current_state, menu_store.clone());
        let menu = VpnMenuWidget::new();

        // Populate menu with connections
        let menu_data: Vec<(String, String, VpnState)> = connections
            .values()
            .map(|c| (c.path.clone(), c.name.clone(), c.state.clone()))
            .collect();
        menu.set_connections(menu_data);

        let mut widget = Self {
            store,
            dbus,
            menu_store,
            toggle,
            menu,
            active_connections: Rc::new(RefCell::new(HashMap::new())),
        };

        widget.setup_toggle_handlers();
        widget.setup_menu_handlers();
        widget.setup_expand_callback();

        widget
    }

    pub fn widget(&self) -> Rc<WidgetFeatureToggle> {
        Rc::new(WidgetFeatureToggle {
            id: "networkmanager:vpn".to_string(),
            el: self.toggle.widget(),
            weight: 103, // After WiFi (102) and Wired (101)
            menu: Some(self.menu.widget()),
            on_expand_toggled: Some(self.toggle.expand_callback()),
            menu_id: Some(self.toggle.menu_id()),
        })
    }

    fn derive_overall_state(
        connections: &HashMap<String, VpnConnectionState>,
    ) -> (Option<String>, VpnState) {
        // Check for any connected or transitional VPN
        for conn in connections.values() {
            match conn.state {
                VpnState::Connected => return (Some(conn.name.clone()), VpnState::Connected),
                VpnState::Connecting => return (Some(conn.name.clone()), VpnState::Connecting),
                VpnState::Disconnecting => {
                    return (Some(conn.name.clone()), VpnState::Disconnecting);
                }
                VpnState::Disconnected => {}
            }
        }
        (None, VpnState::Disconnected)
    }

    fn setup_toggle_handlers(&mut self) {
        let store = self.store.clone();
        let dbus = self.dbus.clone();
        let active_connections = self.active_connections.clone();
        let menu_store = self.menu_store.clone();
        let menu_id = self.toggle.menu_id();

        self.toggle.connect_output(move |event| {
            debug!("VPN toggle event: {:?}", event);

            match event {
                FeatureToggleOutput::Activate => {
                    // When disconnected, expand menu so user can select which VPN to connect
                    debug!("VPN activate - expanding menu for VPN selection");
                    menu_store.emit(MenuOp::OpenMenu(menu_id.clone()));
                }
                FeatureToggleOutput::Deactivate => {
                    // Disconnect all active VPNs
                    info!("VPN toggle: deactivating all VPNs");

                    let active_conns: Vec<(String, String)> = active_connections
                        .borrow()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();

                    for (conn_path, active_path) in active_conns {
                        let dbus_clone = dbus.clone();
                        let store_clone = store.clone();
                        let conn_path_clone = conn_path.clone();
                        let active_path_clone = active_path.clone();

                        // Set state to disconnecting
                        store_clone.emit(NetworkOp::SetVpnState(
                            conn_path_clone.clone(),
                            VpnState::Disconnecting,
                        ));

                        glib::spawn_future_local(async move {
                            match crate::runtime::spawn_on_tokio(
                                dbus::deactivate_vpn_connection_sendable(
                                    dbus_clone,
                                    active_path_clone,
                                ),
                            )
                            .await
                            {
                                Ok(()) => {
                                    info!("Successfully disconnected VPN: {}", conn_path_clone);
                                }
                                Err(e) => {
                                    error!("Failed to disconnect VPN {}: {}", conn_path_clone, e);
                                    // Reset state on error
                                    store_clone.emit(NetworkOp::SetVpnState(
                                        conn_path_clone,
                                        VpnState::Connected,
                                    ));
                                }
                            }
                        });
                    }
                }
            }
        });
    }

    fn setup_menu_handlers(&mut self) {
        let store = self.store.clone();
        let dbus = self.dbus.clone();
        let active_connections = self.active_connections.clone();

        self.menu.connect_output(move |event| {
            debug!("VPN menu event: {:?}", event);

            match event {
                VpnMenuOutput::Connect(conn_path) => {
                    info!("Connecting VPN: {}", conn_path);

                    let dbus_clone = dbus.clone();
                    let store_clone = store.clone();
                    let conn_path_clone = conn_path.clone();
                    let active_conns = active_connections.clone();

                    // Set state to connecting
                    store_clone.emit(NetworkOp::SetVpnState(
                        conn_path.clone(),
                        VpnState::Connecting,
                    ));

                    glib::spawn_future_local(async move {
                        match crate::runtime::spawn_on_tokio(
                            dbus::activate_vpn_connection_sendable(
                                dbus_clone,
                                conn_path_clone.clone(),
                            ),
                        )
                        .await
                        {
                            Ok(active_path) => {
                                info!(
                                    "Successfully initiated VPN connection: {} -> {}",
                                    conn_path_clone, active_path
                                );
                                active_conns
                                    .borrow_mut()
                                    .insert(conn_path_clone, active_path);
                            }
                            Err(e) => {
                                error!("Failed to connect VPN {}: {}", conn_path_clone, e);
                                // Reset state on error
                                store_clone.emit(NetworkOp::SetVpnState(
                                    conn_path_clone,
                                    VpnState::Disconnected,
                                ));
                            }
                        }
                    });
                }
                VpnMenuOutput::Disconnect(conn_path) => {
                    info!("Disconnecting VPN: {}", conn_path);

                    let active_path = match active_connections.borrow().get(&conn_path) {
                        Some(path) => path.clone(),
                        None => {
                            error!("No active connection found for VPN: {}", conn_path);
                            return;
                        }
                    };

                    let dbus_clone = dbus.clone();
                    let store_clone = store.clone();
                    let conn_path_clone = conn_path.clone();
                    let active_conns = active_connections.clone();

                    // Set state to disconnecting
                    store_clone.emit(NetworkOp::SetVpnState(
                        conn_path.clone(),
                        VpnState::Disconnecting,
                    ));

                    glib::spawn_future_local(async move {
                        match crate::runtime::spawn_on_tokio(
                            dbus::deactivate_vpn_connection_sendable(dbus_clone, active_path),
                        )
                        .await
                        {
                            Ok(()) => {
                                info!("Successfully disconnected VPN: {}", conn_path_clone);
                                active_conns.borrow_mut().remove(&conn_path_clone);
                            }
                            Err(e) => {
                                error!("Failed to disconnect VPN {}: {}", conn_path_clone, e);
                                // Reset state on error
                                store_clone.emit(NetworkOp::SetVpnState(
                                    conn_path_clone,
                                    VpnState::Connected,
                                ));
                            }
                        }
                    });
                }
            }
        });
    }

    fn setup_expand_callback(&mut self) {
        let menu = self.menu.clone();
        let store = self.store.clone();

        self.toggle.set_expand_callback(move |expanded| {
            if expanded {
                debug!("VPN menu expanded - refreshing connections");
                // Refresh menu with current state from store
                let state = store.get_state();
                let connections: Vec<(String, String, VpnState)> = state
                    .vpn_connections
                    .values()
                    .map(|c| (c.path.clone(), c.name.clone(), c.state.clone()))
                    .collect();
                menu.set_connections(connections);
            }
        });
    }

    /// Update the widget to reflect current store state.
    pub fn sync_state(&self) {
        let state = self.store.get_state();
        let (connected_name, overall_state) = Self::derive_overall_state(&state.vpn_connections);
        self.toggle.update_state(connected_name, overall_state);

        // Update menu
        let connections: Vec<(String, String, VpnState)> = state
            .vpn_connections
            .values()
            .map(|c| (c.path.clone(), c.name.clone(), c.state.clone()))
            .collect();
        self.menu.set_connections(connections);
    }

    /// Update a single VPN connection's state.
    pub fn update_connection_state(&self, path: &str, state: VpnState) {
        self.menu.set_connection_state(path, state.clone());

        // Update toggle based on new overall state
        let store_state = self.store.get_state();
        let (connected_name, overall_state) =
            Self::derive_overall_state(&store_state.vpn_connections);
        self.toggle.update_state(connected_name, overall_state);
    }

    /// Set the mapping of connection path to active connection path.
    pub fn set_active_connection(&self, conn_path: &str, active_path: Option<String>) {
        let mut active = self.active_connections.borrow_mut();
        if let Some(path) = active_path {
            active.insert(conn_path.to_string(), path);
        } else {
            active.remove(conn_path);
        }
    }
}
