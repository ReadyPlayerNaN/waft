//! Brightness plugin — display brightness control.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! Provides a master brightness slider with per-display control in an expandable menu.
//! Supports backlight devices (via brightnessctl) and external monitors (via ddcutil).

use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use log::{debug, error, info, warn};
use std::cell::RefCell;
use std::rc::Rc;

use waft_core::menu_state::MenuStore;
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, Widget, WidgetRegistrar, Slot};

use self::control_widget::{
    BrightnessControlOutput, BrightnessControlProps, BrightnessControlWidget,
};
use self::dbus::{
    DiscoveredDisplay, discover_backlight_devices, discover_ddc_monitors,
    is_brightnessctl_available, is_ddcutil_available, set_brightness,
};
use self::store::{BrightnessOp, BrightnessStore, Display, DisplayType, create_brightness_store};

mod control_widget;
mod dbus;
mod display_menu;
mod slider_control;
pub mod store;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::brightness", "Brightness", "0.1.0");
waft_plugin_api::export_overview_plugin!(BrightnessPlugin::new());

pub struct BrightnessPlugin {
    store: Rc<BrightnessStore>,
    control: Rc<RefCell<Option<BrightnessControlWidget>>>,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for BrightnessPlugin {
    fn default() -> Self {
        Self {
            store: Rc::new(create_brightness_store()),
            control: Rc::new(RefCell::new(None)),
            tokio_handle: None,
        }
    }
}

impl BrightnessPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Discover all controllable displays from available backends.
    async fn discover_displays(&self, tokio_handle: &tokio::runtime::Handle) -> Vec<Display> {
        let mut displays = Vec::new();

        // Check brightnessctl backend
        if is_brightnessctl_available(tokio_handle).await {
            debug!("[brightness] brightnessctl is available");
            match discover_backlight_devices(tokio_handle).await {
                Ok(backlight_displays) => {
                    debug!(
                        "[brightness] Found {} backlight devices",
                        backlight_displays.len()
                    );
                    for d in backlight_displays {
                        displays.push(discovered_to_display(d));
                    }
                }
                Err(e) => {
                    warn!("[brightness] Failed to discover backlight devices: {}", e);
                }
            }
        } else {
            info!("[brightness] brightnessctl not available");
        }

        // Check ddcutil backend
        if is_ddcutil_available(tokio_handle).await {
            debug!("[brightness] ddcutil is available");
            match discover_ddc_monitors(tokio_handle).await {
                Ok(ddc_displays) => {
                    debug!("[brightness] Found {} DDC monitors", ddc_displays.len());
                    for d in ddc_displays {
                        displays.push(discovered_to_display(d));
                    }
                }
                Err(e) => {
                    warn!("[brightness] Failed to discover DDC monitors: {}", e);
                }
            }
        } else {
            info!("[brightness] ddcutil not available");
        }

        // Sort: backlights first, then externals, alphabetically within each group
        displays.sort_by(|a, b| match (&a.display_type, &b.display_type) {
            (DisplayType::Backlight, DisplayType::External) => std::cmp::Ordering::Less,
            (DisplayType::External, DisplayType::Backlight) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        displays
    }
}

/// Convert a discovered display to a store Display.
fn discovered_to_display(d: DiscoveredDisplay) -> Display {
    Display {
        id: d.id,
        name: d.name,
        display_type: d.display_type,
        brightness: d.brightness,
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for BrightnessPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::brightness")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        debug!("[brightness] init() called");

        // Save the tokio handle and enter runtime context for this plugin's copy of tokio
        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();
        self.tokio_handle = Some(tokio_handle.clone());

        debug!("[brightness] Received tokio handle from host");

        // Discover available displays
        let displays = self
            .discover_displays(
                self.tokio_handle
                    .as_ref()
                    .expect("tokio_handle just set above"),
            )
            .await;

        if displays.is_empty() {
            debug!("[brightness] No controllable displays found");
            return Ok(());
        }

        info!(
            "[brightness] Found {} controllable displays",
            displays.len()
        );
        self.store.emit(BrightnessOp::Available(true));
        self.store.emit(BrightnessOp::Displays(displays));

        debug!("[brightness] init() completed successfully");
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());
        let state = self.store.get_state();
        if !state.available || state.displays.is_empty() {
            debug!("[brightness] Skipping widget creation - no displays available");
            return Ok(());
        }

        // Create brightness control widget
        let control = BrightnessControlWidget::new(
            BrightnessControlProps {
                displays: state.displays.clone(),
            },
            menu_store,
        );

        // Connect output events to backend
        let store_for_output = self.store.clone();
        let tokio_handle = self.tokio_handle.clone().expect("tokio_handle not set");
        control.connect_output(move |event| {
            match event {
                BrightnessControlOutput::MasterChanged(_) => {
                    // Master changed is informational; actual changes come via DisplayChanged
                }
                BrightnessControlOutput::DisplayChanged {
                    display_id,
                    brightness,
                } => {
                    // Update store
                    store_for_output.emit(BrightnessOp::Brightness {
                        display_id: display_id.clone(),
                        brightness,
                    });

                    // Call backend to set brightness
                    let display_id_clone = display_id.clone();
                    let display_id_for_log = display_id.clone();
                    let tokio_handle_clone = tokio_handle.clone();
                    glib::spawn_future_local(async move {
                        // Route through std::thread + block_on to avoid cdylib tokio TLS issues
                        let (tx, rx) = flume::bounded(1);
                        let h = tokio_handle_clone.clone();
                        std::thread::spawn(move || {
                            let result = h.block_on(set_brightness(&display_id_clone, brightness, &h));
                            let _ = tx.send(result);
                        });

                        match rx.recv_async().await {
                            Ok(Ok(())) => {
                                debug!("[brightness] Set brightness for {} to {}", display_id_for_log, brightness);
                            }
                            Ok(Err(e)) => {
                                error!(
                                    "[brightness] Failed to set brightness for {}: {}",
                                    display_id_for_log, e
                                );
                            }
                            Err(e) => {
                                error!("[brightness] Backend task cancelled: {}", e);
                            }
                        }
                    });
                }
            }
        });

        *self.control.borrow_mut() = Some(control);

        // Register widget
        if let Some(ref control) = *self.control.borrow() {
            registrar.register_widget(Rc::new(Widget {
                id: "brightness:control".to_string(),
                slot: Slot::Controls,
                el: control.root.clone().upcast::<gtk::Widget>(),
                weight: 60,
            }));
        }

        // Subscribe to store changes to update widget
        let control_ref = self.control.clone();
        let store_ref = self.store.clone();
        // Track previous state to detect what changed
        let prev_displays: Rc<RefCell<Vec<Display>>> =
            Rc::new(RefCell::new(state.displays.clone()));

        self.store.subscribe(move || {
            let state = store_ref.get_state();
            if let Some(ref control) = *control_ref.borrow() {
                // Check if display list changed
                let prev = prev_displays.borrow();
                if state.displays.len() != prev.len()
                    || state
                        .displays
                        .iter()
                        .zip(prev.iter())
                        .any(|(a, b)| a.id != b.id)
                {
                    // Display list changed - full refresh
                    drop(prev);
                    *prev_displays.borrow_mut() = state.displays.clone();
                    control.set_displays(state.displays);
                } else {
                    // Check for brightness changes on individual displays
                    for (current, previous) in state.displays.iter().zip(prev.iter()) {
                        if (current.brightness - previous.brightness).abs() > 0.001 {
                            control.update_brightness(&current.id, current.brightness);
                        }
                    }
                    drop(prev);
                    *prev_displays.borrow_mut() = state.displays.clone();
                }
            }
        });

        Ok(())
    }
}
