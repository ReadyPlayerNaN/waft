//! Battery plugin — display battery status from UPower.
use crate::menu_state::MenuStore;

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc; // DbusHandle is Arc

use gtk::prelude::*;

use crate::dbus::DbusHandle;
use crate::plugin::{Plugin, PluginId, Slot, Widget, WidgetRegistrar};
use crate::ui::battery::BatteryWidget;

use self::dbus::{get_battery_info, listen_battery_changes};
use self::store::{BatteryOp, BatteryStore, create_battery_store};
use self::values::BatteryInfo;

mod dbus;
pub mod store;
pub mod values;

pub struct BatteryPlugin {
    dbus: Arc<DbusHandle>,
    store: Rc<BatteryStore>,
    widget: Rc<RefCell<Option<BatteryWidget>>>,
    info_channel: (flume::Sender<BatteryInfo>, flume::Receiver<BatteryInfo>),
}

impl BatteryPlugin {
    pub fn new(dbus: Arc<DbusHandle>) -> Self {
        Self {
            dbus,
            store: Rc::new(create_battery_store()),
            widget: Rc::new(RefCell::new(None)),
            info_channel: flume::unbounded(),
        }
    }
}

#[async_trait(?Send)]
impl Plugin for BatteryPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::battery")
    }

    async fn init(&mut self, _resources: &super::super::plugin::PluginResources) -> Result<()> {
        match get_battery_info(&self.dbus).await {
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

        if let Err(e) = listen_battery_changes(&self.dbus, self.info_channel.0.clone()).await {
            warn!("[battery] Failed to start monitoring: {e}");
        }

        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
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
