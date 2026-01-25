//! Notifications plugin - DBus notification handling and display.

use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, info};
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::*;

use crate::features::notifications::store::{create_notification_store, NotificationOp, NotificationStore};
use crate::plugin::WidgetFeatureToggle;
use crate::plugin::{Plugin, PluginId};
use crate::features::notifications::ui::toast_window::{HPos, ToastWindowOutput, ToastWindowWidget, VPos};
use crate::features::notifications::ui::notifications_widget::{NotificationsWidget, NotificationsWidgetOutput};
use crate::plugin::{Slot, Widget};

use self::dbus::client::{IngressEvent, OutboundEvent, close_reasons};
use self::dbus::server::NotificationsDbusServer;
use self::debounce::NotificationDebouncer;
use self::dnd_toggle::{DoNotDisturbToggleInit, DoNotDisturbToggleOutput, DoNotDisturbToggleWidget};

pub mod dbus;
mod debounce;
mod dnd_toggle;
pub mod store;
pub mod types;
pub mod ui;

fn default_toast_limit() -> u32 {
    3
}

/// Configuration for the notifications plugin.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct NotificationsConfig {
    #[serde(default = "default_toast_limit")]
    pub toast_limit: u32,
    #[serde(default)]
    pub disable_toasts: bool,
}

impl NotificationsConfig {
    /// Get toast limit as usize, ensuring minimum of 1.
    pub fn toast_limit(&self) -> usize {
        self.toast_limit.max(1) as usize
    }
}

pub struct NotificationsPlugin {
    store: Rc<NotificationStore>,
    client_channel: flume::Sender<OutboundEvent>,
    client_receiver: flume::Receiver<OutboundEvent>,
    config: NotificationsConfig,
    dbus_server: Option<NotificationsDbusServer>,
    dnd_toggle: Rc<RefCell<Option<DoNotDisturbToggleWidget>>>,
    notifications_widget: Rc<RefCell<Option<NotificationsWidget>>>,
    server_channel: flume::Sender<IngressEvent>,
    server_receiver: flume::Receiver<IngressEvent>,
    tick_source: Arc<std::sync::Mutex<Option<glib::SourceId>>>,
    toast: Option<ToastWindowWidget>,
    debouncer: Option<NotificationDebouncer>,
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        let (client_tx, client_rx) = flume::unbounded();
        let (server_tx, server_rx) = flume::unbounded();
        let store = Rc::new(create_notification_store());

        Self {
            store,
            client_channel: client_tx,
            client_receiver: client_rx,
            config: NotificationsConfig::default(),
            dbus_server: None,
            dnd_toggle: Rc::new(RefCell::new(None)),
            notifications_widget: Rc::new(RefCell::new(None)),
            server_channel: server_tx,
            server_receiver: server_rx,
            tick_source: Arc::new(std::sync::Mutex::new(None)),
            toast: None,
            debouncer: None,
        }
    }

    fn create_toast_window(&self) -> ToastWindowWidget {
        ToastWindowWidget::new(self.store.clone(), HPos::Center, VPos::Top)
    }

    fn schedule_tick(&self) {
        let tick_source = self.tick_source.clone();
        let tick_source_for_closure = self.tick_source.clone();
        let store = self.store.clone();
        let store_for_interval = self.store.clone();

        *tick_source.lock().unwrap() = Some(glib::timeout_add_local_once(
            Duration::from_millis(200),
            move || {
                store.emit(NotificationOp::Tick);

                // Schedule the next tick
                *tick_source_for_closure.lock().unwrap() = Some(glib::timeout_add_local(
                    Duration::from_millis(200),
                    move || {
                        store_for_interval.emit(NotificationOp::Tick);
                        glib::ControlFlow::Continue
                    },
                ));
            },
        ));
    }
}

