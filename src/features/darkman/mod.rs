//! Darkman plugin - dark mode toggle.

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use crate::dbus::DbusHandle;
use crate::plugin::{Plugin, PluginId, WidgetFeatureToggle};
use crate::ui::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

use self::dbus::DARKMAN_DESTINATION;
use self::dbus::{get_state, set_state};
use self::values::DarkmanMode;

mod dbus;
mod values;

pub struct DarkmanPlugin {
    mode: DarkmanMode,
    dbus: Arc<DbusHandle>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
    mode_channel: (flume::Sender<DarkmanMode>, flume::Receiver<DarkmanMode>),
}

impl DarkmanPlugin {
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        Self {
            dbus,
            mode: DarkmanMode::Light,
            toggle: Rc::new(RefCell::new(None)),
            mode_channel: flume::unbounded(),
        }
    }

    async fn start_monitoring(&self) -> Result<()> {
        let mode_tx = self.mode_channel.0.clone();
        let handle_value = move |value: Option<String>| {
            if let Some(value) = value {
                let mode = DarkmanMode::from_str(&value).unwrap_or(DarkmanMode::Light);
                let _ = mode_tx.send(mode);
                info!("[darkman/dbus] Mode changed to: {:?}", mode);
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
        let toggle = FeatureToggleWidget::new(FeatureToggleProps {
            title: "Dark Mode".into(),
            icon: "weather-clear-night-symbolic".into(),
            details: None,
            active: DarkmanMode::is_active(self.mode),
            busy: false,
        });

        // Connect output handler
        let dbus = self.dbus.clone();
        let toggle_ref = self.toggle.clone();
        toggle.connect_output(move |event| {
            debug!("[darkman/ui] Received: {:?}", event);
            let dbus = dbus.clone();
            let toggle_ref = toggle_ref.clone();

            glib::spawn_future_local(async move {
                // Set busy state
                if let Some(ref toggle) = *toggle_ref.borrow() {
                    toggle.set_busy(true);
                }

                let result = match event {
                    FeatureToggleOutput::Activate => {
                        set_state(dbus, DarkmanMode::Dark).await
                    }
                    FeatureToggleOutput::Deactivate => {
                        set_state(dbus, DarkmanMode::Light).await
                    }
                };

                if let Err(err) = result {
                    error!("Failed to set darkman state: {}", err);
                    // Reset busy state on error
                    if let Some(ref toggle) = *toggle_ref.borrow() {
                        toggle.set_busy(false);
                    }
                }
            });
        });

        *self.toggle.borrow_mut() = Some(toggle);

        // Handle mode changes from DBus monitoring
        let toggle_ref = self.toggle.clone();
        let mode_rx = self.mode_channel.1.clone();
        glib::spawn_future_local(async move {
            while let Ok(mode) = mode_rx.recv_async().await {
                let active = DarkmanMode::is_active(mode);
                if let Some(ref toggle) = *toggle_ref.borrow() {
                    toggle.set_active(active);
                    toggle.set_busy(false);
                }
            }
        });

        Ok(())
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        match *self.toggle.borrow() {
            Some(ref toggle) => {
                vec![Arc::new(WidgetFeatureToggle {
                    el: toggle.root.clone().upcast::<gtk::Widget>(),
                    weight: 190,
                })]
            }
            None => vec![],
        }
    }
}
