//! Sunsetr plugin - night light toggle.
use crate::menu_state::MenuStore;

use anyhow::Result;
use async_trait::async_trait;
use flume::unbounded;
use log::{debug, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use crate::plugin::{Plugin, PluginId, WidgetFeatureToggle, WidgetRegistrar};
use crate::ui::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

mod ipc;
pub mod store;
mod values;

use self::ipc::SunsetrIpcEvents;
use self::ipc::{spawn_following, spawn_start, spawn_stop};
use self::store::{SunsetrOp, SunsetrStore, create_sunsetr_store};

pub struct SunsetrPlugin {
    store: Rc<SunsetrStore>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
}

impl SunsetrPlugin {
    pub fn new() -> Self {
        Self {
            store: Rc::new(create_sunsetr_store()),
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

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Arc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let initial_state = {
            let state = self.store.get_state();
            (state.active, state.next_transition.clone())
        };

        let toggle = FeatureToggleWidget::new(FeatureToggleProps {
            title: crate::i18n::t("nightlight-title").into(),
            icon: "night-light-symbolic".into(),
            details: initial_state.1.clone(),
            active: initial_state.0,
            busy: false,
        });

        // Create IPC channel
        let (ipc_tx, ipc_rx) = unbounded::<SunsetrIpcEvents>();

        // Connect output handler
        let ipc_sender = ipc_tx.clone();
        toggle.connect_output(move |event| {
            debug!("[sunsetr/ui] Received: {:?}", event);
            let ipc_sender = ipc_sender.clone();

            // Spawn tokio work on tokio runtime, NOT in glib context
            // This prevents busy-polling (see AGENTS.md: Runtime Mixing)
            tokio::spawn(async move {
                let result = match event {
                    FeatureToggleOutput::Activate => spawn_start(ipc_sender).await,
                    FeatureToggleOutput::Deactivate => spawn_stop(ipc_sender).await,
                };
                if let Err(e) = result {
                    warn!("[sunsetr] toggle action failed: {e}");
                }
            });
        });

        // Register the feature toggle
        registrar.register_feature_toggle(Arc::new(WidgetFeatureToggle {
            id: "sunsetr:toggle".to_string(),
            el: toggle.root.clone().upcast::<gtk::Widget>(),
            weight: 200,
            menu: None,
            menu_id: None,
            on_expand_toggled: None,
        }));

        *self.toggle.borrow_mut() = Some(toggle);

        // Subscribe to store for state changes
        let toggle_ref = self.toggle.clone();
        let store = self.store.clone();
        self.store.subscribe(move || {
            let state = store.get_state();
            if let Some(ref toggle) = *toggle_ref.borrow() {
                toggle.set_active(state.active);

                // Set period-aware label
                let details = if let Some(ref time) = state.next_transition {
                    let is_night = state
                        .period
                        .as_ref()
                        .map(|p| !p.eq_ignore_ascii_case("day"))
                        .unwrap_or(false);

                    let key = if is_night {
                        "nightlight-night-until"
                    } else {
                        "nightlight-day-until"
                    };

                    Some(crate::i18n::t_args(key, &[("time", time)]))
                } else {
                    None
                };

                toggle.set_details(details);
                toggle.set_busy(state.busy);
            }
        });

        // Handle IPC events
        let store_for_ipc = self.store.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = ipc_rx.recv_async().await {
                debug!("[sunsetr/ipc] Received event: {:?}", event);
                match event {
                    SunsetrIpcEvents::Status(status) => {
                        store_for_ipc.emit(SunsetrOp::SetStatus {
                            active: status.active,
                            period: status.period,
                            next_transition: status.next_transition_text,
                        });
                        store_for_ipc.emit(SunsetrOp::SetBusy(false));
                    }
                    SunsetrIpcEvents::Busy(busy) => {
                        store_for_ipc.emit(SunsetrOp::SetBusy(busy));
                    }
                    SunsetrIpcEvents::Error(_error) => {
                        // Errors are logged by the IPC module
                    }
                }
            }
        });

        // Start following sunsetr events
        spawn_following(ipc_tx)?;

        Ok(())
    }
}