#[async_trait(?Send)]
impl Plugin for NotificationsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::notifications")
    }

    fn configure(&mut self, settings: &toml::Table) -> Result<()> {
        self.config = settings.clone().try_into()?;
        debug!("Configured notifications plugin: {:?}", self.config);
        Ok(())
    }

    async fn init(&mut self) -> Result<()> {
        let mut dbus_server = NotificationsDbusServer::connect()
            .await
            .context("Failed to connect DBus notifications server")?;

        info!("Starting notifications dbus server");
        dbus_server
            .start(
                self.server_channel.clone(),
                self.client_receiver.clone(),
            )
            .await
            .context("Failed to start DBus notifications server")?;

        self.dbus_server = Some(dbus_server);

        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        // Configure the store with plugin settings
        self.store.emit(NotificationOp::Configure {
            toast_limit: self.config.toast_limit(),
            disable_toasts: self.config.disable_toasts,
        });

        let (debouncer_tx, debouncer_rx) = flume::unbounded();

        // Forward debouncer output to store
        let store_for_debounce = self.store.clone();
        glib::spawn_future_local(async move {
            while let Ok(op) = debouncer_rx.recv_async().await {
                store_for_debounce.emit(op);
            }
        });

        let debouncer = NotificationDebouncer::new(debouncer_tx);
        let db_server = debouncer.clone();
        let db_toast = debouncer.clone();

        self.debouncer = Some(debouncer);

        // Create pure GTK4 toast window with store reference
        let toast = self.create_toast_window();

        // Add window to application
        let app = gtk::Application::default();
        app.add_window(&toast.window);

        // Connect output handler for toast events
        let outbound_tx_toast = self.client_channel.clone();
        toast.connect_output(move |event| {
            debug!("[toast] Received: {:?}", event);
            match event {
                ToastWindowOutput::ActionClick(id, action_key) => {
                    let _ = db_toast.send(NotificationOp::NotificationDismiss(id));
                    let _ = outbound_tx_toast.send(OutboundEvent::ActionInvoked {
                        id: id as u32,
                        action_key: action_key.clone(),
                    });
                    let _ = outbound_tx_toast.send(OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: close_reasons::DISMISSED_BY_USER,
                    });
                }

                ToastWindowOutput::CardClose(id) => {
                    let _ = outbound_tx_toast.send(OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: close_reasons::DISMISSED_BY_USER,
                    });
                    let _ = db_toast.send(NotificationOp::NotificationDismiss(id));
                }

                ToastWindowOutput::TimedOut(id) => {
                    let _ = outbound_tx_toast.send(OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: close_reasons::EXPIRED,
                    });
                }

                ToastWindowOutput::CardClick(_id) => {}
            }
        });

        self.toast = Some(toast);

        let server_receiver = self.server_receiver.clone();
        let outbound_tx = self.client_channel.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = server_receiver.recv_async().await {
                match event {
                    IngressEvent::Notify { notification } => {
                        let _ = db_server.send(NotificationOp::Ingress(notification));
                    }

                    IngressEvent::CloseNotification { id } => {
                        let _ = db_server.send(NotificationOp::NotificationRetract(id as u64));

                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id,
                            reason: close_reasons::CLOSED_BY_CALL,
                        });
                    }
                }
            }
        });

        // Create DnD toggle
        let dnd_toggle = DoNotDisturbToggleWidget::new(DoNotDisturbToggleInit {
            active: false,
            busy: false,
        });

        // Connect output handler
        let dnd_toggle_ref = self.dnd_toggle.clone();
        let store_for_dnd = self.store.clone();
        dnd_toggle.connect_output(move |event| {
            debug!("[dnd] Received: {:?}", event);
            match event {
                DoNotDisturbToggleOutput::Activate => {
                    store_for_dnd.emit(NotificationOp::SetDnd(true));
                    if let Some(ref toggle) = *dnd_toggle_ref.borrow() {
                        toggle.set_active(true);
                    }
                }
                DoNotDisturbToggleOutput::Deactivate => {
                    store_for_dnd.emit(NotificationOp::SetDnd(false));
                    if let Some(ref toggle) = *dnd_toggle_ref.borrow() {
                        toggle.set_active(false);
                    }
                }
            }
        });

        *self.dnd_toggle.borrow_mut() = Some(dnd_toggle);

        // Create NotificationsWidget for the overlay Info slot
        let notifications_widget = NotificationsWidget::new(self.store.clone());

        // Connect output handler for widget events
        let db_widget = self.debouncer.as_ref().unwrap().clone();
        let outbound_tx_widget = self.client_channel.clone();
        notifications_widget.connect_output(move |event| {
            debug!("[notifications_widget] Received: {:?}", event);
            match event {
                NotificationsWidgetOutput::ActionClick(id, action_key) => {
                    let _ = db_widget.send(NotificationOp::NotificationDismiss(id));
                    let _ = outbound_tx_widget.send(OutboundEvent::ActionInvoked {
                        id: id as u32,
                        action_key: action_key.clone(),
                    });
                    let _ = outbound_tx_widget.send(OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: close_reasons::DISMISSED_BY_USER,
                    });
                }
                NotificationsWidgetOutput::Dismiss(id) => {
                    let _ = outbound_tx_widget.send(OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: close_reasons::DISMISSED_BY_USER,
                    });
                    let _ = db_widget.send(NotificationOp::NotificationDismiss(id));
                }
            }
        });

        *self.notifications_widget.borrow_mut() = Some(notifications_widget);

        self.schedule_tick();

        Ok(())
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        match *self.dnd_toggle.borrow() {
            Some(ref dnd_toggle) => vec![Arc::new(WidgetFeatureToggle {
                el: dnd_toggle.widget().clone().upcast::<gtk::Widget>(),
                weight: 60,
                menu: None,
                on_expand_toggled: None,
            })],
            None => vec![],
        }
    }

    fn get_widgets(&self) -> Vec<Arc<Widget>> {
        match *self.notifications_widget.borrow() {
            Some(ref notifications_widget) => vec![Arc::new(Widget {
                slot: Slot::Info,
                weight: 10,
                el: notifications_widget.widget().clone().upcast::<gtk::Widget>(),
            })],
            None => vec![],
        }
    }
}
