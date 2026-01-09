use anyhow::Result;
use async_trait::async_trait;
use gtk::glib::DateTime;
use log::{error, info};
use relm4::prelude::*;
mod widget;

use crate::relm4_app::channels::{Channel, connect_component};
use crate::relm4_app::plugin::{Plugin, PluginId, Slot, Widget};
use std::sync::Arc;
use std::time::Duration;

use gtk::prelude::Cast;
use relm4::{Component, ComponentController};

use self::widget::{ClockInit, ClockInput, ClockOutput, ClockWidget};

pub struct ClockPlugin {
    channel: Channel<ClockOutput>,
    widget: Option<Controller<ClockWidget>>,
}

impl ClockPlugin {
    pub fn new() -> Self {
        Self {
            channel: Channel::new(),
            widget: None,
        }
    }

    fn create_widget(&self, datetime: DateTime) -> Controller<ClockWidget> {
        connect_component(
            ClockWidget::builder().launch(ClockInit { datetime: datetime }),
            &self.channel,
        )
    }
}

#[async_trait(?Send)]
impl Plugin for ClockPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::clock")
    }

    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        let datetime = DateTime::now_local()?;
        let cx = self.create_widget(datetime);
        let cx_sender = cx.sender().clone();
        let ui_receiver = self.channel.receiver.clone();
        self.widget = Some(cx);

        tokio::spawn(async move {
            while let Ok(event) = ui_receiver.recv_async().await {
                info!("[clock] Received event: {:?}", event);
            }
        });
        tokio::spawn(async move {
            loop {
                match DateTime::now_local() {
                    Ok(datetime) => {
                        cx_sender.emit(ClockInput::Tick(datetime));
                    }
                    Err(err) => {
                        error!("[clock] Failed to get current datetime: {:?}", err);
                    }
                };
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        Ok(())
    }

    fn get_widgets(&self) -> Vec<Arc<Widget>> {
        match &self.widget {
            Some(w) => {
                let widget = w.widget().clone().upcast::<gtk::Widget>();
                vec![Arc::new(Widget {
                    slot: Slot::Header,
                    el: widget,
                    weight: 10,
                })]
            }
            None => vec![],
        }
    }
}
