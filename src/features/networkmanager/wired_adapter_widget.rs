use crate::dbus::DbusHandle;
use crate::menu_state::MenuStore;
use crate::plugin::WidgetFeatureToggle;
use crate::ui::feature_toggle_expandable::{
    FeatureToggleExpandableOutput, FeatureToggleExpandableProps, FeatureToggleExpandableWidget,
};
use log::{debug, error, info};
use nmrs::NetworkManager;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::dbus;
use super::ethernet_menu::{ConnectionDetails, EthernetMenuWidget};
use super::store::{EthernetAdapterState, NetworkStore};

pub type ExpandCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

#[derive(Clone)]
pub struct WiredAdapterWidget {
    path: String,
    store: Arc<NetworkStore>,
    nm: Option<NetworkManager>,
    dbus: Arc<DbusHandle>,
    toggle: FeatureToggleExpandableWidget,
    menu: EthernetMenuWidget,
    expand_callback: ExpandCallback,
}

impl WiredAdapterWidget {
    pub fn new(
        adapter: &EthernetAdapterState,
        store: Arc<NetworkStore>,
        nm: Option<NetworkManager>,
        dbus: Arc<DbusHandle>,
        menu_store: Arc<MenuStore>,
    ) -> Self {
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
            menu_store,
        );

        let menu = EthernetMenuWidget::new();
        let expand_callback: ExpandCallback = Rc::new(RefCell::new(None));

        let mut widget = Self {
            path: adapter.path.clone(),
            store,
            nm,
            dbus,
            toggle,
            menu,
            expand_callback,
        };

        widget.setup_toggle_handlers();
        widget.setup_expand_callback();

        widget
    }

    pub fn widget(&self) -> Arc<WidgetFeatureToggle> {
        Arc::new(WidgetFeatureToggle {
            el: self.toggle.widget(),
            weight: 101,
            menu: Some(self.menu.widget()),
            on_expand_toggled: Some(self.expand_callback.clone()),
            menu_id: Some(self.toggle.menu_id.clone()),
        })
    }

    fn setup_toggle_handlers(&mut self) {
        let device_path = self.path.clone();
        let nm_clone = self.nm.clone();

        self.toggle.connect_output(move |event| {
            debug!("Ethernet toggle event: {:?}", event);
            let device_path = device_path.clone();
            let nm = nm_clone.clone();

            match event {
                FeatureToggleExpandableOutput::Activate
                | FeatureToggleExpandableOutput::Deactivate => {
                    let enabled = matches!(event, FeatureToggleExpandableOutput::Activate);

                    info!("Ethernet toggle: enabled={}, device={}", enabled, device_path);

                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        tokio::runtime::Runtime::new()
                            .unwrap()
                            .block_on(async move {
                                if let Some(nm) = nm {
                                    if enabled {
                                        match dbus::connect_wired_nmrs(&nm).await {
                                            Ok(_) => {
                                                info!("Successfully activated ethernet device");
                                            }
                                            Err(e) => {
                                                error!("Failed to activate ethernet device: {}", e);
                                            }
                                        }
                                    } else {
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

                    let rx = std::rc::Rc::new(std::cell::RefCell::new(Some(rx)));
                    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                        let receiver_opt = rx.borrow_mut().take();
                        if let Some(receiver) = receiver_opt {
                            match receiver.try_recv() {
                                Ok(_enabled) => {
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
    }

    fn setup_expand_callback(&mut self) {
        let menu_clone = self.menu.clone();
        let device_path_clone = self.path.clone();
        let dbus_clone = self.dbus.clone();

        let expand_cb = move |expanded: bool| {
            if expanded {
                debug!("Fetching ethernet connection details for {}", device_path_clone);
                let menu = menu_clone.clone();
                let device_path = device_path_clone.clone();
                let dbus = dbus_clone.clone();

                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    tokio::runtime::Runtime::new()
                        .unwrap()
                        .block_on(async move {
                            let mut details = ConnectionDetails::default();

                            if let Ok(Some(speed)) = dbus::get_link_speed(&dbus, &device_path).await {
                                if speed >= 1000 {
                                    details.link_speed = Some(format!("{} Gbps", speed / 1000));
                                } else {
                                    details.link_speed = Some(format!("{} Mbps", speed));
                                }
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
        };
        *self.expand_callback.borrow_mut() = Some(Box::new(expand_cb));
    }

    #[allow(dead_code)]
    pub fn sync_state(&self, state: &EthernetAdapterState) {
        let is_connected = state.device_state == 100;

        let details = if state.enabled {
            if is_connected {
                Some(crate::i18n::t("network-connected"))
            } else if state.carrier {
                Some(crate::i18n::t("network-disconnected"))
            } else {
                Some(crate::i18n::t("network-disconnected"))
            }
        } else {
            Some(crate::i18n::t("network-disabled"))
        };

        let icon = if state.enabled {
            if is_connected {
                "network-wired-symbolic"
            } else if state.carrier {
                "network-wired-disconnected-symbolic"
            } else {
                "network-wired-disconnected-symbolic"
            }
        } else {
            "network-wired-offline-symbolic"
        };

        self.toggle.set_active(state.enabled);
        self.toggle.set_icon(icon);
        self.toggle.set_details(details);
    }
}
