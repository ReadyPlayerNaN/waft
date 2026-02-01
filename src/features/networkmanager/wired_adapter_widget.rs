#![allow(dead_code)] // NetworkManager plugin is under development

use crate::dbus::DbusHandle;
use crate::menu_state::MenuStore;
use crate::plugin::WidgetFeatureToggle;
use crate::ui::feature_toggle_expandable::FeatureToggleExpandableOutput;
use log::{debug, error, info};
use nmrs::NetworkManager;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::dbus;
use super::ethernet_menu::{ConnectionDetails, EthernetMenuWidget};
use super::store::{EthernetAdapterState, NetworkStore};
use super::wired_toggle_widget::WiredToggleWidget;

#[derive(Clone)]
pub struct WiredAdapterWidget {
    path: String,
    store: Arc<NetworkStore>,
    nm: Option<NetworkManager>,
    dbus: Arc<DbusHandle>,
    toggle: WiredToggleWidget,
    menu: EthernetMenuWidget,
}

impl WiredAdapterWidget {
    pub fn new(
        adapter: &EthernetAdapterState,
        store: Arc<NetworkStore>,
        nm: Option<NetworkManager>,
        dbus: Arc<DbusHandle>,
        menu_store: Arc<MenuStore>,
    ) -> Self {
        let toggle = WiredToggleWidget::new(
            adapter.interface_name.clone(),
            adapter.enabled,
            adapter.carrier,
            adapter.device_state,
            menu_store,
        );

        let menu = EthernetMenuWidget::new();

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

        widget
    }

    pub fn widget(&self) -> Arc<WidgetFeatureToggle> {
        Arc::new(WidgetFeatureToggle {
            id: format!("networkmanager:wired:{}", self.path),
            el: self.toggle.widget(),
            weight: 101,
            menu: Some(self.menu.widget()),
            on_expand_toggled: Some(self.toggle.expand_callback()),
            menu_id: Some(self.toggle.menu_id()),
        })
    }

    fn setup_toggle_handlers(&mut self) {
        let device_path = self.path.clone();
        let nm_clone = self.nm.clone();
        let dbus_clone = self.dbus.clone();

        self.toggle.connect_output(move |event| {
            debug!("Ethernet toggle event: {:?}", event);
            let device_path = device_path.clone();
            let nm = nm_clone.clone();
            let dbus = dbus_clone.clone();

            match event {
                FeatureToggleExpandableOutput::Activate
                | FeatureToggleExpandableOutput::Deactivate => {
                    let enabled = matches!(event, FeatureToggleExpandableOutput::Activate);

                    info!(
                        "Ethernet toggle: enabled={}, device={}",
                        enabled, device_path
                    );

                    glib::spawn_future_local(async move {
                        if enabled {
                            // Use nmrs for connecting (it auto-activates wired)
                            if let Some(nm) = nm {
                                match crate::runtime::spawn_on_tokio(async move {
                                    dbus::connect_wired_nmrs(&nm).await
                                })
                                .await
                                {
                                    Ok(_) => {
                                        info!("Successfully activated ethernet device");
                                    }
                                    Err(e) => {
                                        error!("Failed to activate ethernet device: {}", e);
                                    }
                                }
                            } else {
                                error!("NetworkManager not available");
                            }
                        } else {
                            // Use D-Bus to disconnect the specific device
                            match crate::runtime::spawn_on_tokio(dbus::disconnect_device_sendable(
                                dbus.clone(),
                                device_path.clone(),
                            ))
                            .await
                            {
                                Ok(_) => {
                                    info!(
                                        "Successfully disconnected ethernet device: {}",
                                        device_path
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to disconnect ethernet device {}: {}",
                                        device_path, e
                                    );
                                }
                            }
                        }
                    });
                }
                FeatureToggleExpandableOutput::ToggleExpand => {
                    // Expand is handled by the menu system automatically
                }
            }
        });
    }

    fn setup_expand_callback(&mut self) {
        let menu_clone = self.menu.clone();
        let device_path_clone = self.path.clone();
        let dbus_clone = self.dbus.clone();

        self.toggle.set_expand_callback(move |expanded: bool| {
            if expanded {
                debug!(
                    "Fetching ethernet connection details for {}",
                    device_path_clone
                );
                let menu = menu_clone.clone();
                let device_path = device_path_clone.clone();
                let dbus = dbus_clone.clone();

                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(async move {
                            let mut details = ConnectionDetails::default();

                            if let Ok(Some(speed)) = dbus::get_link_speed(&dbus, &device_path).await
                            {
                                if speed >= 1000 {
                                    details.link_speed = Some(format!("{} Gbps", speed / 1000));
                                } else {
                                    details.link_speed = Some(format!("{} Mbps", speed));
                                }
                            }

                            if let Ok(ip_config) =
                                dbus::get_ip_configuration(&dbus, &device_path).await
                            {
                                details.ipv4_address = ip_config.ipv4_address;
                                details.ipv6_address = ip_config.ipv6_address;
                                details.subnet_mask = ip_config.subnet_mask;
                                details.gateway = ip_config.gateway;
                            }

                            let _ = tx.send(details);
                        });
                });

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
        });
    }

    pub fn sync_state(&self, state: &EthernetAdapterState) {
        self.toggle
            .update_state(state.enabled, state.carrier, state.device_state);
    }
}
