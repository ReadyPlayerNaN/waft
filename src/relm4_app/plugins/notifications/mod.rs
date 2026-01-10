use anyhow::Result;
use async_trait::async_trait;
use glib::object::Cast;
use relm4::prelude::*;
use std::sync::Arc;
use std::time::SystemTime;

use crate::relm4_app::channels::{Channel, connect_component};
use crate::relm4_app::plugin::{Plugin, PluginId, Slot, Widget};
use crate::relm4_app::plugins::notifications::types::{
    NotificationDisplay, NotificationIcon, NotificationUrgency,
};
use crate::relm4_app::plugins::notifications::widget::NotificationsWidgetOutput;

use self::widget::{NotificationsWidget, NotificationsWidgetInit};

mod gate;
mod types;
mod ui;
mod widget;

pub struct NotificationsPlugin {
    channel: Channel<NotificationsWidgetOutput>,
    initialized: bool,
    widget: Option<Controller<NotificationsWidget>>,
}

impl NotificationsPlugin {
    pub fn new() -> Self {
        Self {
            channel: Channel::new(),
            initialized: false,
            widget: None,
        }
    }

    fn create_widget(&self) -> Controller<NotificationsWidget> {
        connect_component(
            NotificationsWidget::builder().launch(NotificationsWidgetInit {
                notifications: Some(vec![NotificationDisplay {
                    app: None,
                    id: 1,
                    description: "Lorem ipsum dolor".into(),
                    created_at: SystemTime::now(),
                    icon: NotificationIcon::Themed("dialog-information-symbolic".into()),
                    progress: 0.0,
                    title: "Notification 1".into(),
                    urgency: NotificationUrgency::Normal,
                }]),
            }),
            &self.channel,
        )
    }
}

#[async_trait(?Send)]
impl Plugin for NotificationsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::notifications")
    }

    async fn init(&mut self) -> Result<()> {
        self.initialized = true;
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        self.widget = Some(self.create_widget());
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
