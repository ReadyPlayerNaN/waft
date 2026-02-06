//! Darkman plugin — dark mode toggle.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Controls the darkman service via D-Bus to switch between light and dark mode.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use log::{debug, error, info};

use waft_core::dbus::DbusHandle;
use waft_core::menu_state::MenuStore;
use waft_plugin_api::ui::feature_toggle::{
    FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget,
};
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, WidgetFeatureToggle, WidgetRegistrar};

use self::dbus::DARKMAN_DESTINATION;
use self::dbus::{get_state, set_state};
use self::store::{DarkmanOp, DarkmanStore, create_darkman_store};
use self::values::DarkmanMode;

mod dbus;
mod store;
mod values;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::darkman", "Darkman", "0.1.0");
waft_plugin_api::export_overview_plugin!(DarkmanPlugin::new());

pub struct DarkmanPlugin {
    store: Rc<DarkmanStore>,
    dbus: Option<Arc<DbusHandle>>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
    mode_channel: (flume::Sender<DarkmanMode>, flume::Receiver<DarkmanMode>),
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for DarkmanPlugin {
    fn default() -> Self {
        Self {
            store: Rc::new(create_darkman_store()),
            dbus: None,
            toggle: Rc::new(RefCell::new(None)),
            mode_channel: flume::unbounded(),
            tokio_handle: None,
        }
    }
}

impl DarkmanPlugin {
    pub fn new() -> Self {
        Self::default()
    }

}

#[async_trait(?Send)]
impl OverviewPlugin for DarkmanPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::darkman")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[darkman] init() called");

        // Use the session dbus connection provided by the host
        let dbus = resources
            .session_dbus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session_dbus not provided"))?
            .clone();
        debug!("[darkman] Received dbus connection from host");

        // Save the tokio handle for use in create_elements
        self.tokio_handle = resources.tokio_handle.clone();

        // Get initial state
        debug!("[darkman] Getting initial state");
        let initial_mode = get_state(&dbus).await?;
        debug!("[darkman] Initial mode: {:?}", initial_mode);
        self.store.emit(DarkmanOp::SetMode(initial_mode));

        self.dbus = Some(dbus);
        debug!("[darkman] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let initial_active = {
            let state = self.store.get_state();
            DarkmanMode::is_active(state.mode)
        };

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: "Dark Mode".into(),
                icon: "weather-clear-night-symbolic".into(),
                details: None,
                active: initial_active,
                busy: false,
                expandable: false,
            },
            None, // No menu support
        );

        // Connect output handler
        let dbus = self.dbus.clone().expect("dbus not initialized");
        let store = self.store.clone();
        toggle.connect_output(move |event| {
            debug!("[darkman/ui] Received: {:?}", event);
            let dbus = dbus.clone();
            let store = store.clone();

            glib::spawn_future_local(async move {
                // Set busy state
                store.emit(DarkmanOp::SetBusy(true));

                let result = match event {
                    FeatureToggleOutput::Activate => set_state(dbus, DarkmanMode::Dark).await,
                    FeatureToggleOutput::Deactivate => set_state(dbus, DarkmanMode::Light).await,
                };

                if let Err(err) = result {
                    error!("Failed to set darkman state: {}", err);
                    // Reset busy state on error
                    store.emit(DarkmanOp::SetBusy(false));
                }
            });
        });

        // Register the feature toggle
        registrar.register_feature_toggle(Rc::new(WidgetFeatureToggle {
            id: "darkman:toggle".to_string(),
            el: toggle.root.clone().upcast::<gtk::Widget>(),
            weight: 190,
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
                toggle.set_active(DarkmanMode::is_active(state.mode));
                toggle.set_busy(state.busy);
            }
        });

        // Start D-Bus monitoring using the tokio runtime handle from the host
        debug!("[darkman] Starting D-Bus monitoring");
        let mode_tx = self.mode_channel.0.clone();
        let dbus_for_monitor = self.dbus.clone().expect("dbus not initialized");
        let tokio_handle = self
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;

        // Set up D-Bus signal monitoring with the tokio handle
        let handle_value = move |value: Option<String>| {
            if let Some(value) = value {
                let mode = DarkmanMode::from_str(&value).unwrap_or(DarkmanMode::Light);
                let _ = mode_tx.send(mode);
                info!("[darkman/dbus] Mode changed to: {:?}", mode);
            }
        };

        if let Err(e) = dbus_for_monitor
            .listen_for_values_with_handle(
                DARKMAN_DESTINATION,
                "ModeChanged",
                handle_value,
                Some(tokio_handle),
            )
            .await
        {
            error!("[darkman] Failed to start D-Bus monitoring: {}", e);
        } else {
            debug!("[darkman] D-Bus monitoring started");
        }

        // Handle mode changes from DBus monitoring
        let store_for_mode = self.store.clone();
        let mode_rx = self.mode_channel.1.clone();
        glib::spawn_future_local(async move {
            while let Ok(mode) = mode_rx.recv_async().await {
                store_for_mode.emit(DarkmanOp::SetMode(mode));
                store_for_mode.emit(DarkmanOp::SetBusy(false));
            }
        });

        Ok(())
    }
}
