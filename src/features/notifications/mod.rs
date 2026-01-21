use anyhow::{Context, Result};
use async_trait::async_trait;
use glib::object::Cast;
use indexmap::indexmap;
use log::{debug, info};
use relm4::gtk::prelude::GtkApplicationExt;
use relm4::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::channels::{Channel, connect_component};
use crate::features::notifications::store::NotificationOp;
use crate::plugin::WidgetFeatureToggle;
use crate::plugin::{Plugin, PluginId, Slot, Widget};

use self::dbus::client::{IngressEvent, OutboundEvent, close_reasons};
use self::dbus::server::NotificationsDbusServer;
use self::debounce::NotificationDebouncer;
use self::dnd_toggle::{
    DoNotDisturbToggle, DoNotDisturbToggleInit, DoNotDisturbToggleInput, DoNotDisturbToggleOutput,
};
use self::store::REDUCER;
use self::ui::toast_window::{HPos, ToastWindow, ToastWindowInit, ToastWindowOutput, VPos};
use self::ui::widget::{NotificationsWidget, NotificationsWidgetInit, NotificationsWidgetOutput};

mod dbus;
mod debounce;
mod dnd_toggle;
mod gate;
mod store;
mod types;
mod ui;

pub struct NotificationsPlugin {
    client_channel: Channel<OutboundEvent>,
    dbus_server: Option<NotificationsDbusServer>,
    dnd: Arc<AtomicBool>,
    dnd_toggle: Option<Controller<DoNotDisturbToggle>>,
    dnd_toggle_channel: Channel<DoNotDisturbToggleOutput>,
    initialized: bool,
    server_channel: Channel<IngressEvent>,
    tick_source: Arc<std::sync::Mutex<Option<glib::SourceId>>>,
    tick_channel: tokio::sync::mpsc::UnboundedSender<()>,
    toast_channel: Channel<ToastWindowOutput>,
    toast: Option<Controller<ToastWindow>>,
    widget_channel: Channel<NotificationsWidgetOutput>,
    widget: Option<Controller<NotificationsWidget>>,
    debouncer: Option<NotificationDebouncer>,
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        let (tick_tx, _) = tokio::sync::mpsc::unbounded_channel();
        Self {
            client_channel: Channel::new(),
            dbus_server: None,
            dnd: Arc::new(AtomicBool::new(false)),
            dnd_toggle: None,
            dnd_toggle_channel: Channel::new(),
            initialized: false,
            server_channel: Channel::new(),
            tick_source: Arc::new(std::sync::Mutex::new(None)),
            tick_channel: tick_tx,
            toast_channel: Channel::new(),
            toast: None,
            widget_channel: Channel::new(),
            widget: None,
            debouncer: None,
        }
    }

    fn create_widget(&self) -> Controller<NotificationsWidget> {
        connect_component(
            NotificationsWidget::builder().launch(NotificationsWidgetInit {
                expanded_group: None,
            }),
            &self.widget_channel,
        )
    }

    fn create_toast_window(&self, hpos: HPos, vpos: VPos) -> Controller<ToastWindow> {
        connect_component(
            ToastWindow::builder().launch(ToastWindowInit { hpos, vpos }),
            &self.toast_channel,
        )
    }

    fn create_dnd_toggle(
        &mut self,
        init: DoNotDisturbToggleInit,
    ) -> Controller<DoNotDisturbToggle> {
        connect_component(
            DoNotDisturbToggle::builder().launch(init),
            &self.dnd_toggle_channel,
        )
    }

    fn schedule_tick(&self) {
        let tick_source = self.tick_source.clone();
        let tick_source_for_closure = self.tick_source.clone();
        *tick_source.lock().unwrap() = Some(glib::timeout_add_local_once(
            Duration::from_millis(200),
            move || {
                REDUCER.emit(NotificationOp::Tick);

                // Schedule the next tick
                *tick_source_for_closure.lock().unwrap() = Some(glib::timeout_add_local(
                    Duration::from_millis(200),
                    move || {
                        REDUCER.emit(NotificationOp::Tick);
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

    async fn init(&mut self) -> Result<()> {
        let mut dbus_server = NotificationsDbusServer::connect()
            .await
            .context("Failed to connect DBus notifications server")?;

        info!("Starting notifications dbus server");
        dbus_server
            .start(
                self.server_channel.sender.clone(),
                self.client_channel.receiver.clone(),
            )
            .await
            .context("Failed to start DBus notifications server")?;

        self.dbus_server = Some(dbus_server);

        self.initialized = true;
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        let (debouncer_tx, debouncer_rx) = flume::unbounded();

        relm4::tokio::spawn(async move {
            while let Ok(op) = debouncer_rx.recv_async().await {
                REDUCER.emit(op);
            }
        });

        let debouncer = NotificationDebouncer::new(debouncer_tx);
        let db_server = debouncer.clone();
        let db_toast = debouncer.clone();
        let db_widget = debouncer.clone();

        self.debouncer = Some(debouncer);

        let widget = self.create_widget();
        self.widget = Some(widget);

        let toast = self.create_toast_window(HPos::Center, VPos::Top);
        let toast_window = toast.widget().clone();
        relm4::main_application().add_window(&toast_window);
        self.toast = Some(toast);

        let server_receiver = self.server_channel.receiver.clone();
        let outbound_tx = self.client_channel.sender.clone();
        relm4::tokio::spawn(async move {
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

        let toast_receiver = self.toast_channel.receiver.clone();
        let outbound_tx = self.client_channel.sender.clone();
        relm4::tokio::spawn(async move {
            while let Ok(event) = toast_receiver.recv_async().await {
                debug!("[toast] Received: {:?}", event);
                match event {
                    ToastWindowOutput::ActionClick(id, action_key) => {
                        let _ = db_toast.send(NotificationOp::NotificationDismiss(id));
                        let _ = outbound_tx.send(OutboundEvent::ActionInvoked {
                            id: id as u32,
                            action_key: action_key.clone(),
                        });
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::DISMISSED_BY_USER,
                        });
                    }

                    ToastWindowOutput::TimedOut(id) => {
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::EXPIRED,
                        });
                    }

                    ToastWindowOutput::CardClick(_id) => {}
                }
            }
        });

        let widget_receiver = self.widget_channel.receiver.clone();
        let outbound_tx = self.client_channel.sender.clone();
        relm4::tokio::spawn(async move {
            while let Ok(event) = widget_receiver.recv_async().await {
                debug!("[notifications-widget] Received: {:?}", event);
                match event {
                    NotificationsWidgetOutput::ActionClick(id, action_key) => {
                        let _ = outbound_tx.send(OutboundEvent::ActionInvoked {
                            id: id as u32,
                            action_key: action_key.clone(),
                        });
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::DISMISSED_BY_USER,
                        });
                        let _ = db_widget.send(NotificationOp::NotificationDismiss(id));
                    }

                    NotificationsWidgetOutput::CardClose(id) => {
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::DISMISSED_BY_USER,
                        });
                        let _ = db_widget.send(NotificationOp::NotificationDismiss(id));
                    }

                    NotificationsWidgetOutput::CardClick(_id) => {}
                }
            }
        });

        let active = self.dnd.load(Ordering::SeqCst);
        let dnd_toggle = self.create_dnd_toggle(DoNotDisturbToggleInit {
            active,
            busy: false,
        });
        let dnd_toggle_receiver = self.dnd_toggle_channel.receiver.clone();
        let dnd_toggle_sender = dnd_toggle.sender().clone();
        let dnd_state = self.dnd.clone();
        self.dnd_toggle = Some(dnd_toggle);
        relm4::tokio::spawn(async move {
            while let Ok(event) = dnd_toggle_receiver.recv_async().await {
                debug!("[dnd] Received: {:?}", event);
                match event {
                    DoNotDisturbToggleOutput::Activate => {
                        dnd_toggle_sender.emit(DoNotDisturbToggleInput::Active(true));
                        dnd_state.store(true, Ordering::SeqCst);
                    }
                    DoNotDisturbToggleOutput::Deactivate => {
                        dnd_toggle_sender.emit(DoNotDisturbToggleInput::Active(false));
                        dnd_state.store(false, Ordering::SeqCst);
                    }
                };
            }
        });

        self.schedule_tick();

        Ok(())
    }

    fn get_widgets(&self) -> Vec<Arc<Widget>> {
        match self.widget {
            Some(ref widget) => vec![Arc::new(Widget {
                el: widget.widget().clone().upcast::<gtk::Widget>(),
                slot: Slot::Info,
                weight: 100,
            })],
            _ => vec![],
        }
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        match self.dnd_toggle {
            Some(ref dnd_toggle) => vec![Arc::new(WidgetFeatureToggle {
                el: dnd_toggle.widget().clone().upcast::<gtk::Widget>(),
                weight: 60,
            })],
            None => vec![],
        }
    }
}
