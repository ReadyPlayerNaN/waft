//! Sunsetr plugin - night light toggle.

use anyhow::Result;
use async_trait::async_trait;
use flume::unbounded;
use log::debug;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use crate::plugin::{Plugin, PluginId, WidgetFeatureToggle};
use crate::ui::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

mod ipc;
mod values;

use self::ipc::SunsetrIpcEvents;
use self::ipc::{spawn_following, spawn_start, spawn_stop};

pub struct SunsetrPlugin {
    active: bool,
    next_transition: Option<String>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
}

impl SunsetrPlugin {
    pub fn new() -> Self {
        Self {
            active: false,
            next_transition: None,
            toggle: Rc::new(RefCell::new(None)),
        }
    }
}

#[async_trait(?Send)]
impl Plugin for SunsetrPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::sunsetr")
    }

    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        let toggle = FeatureToggleWidget::new(FeatureToggleProps {
            title: "Night light".into(),
            icon: "night-light-symbolic".into(),
            details: self.next_transition.clone(),
            active: self.active,
            busy: false,
        });

        // Create IPC channel
        let (ipc_tx, ipc_rx) = unbounded::<SunsetrIpcEvents>();

        // Connect output handler
        let ipc_sender = ipc_tx.clone();
        toggle.connect_output(move |event| {
            debug!("[sunsetr/ui] Received: {:?}", event);
            let ipc_sender = ipc_sender.clone();

            glib::spawn_future_local(async move {
                let _ = match event {
                    FeatureToggleOutput::Activate => spawn_start(ipc_sender).await,
                    FeatureToggleOutput::Deactivate => spawn_stop(ipc_sender).await,
                };
            });
        });

        *self.toggle.borrow_mut() = Some(toggle);

        // Handle IPC events
        let toggle_ref = self.toggle.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = ipc_rx.recv_async().await {
                debug!("[sunsetr/ipc] Received event: {:?}", event);
                if let Some(ref toggle) = *toggle_ref.borrow() {
                    match event {
                        SunsetrIpcEvents::Status(status) => {
                            toggle.set_active(status.active);
                            toggle.set_details(
                                status.next_transition_text.map(|text| format!("Until: {}", text)),
                            );
                            toggle.set_busy(false);
                        }
                        SunsetrIpcEvents::Busy(busy) => {
                            toggle.set_busy(busy);
                        }
                        SunsetrIpcEvents::Error(_error) => {
                            // Errors are logged by the IPC module
                        }
                    }
                }
            }
        });

        // Start following sunsetr events
        spawn_following(ipc_tx)?;

        Ok(())
    }

    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>> {
        match *self.toggle.borrow() {
            Some(ref toggle) => {
                vec![Arc::new(WidgetFeatureToggle {
                    el: toggle.root.clone().upcast::<gtk::Widget>(),
                    weight: 200,
                })]
            }
            None => vec![],
        }
    }
}
