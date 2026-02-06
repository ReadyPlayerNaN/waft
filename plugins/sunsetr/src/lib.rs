//! Sunsetr plugin — night light toggle.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Controls the sunsetr CLI tool to manage screen color temperature.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use log::{debug, warn};

use waft_core::menu_state::MenuStore;
use waft_plugin_api::ui::feature_toggle::{
    FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget,
};
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, WidgetFeatureToggle, WidgetRegistrar};

use self::ipc::{SunsetrIpcEvents, spawn_start, spawn_stop};
use self::preset_menu::{PresetMenuOutput, PresetMenuWidget};
use self::store::{SunsetrOp, SunsetrStore, create_sunsetr_store};

mod ipc;
mod preset_menu;
mod store;
mod values;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::sunsetr", "Sunsetr", "0.1.0");
waft_plugin_api::export_overview_plugin!(SunsetrPlugin::new());

pub struct SunsetrPlugin {
    store: Rc<SunsetrStore>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
    preset_menu: Rc<RefCell<Option<PresetMenuWidget>>>,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for SunsetrPlugin {
    fn default() -> Self {
        Self {
            store: Rc::new(create_sunsetr_store()),
            toggle: Rc::new(RefCell::new(None)),
            preset_menu: Rc::new(RefCell::new(None)),
            tokio_handle: None,
        }
    }
}

impl SunsetrPlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for SunsetrPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::sunsetr")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[sunsetr] init() called");

        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();

        // Save the tokio handle for use in create_elements
        self.tokio_handle = Some(tokio_handle.clone());

        debug!("[sunsetr] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let tokio_handle = self
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();

        // Load presets initially to determine if toggle should be expandable
        let has_presets = ipc::query_presets().await?;
        let has_presets = !has_presets.is_empty();
        self.store.emit(SunsetrOp::HasPresets(has_presets));

        let initial_state = {
            let state = self.store.get_state();
            (state.active, state.next_transition.clone(), state.has_presets)
        };

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: "Night Light".into(),
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
        let (ipc_tx, ipc_rx) = flume::unbounded::<SunsetrIpcEvents>();

        // Connect preset menu output handler
        let _ipc_sender_for_preset = ipc_tx.clone();
        let tokio_handle_for_preset = tokio_handle.clone();
        preset_menu.connect_output(move |event| {
            debug!("[sunsetr/preset-menu] Received: {:?}", event);

            // Use std::thread + block_on to run async IPC calls with proper
            // tokio context (handle.spawn runs on host worker threads)
            let handle = tokio_handle_for_preset.clone();
            std::thread::spawn(move || {
                let preset_name = match event {
                    PresetMenuOutput::SelectPreset(name) => name,
                    PresetMenuOutput::SelectDefault => "default".to_string(),
                };

                if let Err(e) = handle.block_on(ipc::set_preset(&preset_name)) {
                    warn!("[sunsetr] preset switch to '{}' failed: {e}", preset_name);
                    return;
                }

                debug!("[sunsetr] preset switch to '{}' completed", preset_name);
            });
        });

        // Connect toggle output handler
        let ipc_sender = ipc_tx.clone();
        let tokio_handle_for_toggle = tokio_handle.clone();
        toggle.connect_output(move |event| {
            debug!("[sunsetr/ui] Received: {:?}", event);
            let ipc_sender = ipc_sender.clone();
            let handle = tokio_handle_for_toggle.clone();

            // Use std::thread + block_on to run async IPC calls
            std::thread::spawn(move || {
                let result = handle.block_on(async move {
                    match event {
                        FeatureToggleOutput::Activate => spawn_start(ipc_sender).await,
                        FeatureToggleOutput::Deactivate => spawn_stop(ipc_sender).await,
                    }
                });
                if let Err(e) = result {
                    warn!("[sunsetr] toggle action failed: {e}");
                }
            });
        });

        // Set up expand callback to load presets
        // Use a channel to send presets from tokio to glib thread
        let (preset_tx, preset_rx) = flume::unbounded::<(Vec<String>, Option<String>)>();
        let preset_menu_for_rx = self.preset_menu.clone();
        let store_for_presets = self.store.clone();

        // Handle incoming preset lists on glib thread
        glib::spawn_future_local(async move {
            while let Ok((presets, active_preset)) = preset_rx.recv_async().await {
                // Update has_presets flag in store
                let has_presets = !presets.is_empty();
                store_for_presets.emit(SunsetrOp::HasPresets(has_presets));

                // Update preset menu
                if let Some(ref menu) = *preset_menu_for_rx.borrow() {
                    menu.set_presets(presets, active_preset);
                }
            }
        });

        let store_for_expand = self.store.clone();
        let tokio_handle_for_expand = tokio_handle.clone();
        toggle.set_expand_callback(move |will_be_open| {
            if will_be_open {
                debug!("[sunsetr] Menu expanded, loading presets");
                let sender = preset_tx.clone();
                let handle = tokio_handle_for_expand.clone();
                // Extract active preset before entering tokio context
                let active_preset = store_for_expand.get_state().active_preset.clone();
                std::thread::spawn(move || {
                    match handle.block_on(ipc::query_presets()) {
                        Ok(presets) => {
                            debug!("[sunsetr] Loaded {} presets", presets.len());
                            let _ = sender.send((presets, active_preset));
                        }
                        Err(e) => {
                            warn!("[sunsetr] Failed to load presets: {e}");
                            // Send empty list on error
                            let _ = sender.send((vec![], None));
                        }
                    }
                });
            }
        });

        let expand_callback = Rc::new(RefCell::new(None));

        // Register the feature toggle
        registrar.register_feature_toggle(Rc::new(WidgetFeatureToggle {
            id: "sunsetr:toggle".to_string(),
            el: toggle.root.clone().upcast::<gtk::Widget>(),
            weight: 200,
            menu: Some(preset_menu.widget()),
            menu_id: toggle.menu_id.clone(),
            on_expand_toggled: Some(expand_callback),
        }));

        *self.toggle.borrow_mut() = Some(toggle);
        *self.preset_menu.borrow_mut() = Some(preset_menu);

        // Subscribe to store for state changes
        let toggle_ref = self.toggle.clone();
        let preset_menu_ref = self.preset_menu.clone();
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
                        "Night until"
                    } else {
                        "Day until"
                    };

                    Some(format!("{} {}", key, time))
                } else {
                    None
                };

                toggle.set_details(details);
                toggle.set_busy(state.busy);

                // Toggle expandability based on active state AND preset availability
                // Only show expand button when sunsetr is running and presets are configured
                toggle.set_expandable(state.active && state.has_presets);
            }

            // Update preset menu checkmarks when active preset changes
            if let Some(ref menu) = *preset_menu_ref.borrow() {
                menu.update_active_preset(state.active_preset.clone());
            }
        });

        // Handle IPC events
        let store_for_ipc = self.store.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = ipc_rx.recv_async().await {
                debug!("[sunsetr/ipc] Received event: {:?}", event);
                match event {
                    SunsetrIpcEvents::Status(status) => {
                        store_for_ipc.emit(SunsetrOp::Status {
                            active: status.active,
                            period: status.period,
                            next_transition: status.next_transition_text,
                        });
                        store_for_ipc.emit(SunsetrOp::Busy(false));
                    }
                    SunsetrIpcEvents::Busy(busy) => {
                        store_for_ipc.emit(SunsetrOp::Busy(busy));
                    }
                    SunsetrIpcEvents::ActivePreset(preset) => {
                        store_for_ipc.emit(SunsetrOp::ActivePreset(preset));
                    }
                    SunsetrIpcEvents::Error(_error) => {
                        // Errors are logged by the IPC module
                    }
                }
            }
        });

        // Start following sunsetr events using tokio handle
        ipc::spawn_following_with_handle(ipc_tx, tokio_handle)?;

        Ok(())
    }
}
