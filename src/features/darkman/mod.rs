use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info};
use std::sync::Arc;

use gtk::prelude::Cast;
use relm4::prelude::*;
use relm4::{Component, ComponentController};

use crate::channels::{Channel, connect_component};
use crate::dbus::DbusHandle;
use crate::plugin::{Plugin, PluginId, WidgetFeatureToggle};

use self::dbus::DARKMAN_DESTINATION;
use self::dbus::{get_state, set_state};
use self::feature_toggle::{
    DarkmanToggle, DarkmanToggleInit, DarkmanToggleInput, DarkmanToggleOutput,
};
use self::values::DarkmanMode;

mod dbus;
mod feature_toggle;
mod values;

#[derive(Debug, Clone)]
enum DarkmanDbusOutput {
    State(DarkmanMode),
}

pub struct DarkmanPlugin {
    mode: DarkmanMode,
    busy: bool,
    dbus: Arc<DbusHandle>,
    dbus_channel: Channel<DarkmanDbusOutput>,
    ui_channel: Channel<DarkmanToggleOutput>,
    toggle: Option<Controller<DarkmanToggle>>,
}

impl DarkmanPlugin {
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        Self {
            busy: false,
            ui_channel: Channel::new(),
            dbus: dbus,
            dbus_channel: Channel::new(),
            mode: DarkmanMode::Light,
            toggle: None,
        }
    }

    fn create_feature_toggle(&self, init: DarkmanToggleInit) -> Controller<DarkmanToggle> {
        connect_component(DarkmanToggle::builder().launch(init), &self.ui_channel)
    }

    async fn start_monitoring(&mut self) -> Result<()> {
        let sender = self.dbus_channel.sender.clone();
        let handle_value = move |value: Option<String>| {
            if let Some(value) = value {
                match sender.send(DarkmanDbusOutput::State(
                    DarkmanMode::from_str(&value).unwrap_or(DarkmanMode::Light),
                )) {
                    Ok(_) => {}
                    Err(err) => error!("Failed to send state update: {}", err),
                };
            }
        };
        self.dbus
            .listen_for_values(DARKMAN_DESTINATION, "ModeChanged", handle_value)
            .await?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl Plugin for DarkmanPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::darkman")
    }

    async fn init(&mut self) -> Result<()> {
        self.mode = get_state(&self.dbus).await?;
        self.start_monitoring().await?;
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        let cx = self.create_feature_toggle(DarkmanToggleInit {
            active: DarkmanMode::is_active(self.mode),
            busy: self.busy,
        });
        let cx_sender1 = cx.sender().clone();
        let cx_sender2 = cx.sender().clone();
        let dbus_receiver = self.dbus_channel.receiver.clone();
        let ui_receiver = self.ui_channel.receiver.clone();
        let dbus = self.dbus.clone();
        self.toggle = Some(cx);

        tokio::spawn(async move {
            while let Ok(event) = ui_receiver.recv_async().await {
                debug!("[darkman/ui] Received: {:?}", event);
                let r = match event {
                    DarkmanToggleOutput::Activate => {
                        cx_sender1.emit(DarkmanToggleInput::Busy(true));
                        set_state(dbus.clone(), DarkmanMode::Dark).await
                    }
                    DarkmanToggleOutput::Deactivate => {
                        cx_sender1.emit(DarkmanToggleInput::Busy(true));
                        set_state(dbus.clone(), DarkmanMode::Light).await
                    }
                };
                match r {
                    Ok(_) => {}
                    Err(err) => error!("Failed to send state update: {}", err),
                };
            }
        });
        tokio::spawn(async move {
            while let Ok(event) = dbus_receiver.recv_async().await {
                info!("[darkman/dbus] Received: {:?}", event);
                match event {
                    DarkmanDbusOutput::State(state) => {
                        cx_sender2.emit(DarkmanToggleInput::Active(DarkmanMode::is_active(state)));
                        cx_sender2.emit(DarkmanToggleInput::Busy(false));
                    }
                }
            }
        });
        Ok(())
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        match &self.toggle {
            Some(w) => {
                let widget = w.widget().clone().upcast::<gtk::Widget>();
                vec![Arc::new(WidgetFeatureToggle {
                    el: widget,
                    weight: 190,
                })]
            }
            None => vec![],
        }
    }
}
