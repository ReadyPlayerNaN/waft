//! Caffeine plugin — screen lock/screensaver inhibition toggle.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Inhibits screen lock and screensavers through multiple backends (Portal, ScreenSaver, Wayland).

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

use self::backends::{InhibitBackend, inhibit, probe_backends, uninhibit};
use self::store::{CaffeineOp, CaffeineStore, create_caffeine_store};

mod backends;
mod store;
mod wayland_protocol;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::caffeine", "Caffeine", "0.1.0");
waft_plugin_api::export_overview_plugin!(CaffeinePlugin::new());

pub struct CaffeinePlugin {
    store: Rc<CaffeineStore>,
    dbus: Option<Arc<DbusHandle>>,
    tokio_handle: Option<tokio::runtime::Handle>,
    backend: RefCell<Option<InhibitBackend>>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
    app: RefCell<Option<gtk::Application>>,
}

impl Default for CaffeinePlugin {
    fn default() -> Self {
        Self {
            store: Rc::new(create_caffeine_store()),
            dbus: None,
            tokio_handle: None,
            backend: RefCell::new(None),
            toggle: Rc::new(RefCell::new(None)),
            app: RefCell::new(None),
        }
    }
}

impl CaffeinePlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for CaffeinePlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::caffeine")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[caffeine] init() called");

        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();
        self.tokio_handle = Some(tokio_handle.clone());

        // Use the session dbus connection provided by the host
        let dbus = resources
            .session_dbus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session_dbus not provided"))?
            .clone();
        debug!("[caffeine] Received dbus connection from host");

        // Probe for available backend
        let backend = probe_backends(&dbus).await?;
        info!("[caffeine] Using backend: {:?}", backend);
        *self.backend.borrow_mut() = Some(backend);

        self.dbus = Some(dbus);
        debug!("[caffeine] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());
        // Store the app reference for getting window later
        *self.app.borrow_mut() = Some(app.clone());

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: waft_plugin_api::i18n::t("caffeine-title"),
                icon: "changes-allow-symbolic".into(),
                details: None,
                active: false,
                busy: false,
                expandable: false,
            },
            None, // No menu support
        );

        // Connect output handler
        let dbus = self.dbus.clone().expect("dbus not initialized");
        let store = self.store.clone();
        let backend_cell = self.backend.clone();
        let tokio_handle = self
            .tokio_handle
            .clone()
            .expect("tokio_handle not initialized");

        toggle.connect_output(move |event| {
            debug!("[caffeine/ui] Received: {:?}", event);
            let dbus = dbus.clone();
            let store = store.clone();
            let backend_cell = backend_cell.clone();
            let handle = tokio_handle.clone();

            glib::spawn_future_local(async move {
                // Take backend out (brief borrow)
                let mut backend = {
                    let mut cell = backend_cell.borrow_mut();
                    match cell.take() {
                        Some(b) => b,
                        None => {
                            error!("[caffeine] No backend available");
                            return;
                        }
                    }
                };

                store.emit(CaffeineOp::SetBusy(true));

                // Route D-Bus call through std::thread + block_on to avoid
                // cdylib tokio TLS issues (zbus::Proxy needs tokio context)
                let (tx, rx) = flume::bounded(1);
                let h = handle.clone();
                let d = dbus.clone();
                std::thread::spawn(move || {
                    let result = h.block_on(async {
                        match event {
                            FeatureToggleOutput::Activate => inhibit(&d, &mut backend, None).await,
                            FeatureToggleOutput::Deactivate => uninhibit(&d, &mut backend).await,
                        }
                    });
                    let _ = tx.send((result, backend));
                });

                match rx.recv_async().await {
                    Ok((Ok(active), returned_backend)) => {
                        store.emit(CaffeineOp::SetActive(active));
                        store.emit(CaffeineOp::SetBusy(false));
                        *backend_cell.borrow_mut() = Some(returned_backend);
                    }
                    Ok((Err(err), returned_backend)) => {
                        error!("[caffeine] Failed to toggle: {}", err);
                        store.emit(CaffeineOp::SetBusy(false));
                        *backend_cell.borrow_mut() = Some(returned_backend);
                    }
                    Err(err) => {
                        error!("[caffeine] Toggle task failed: {}", err);
                        store.emit(CaffeineOp::SetBusy(false));
                    }
                }
            });
        });

        // Register the feature toggle
        registrar.register_feature_toggle(Rc::new(WidgetFeatureToggle {
            id: "caffeine:toggle".to_string(),
            el: toggle.root.clone().upcast::<gtk::Widget>(),
            weight: 65,
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
                toggle.set_busy(state.busy);
            }
        });

        Ok(())
    }
}
