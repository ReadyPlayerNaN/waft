//! Caffeine plugin - screen lock/screensaver inhibition toggle.

use crate::menu_state::MenuStore;

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use crate::dbus::DbusHandle;
use crate::plugin::{Plugin, PluginId, WidgetFeatureToggle, WidgetRegistrar};
use crate::ui::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

use self::backends::{InhibitBackend, inhibit, probe_backends, uninhibit};
use self::store::{CaffeineOp, CaffeineStore, create_caffeine_store};

mod backends;
mod store;
mod wayland_protocol;

pub struct CaffeinePlugin {
    store: Rc<CaffeineStore>,
    dbus: Arc<DbusHandle>,
    backend: RefCell<Option<InhibitBackend>>,
    toggle: Rc<RefCell<Option<FeatureToggleWidget>>>,
    app: RefCell<Option<gtk::Application>>,
}

impl CaffeinePlugin {
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        Self {
            store: Rc::new(create_caffeine_store()),
            dbus,
            backend: RefCell::new(None),
            toggle: Rc::new(RefCell::new(None)),
            app: RefCell::new(None),
        }
    }
}

#[async_trait(?Send)]
impl Plugin for CaffeinePlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::caffeine")
    }

    async fn init(&mut self) -> Result<()> {
        let backend = probe_backends(&self.dbus).await?;
        info!("[caffeine] Using backend: {:?}", backend);
        *self.backend.borrow_mut() = Some(backend);
        Ok(())
    }

    async fn create_elements(
        &mut self,
        app: &gtk::Application,
        _menu_store: Arc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        // Store the app reference for getting window later
        *self.app.borrow_mut() = Some(app.clone());

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: crate::i18n::t("caffeine-title"),
                icon: "changes-allow-symbolic".into(),
                details: None,
                active: false,
                busy: false,
                expandable: false,
            },
            None, // No menu support
        );

        // Connect output handler
        let dbus = self.dbus.clone();
        let store = self.store.clone();
        let backend_cell = self.backend.clone();
        let app_cell = self.app.clone();

        toggle.connect_output(move |event| {
            debug!("[caffeine/ui] Received: {:?}", event);
            let dbus = dbus.clone();
            let store = store.clone();
            let backend_cell = backend_cell.clone();
            let app_cell = app_cell.clone();

            glib::spawn_future_local(async move {
                let mut backend_borrow = backend_cell.borrow_mut();
                let Some(ref mut backend) = *backend_borrow else {
                    error!("[caffeine] No backend available");
                    return;
                };

                // Get the window from the app
                let window = app_cell
                    .borrow()
                    .as_ref()
                    .and_then(|app| app.active_window());

                // Set busy state
                store.emit(CaffeineOp::SetBusy(true));

                let result = match event {
                    FeatureToggleOutput::Activate => inhibit(&dbus, backend, window.as_ref()).await,
                    FeatureToggleOutput::Deactivate => uninhibit(&dbus, backend).await,
                };

                match result {
                    Ok(active) => {
                        store.emit(CaffeineOp::SetActive(active));
                        store.emit(CaffeineOp::SetBusy(false));
                    }
                    Err(err) => {
                        error!("[caffeine] Failed to toggle: {}", err);
                        store.emit(CaffeineOp::SetBusy(false));
                    }
                }
            });
        });

        // Register the feature toggle
        registrar.register_feature_toggle(Arc::new(WidgetFeatureToggle {
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
