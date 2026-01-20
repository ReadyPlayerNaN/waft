use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, info};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glib::object::Cast;
use relm4::gtk::prelude::GtkApplicationExt;
use relm4::prelude::*;

use crate::channels::{Channel, connect_component};
use crate::features::notifications::store::NotificationOp;
use crate::plugin::WidgetFeatureToggle;
use crate::plugin::{Plugin, PluginId, Slot, Widget};

use self::dbus::client::{IngressEvent, OutboundEvent, close_reasons};
use self::dbus::server::NotificationsDbusServer;
use self::dnd_toggle::{
    DoNotDisturbToggle, DoNotDisturbToggleInit, DoNotDisturbToggleInput, DoNotDisturbToggleOutput,
};
use self::store::REDUCER;
use self::ui::toast_window::{HPos, ToastWindow, ToastWindowInit, ToastWindowOutput, VPos};
use self::ui::widget::{NotificationsWidget, NotificationsWidgetInit, NotificationsWidgetOutput};

mod dbus;
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
    toast_channel: Channel<ToastWindowOutput>,
    toast: Option<Controller<ToastWindow>>,
    widget_channel: Channel<NotificationsWidgetOutput>,
    widget: Option<Controller<NotificationsWidget>>,
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        Self {
            client_channel: Channel::new(),
            dbus_server: None,
            dnd: Arc::new(AtomicBool::new(false)),
            dnd_toggle: None,
            dnd_toggle_channel: Channel::new(),
            initialized: false,
            server_channel: Channel::new(),
            toast_channel: Channel::new(),
            toast: None,
            widget_channel: Channel::new(),
            widget: None,
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
        // NOTE: Start empty; the plugin is the source of truth and will ingest/remove via inputs.
        // This also ensures the toast window begins hidden (per requirement "hidden when empty").
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
        // Start the server now; it will attempt to replace any existing owner of the name.
        // If it cannot acquire the name, this returns an error and we exit.
        dbus_server
            .start(
                self.server_channel.sender.clone(),
                self.client_channel.receiver.clone(),
            )
            .await
            .context("Failed to start DBus notifications server")?;

        // Store the dbus_server to prevent it from being dropped
        self.dbus_server = Some(dbus_server);

        self.initialized = true;
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        // Create the main notifications widget (overlay column content).
        let widget = self.create_widget();
        self.widget = Some(widget);

        // Create the toast window (separate layer-shell surface above all other windows).
        //
        // Position defaults: top-right (no margins; compositor edge aligned).
        // NOTE: The toast window is focusable (keyboard mode on-demand) for now.
        // We may want to change that later to avoid focus-stealing.
        let toast = self.create_toast_window(HPos::Center, VPos::Top);
        let toast_window = toast.widget().clone();
        relm4::main_application().add_window(&toast_window);
        self.toast = Some(toast);

        // DBus ingress -> plugin -> UI reconciliation
        let server_receiver = self.server_channel.receiver.clone();
        let outbound_tx = self.client_channel.sender.clone();
        relm4::tokio::spawn(async move {
            while let Ok(event) = server_receiver.recv_async().await {
                info!("[notifications] Received: {:?}", event);
                match event {
                    IngressEvent::Notify { notification } => {
                        REDUCER.emit(NotificationOp::Ingress(notification));
                    }

                    IngressEvent::CloseNotification { id } => {
                        REDUCER.emit(NotificationOp::NotificationRetract(id as u64));

                        // Then emit DBus NotificationClosed(reason=CLOSED_BY_CALL).
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id,
                            reason: close_reasons::CLOSED_BY_CALL,
                        });
                    }
                }
            }
        });

        // Toast window outputs -> plugin -> DBus outbound + plugin-driven reconciliation
        let toast_receiver = self.toast_channel.receiver.clone();
        let outbound_tx = self.client_channel.sender.clone();
        relm4::tokio::spawn(async move {
            while let Ok(event) = toast_receiver.recv_async().await {
                debug!("[toast] Received: {:?}", event);
                match event {
                    ToastWindowOutput::ActionClick(id, action_key) => {
                        REDUCER.emit(NotificationOp::NotificationDismiss(id));
                        // Emit ActionInvoked first, then close and remove (per policy).
                        let _ = outbound_tx.send(OutboundEvent::ActionInvoked {
                            id: id as u32,
                            action_key: action_key.clone(),
                        });
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::DISMISSED_BY_USER,
                        });
                    }

                    // ToastWindowOutput::CardClose(id) => {
                    //     let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                    //         id: id as u32,
                    //         reason: close_reasons::DISMISSED_BY_USER,
                    //     });
                    // }
                    ToastWindowOutput::TimedOut(id) => {
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::EXPIRED,
                        });
                        // REDUCER.emit(NotificationOp::NotificationDismiss(id));
                        // widget_sender_for_toast.emit(NotificationsWidgetInput::Remove(id));
                        // toast_sender_for_toast.emit(ToastWindowInput::Remove(id));
                    }

                    ToastWindowOutput::CardClick(_id) => {
                        // No-op for now (could map to "default action" later).
                    }
                }
            }
        });

        // NotificationsWidget outputs -> plugin -> DBus outbound + plugin-driven reconciliation
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
                        REDUCER.emit(NotificationOp::NotificationDismiss(id));
                    }

                    NotificationsWidgetOutput::CardClose(id) => {
                        let _ = outbound_tx.send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::DISMISSED_BY_USER,
                        });
                        REDUCER.emit(NotificationOp::NotificationDismiss(id));
                    }

                    NotificationsWidgetOutput::CardClick(_id) => {
                        // No-op for now (could map to "default action" later).
                    }
                }
            }
        });

        // DND toggle wiring (unchanged behavior).
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
