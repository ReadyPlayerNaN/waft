use anyhow::Result;
use async_trait::async_trait;
use log::debug;
use std::sync::Arc;

use gtk::prelude::Cast;
use relm4::prelude::*;
use relm4::{Component, ComponentController};

mod feature_toggle;
mod ipc;
mod values;

use crate::channels::{Channel, connect_component};
use crate::plugin::{Plugin, PluginId, WidgetFeatureToggle};

use self::feature_toggle::FeatureToggle;
use self::feature_toggle::Init as FeatureToggleInit;
use self::feature_toggle::Input as FeatureToggleInput;
use self::feature_toggle::Output as FeatureToggleEvents;
use self::ipc::SunsetrIpcEvents;
use self::ipc::{spawn_following, spawn_start, spawn_stop};

pub struct SunsetrPlugin {
    active: bool,
    initialized: bool,
    next_transition: Option<String>,
    toggle: Option<Controller<FeatureToggle>>,
    ui_channel: Channel<FeatureToggleEvents>,
    ipc_channel: Channel<SunsetrIpcEvents>,
}

impl SunsetrPlugin {
    pub fn new() -> Self {
        Self {
            initialized: false,
            active: false,
            next_transition: None,
            ui_channel: Channel::new(),
            ipc_channel: Channel::new(),
            toggle: None,
        }
    }

    pub fn create_feature_toggle(&self) -> Controller<FeatureToggle> {
        connect_component(
            FeatureToggle::builder().launch(FeatureToggleInit {
                active: self.active,
                busy: false,
                next_transition: self.next_transition.clone(),
            }),
            &self.ui_channel,
        )
    }
}

#[async_trait(?Send)]
impl Plugin for SunsetrPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::sunsetr")
    }

    async fn init(&mut self) -> Result<()> {
        self.initialized = true;
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        let cx = self.create_feature_toggle();
        let rx = self.ipc_channel.receiver.clone();
        let cx_sender = cx.sender().clone();
        let ui_receiver = self.ui_channel.receiver.clone();
        self.toggle = Some(cx);

        relm4::tokio::spawn(async move {
            while let Ok(event) = rx.recv_async().await {
                debug!("[sunsetr/ipc] Received event: {:?}", event);
                match event {
                    SunsetrIpcEvents::Status(status) => {
                        cx_sender.emit(FeatureToggleInput::Status(status));
                    }
                    SunsetrIpcEvents::Busy(busy) => {
                        cx_sender.emit(FeatureToggleInput::Busy(busy));
                    }
                    SunsetrIpcEvents::Error(error) => {
                        cx_sender.emit(FeatureToggleInput::Error(error));
                    }
                }
            }
        });

        let ipc_sender = self.ipc_channel.sender.clone();
        relm4::tokio::spawn(async move {
            while let Ok(event) = ui_receiver.recv_async().await {
                debug!("[sunsetr/ui] Received event: {:?}", event);
                let _ = match event {
                    FeatureToggleEvents::Deactivate => spawn_stop(ipc_sender.clone()).await,
                    FeatureToggleEvents::Activate => spawn_start(ipc_sender.clone()).await,
                };
            }
        });
        spawn_following(self.ipc_channel.sender.clone())?;
        Ok(())
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        match &self.toggle {
            Some(toggle) => {
                let widget = toggle.widget().clone().upcast::<gtk::Widget>();
                vec![Arc::new(WidgetFeatureToggle {
                    el: widget,
                    weight: 200,
                })]
            }
            None => vec![],
        }
    }
}
