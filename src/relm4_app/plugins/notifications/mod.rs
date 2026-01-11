use anyhow::{Context, Result};
use async_trait::async_trait;
use glib::object::Cast;
use log::info;
use relm4::prelude::*;
use std::sync::Arc;
use std::time::SystemTime;

use crate::relm4_app::channels::{Channel, connect_component};
use crate::relm4_app::plugin::{Plugin, PluginId, Slot, Widget};
use crate::relm4_app::plugins::notifications::types::AppIdent;
use crate::relm4_app::plugins::notifications::widget::NotificationsWidgetInput;

use self::dbus::client::{IngressEvent, OutboundEvent};
use self::dbus::server::NotificationsDbusServer;
use self::types::{NotificationDisplay, NotificationIcon, NotificationUrgency};
use self::widget::{NotificationsWidget, NotificationsWidgetInit, NotificationsWidgetOutput};

mod dbus;
mod gate;
mod types;
mod ui;
mod widget;

pub struct NotificationsPlugin {
    client_channel: Channel<OutboundEvent>,
    dbus_server: Option<NotificationsDbusServer>,
    initialized: bool,
    server_channel: Channel<IngressEvent>,
    ui_channel: Channel<NotificationsWidgetOutput>,
    widget: Option<Controller<NotificationsWidget>>,
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        Self {
            ui_channel: Channel::new(),
            server_channel: Channel::new(),
            client_channel: Channel::new(),
            initialized: false,
            dbus_server: None,
            widget: None,
        }
    }

    fn create_widget(&self) -> Controller<NotificationsWidget> {
        connect_component(
            NotificationsWidget::builder().launch(NotificationsWidgetInit {
                notifications: Some(vec![
                    Arc::new(NotificationDisplay {
                        app: None,
                        actions: vec![],
                        id: 1,
                        replaces_id: None,
                        description: "Lorem ipsum dolor".into(),
                        created_at: SystemTime::now(),
                        icon: NotificationIcon::Themed("dialog-information-symbolic".into()),
                        ttl: Some(0),
                        title: "Notification 1".into(),
                        urgency: NotificationUrgency::Normal,
                    }),
                    Arc::new(NotificationDisplay {
                        app: Some(AppIdent {
                            title: Some("MyApp".into()),
                            ident: "myapp".into(),
                            icon: None,
                        }),
                        actions: vec![],
                        id: 2,
                        replaces_id: None,
                        description: "Lorem ipsum dolor".into(),
                        created_at: SystemTime::now(),
                        icon: NotificationIcon::Themed("dialog-information-symbolic".into()),
                        ttl: Some(0),
                        title: "Notification 2".into(),
                        urgency: NotificationUrgency::Normal,
                    }),
                ]),
            }),
            &self.ui_channel,
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
        let widget = self.create_widget();

        let widget_sender = widget.sender().clone();
        self.widget = Some(widget);

        let server_receiver = self.server_channel.receiver.clone();
        tokio::spawn(async move {
            while let Ok(event) = server_receiver.recv_async().await {
                info!("[notifications] Received: {:?}", event);
                match event {
                    IngressEvent::Notify { notification } => {
                        widget_sender
                            .emit(NotificationsWidgetInput::Ingest(Arc::new(notification)));
                    }
                    _ => {}
                }
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
}
