//! Battery plugin — display battery status from UPower.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Monitors the UPower DisplayDevice on the system bus.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use log::{debug, warn};

use waft_core::dbus::DbusHandle;
use waft_core::menu_state::MenuStore;
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, Widget, WidgetRegistrar, Slot};

use self::dbus::{get_battery_info, listen_battery_changes};
use self::store::{BatteryOp, BatteryStore, create_battery_store};
use self::ui::BatteryWidget;
use self::values::BatteryInfo;

mod dbus;
mod store;
mod ui;
mod values;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::battery", "Battery", "0.1.0");
waft_plugin_api::export_overview_plugin!(BatteryPlugin::new());

pub struct BatteryPlugin {
    dbus: Option<std::sync::Arc<DbusHandle>>,
    store: Rc<BatteryStore>,
    widget: Rc<RefCell<Option<BatteryWidget>>>,
    info_channel: (flume::Sender<BatteryInfo>, flume::Receiver<BatteryInfo>),
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for BatteryPlugin {
    fn default() -> Self {
        Self {
            dbus: None,
            store: Rc::new(create_battery_store()),
            widget: Rc::new(RefCell::new(None)),
            info_channel: flume::unbounded(),
            tokio_handle: None,
        }
    }
}

impl BatteryPlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for BatteryPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::battery")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[battery] init() called");

        // Use the system dbus connection provided by the host
        let dbus = resources
            .system_dbus
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("system_dbus not provided"))?
            .clone();
        debug!("[battery] Received system dbus connection from host");

        // Save the tokio handle and enter runtime context for this plugin's copy of tokio
        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();
        self.tokio_handle = Some(tokio_handle.clone());

        // Get initial battery info
        debug!("[battery] Getting initial battery info");
        match get_battery_info(&dbus).await {
            Ok(info) => {
                debug!(
                    "[battery] Initial state: present={}, {}%, {:?}",
                    info.present, info.percentage, info.state
                );
                self.store.emit(BatteryOp::SetInfo(info));
            }
            Err(e) => {
                warn!("[battery] Failed to read initial battery info: {e}");
            }
        }

        self.dbus = Some(dbus);
        debug!("[battery] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());
        let battery_widget = BatteryWidget::new();

        // Apply initial state
        {
            let state = self.store.get_state();
            battery_widget.update(&state.info);
        }

        // Register the widget
        registrar.register_widget(Rc::new(Widget {
            id: "battery:main".to_string(),
            slot: Slot::Header,
            el: battery_widget.root.clone().upcast::<gtk::Widget>(),
            weight: 30,
        }));

        *self.widget.borrow_mut() = Some(battery_widget);

        // Subscribe to store changes
        let widget_ref = self.widget.clone();
        let store = self.store.clone();
        self.store.subscribe(move || {
            let state = store.get_state();
            if let Some(ref widget) = *widget_ref.borrow() {
                widget.update(&state.info);
            }
        });

        // Start D-Bus monitoring using the tokio runtime handle from the host
        debug!("[battery] Starting D-Bus monitoring");
        let dbus_for_monitor = self.dbus.clone().expect("dbus not initialized");
        let tokio_handle = self
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;

        if let Err(e) = listen_battery_changes(
            &dbus_for_monitor,
            self.info_channel.0.clone(),
            Some(tokio_handle),
        )
        .await
        {
            warn!("[battery] Failed to start monitoring: {e}");
        } else {
            debug!("[battery] D-Bus monitoring started");
        }

        // Forward info from D-Bus channel to store
        let store_for_channel = self.store.clone();
        let info_rx = self.info_channel.1.clone();
        glib::spawn_future_local(async move {
            while let Ok(info) = info_rx.recv_async().await {
                store_for_channel.emit(BatteryOp::SetInfo(info));
            }
            warn!("[battery] info receiver loop exited — battery updates will stop");
        });

        Ok(())
    }
}
