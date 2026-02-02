//! Sunsetr plugin - night light toggle.
use crate::menu_state::MenuStore;

use anyhow::Result;
use async_trait::async_trait;
use flume::unbounded;
use log::{debug, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::plugin::{ExpandCallback, Plugin, PluginId, WidgetFeatureToggle, WidgetRegistrar};
use crate::ui::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

mod ipc;
mod preset_menu;
pub mod store;
mod values;

use self::ipc::SunsetrIpcEvents;
use self::ipc::{spawn_following, spawn_start, spawn_stop};
use self::preset_menu::{PresetMenuOutput, PresetMenuWidget};
use self::store::{SunsetrOp, SunsetrStore, create_sunsetr_store};

pub struct SunsetrPlugin {
    store: Rc<SunsetrStore>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
    preset_menu: Rc<RefCell<Option<PresetMenuWidget>>>,
}

impl SunsetrPlugin {
    pub fn new() -> Self {
        Self {
            store: Rc::new(create_sunsetr_store()),
            toggle: Rc::new(RefCell::new(None)),
            preset_menu: Rc::new(RefCell::new(None)),
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
        menu_store: Arc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        // Load presets initially to determine if toggle should be expandable
        let has_presets = match ipc::query_presets().await {
            Ok(presets) => !presets.is_empty(),
            Err(_) => false,
        };
        self.store.emit(SunsetrOp::SetHasPresets(has_presets));

        let initial_state = {
            let state = self.store.get_state();
            (state.active, state.next_transition.clone(), state.has_presets)
        };

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: crate::i18n::t("nightlight-title").into(),
                icon: "night-light-symbolic".into(),
                details: initial_state.1.clone(),
                active: initial_state.0,
                busy: false,
                expandable: initial_state.0 && initial_state.2, // Expandable when active AND presets available
            },
            Some(menu_store.clone()), // Menu support enabled
        );

        // Create preset menu
        let preset_menu = PresetMenuWidget::new();

        // Create IPC channel
        let (ipc_tx, ipc_rx) = unbounded::<SunsetrIpcEvents>();

        // Connect preset menu output handler
        let ipc_sender_for_preset = ipc_tx.clone();
        preset_menu.connect_output(move |event| {
            debug!("[sunsetr/preset-menu] Received: {:?}", event);
            match event {
                PresetMenuOutput::SelectPreset(preset_name) => {
                    let ipc_sender = ipc_sender_for_preset.clone();
                    let preset = preset_name.clone();
                    tokio::spawn(async move {
                        if let Err(e) = ipc::set_preset(&preset).await {
                            warn!("[sunsetr] preset switch failed: {e}");
                        }
                        // Trigger a status refresh after preset switch
                        if let Err(e) = spawn_start(ipc_sender).await {
                            warn!("[sunsetr] refresh after preset switch failed: {e}");
                        }
                    });
                }
            }
        });

        // Connect toggle output handler
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

        // Set up expand callback to load presets
        // Use a channel to send presets from tokio to glib thread
        let (preset_tx, preset_rx) = unbounded::<Vec<String>>();
        let preset_menu_for_rx = self.preset_menu.clone();
        let store_for_presets = self.store.clone();

        // Handle incoming preset lists on glib thread
        glib::spawn_future_local(async move {
            while let Ok(presets) = preset_rx.recv_async().await {
                // Update has_presets flag in store
                let has_presets = !presets.is_empty();
                store_for_presets.emit(SunsetrOp::SetHasPresets(has_presets));

                // Update preset menu
                if let Some(ref menu) = *preset_menu_for_rx.borrow() {
                    menu.set_presets(presets);
                }
            }
        });

        toggle.set_expand_callback(move |will_be_open| {
            if will_be_open {
                debug!("[sunsetr] Menu expanded, loading presets");
                let sender = preset_tx.clone();
                tokio::spawn(async move {
                    match ipc::query_presets().await {
                        Ok(presets) => {
                            debug!("[sunsetr] Loaded {} presets", presets.len());
                            let _ = sender.send(presets);
                        }
                        Err(e) => {
                            warn!("[sunsetr] Failed to load presets: {e}");
                            // Send empty list on error
                            let _ = sender.send(vec![]);
                        }
                    }
                });
            }
        });

        let expand_callback: ExpandCallback = Rc::new(RefCell::new(None));

        // Register the feature toggle
        registrar.register_feature_toggle(Arc::new(WidgetFeatureToggle {
            id: "sunsetr:toggle".to_string(),
            el: toggle.widget(),
            weight: 200,
            menu: Some(preset_menu.widget()),
            menu_id: toggle.menu_id.clone(),
            on_expand_toggled: Some(expand_callback),
        }));

        *self.toggle.borrow_mut() = Some(toggle);
        *self.preset_menu.borrow_mut() = Some(preset_menu);

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

                // Toggle expandability based on active state AND preset availability
                // Only show expand button when sunsetr is running and presets are configured
                toggle.set_expandable(state.active && state.has_presets);
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
