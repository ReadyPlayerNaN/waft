//! Brightness plugin - display brightness control.
//!
//! Provides a master brightness slider with per-display control in an expandable menu.
//! Supports backlight devices (via brightnessctl) and external monitors (via ddcutil).

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::menu_state::MenuStore;
use crate::plugin::{Plugin, PluginId, Slot, Widget, WidgetRegistrar};

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
pub mod store;

pub struct BrightnessPlugin {
    store: Rc<BrightnessStore>,
    control: Rc<RefCell<Option<BrightnessControlWidget>>>,
}

impl BrightnessPlugin {
    pub fn new() -> Self {
        Self {
            store: Rc::new(create_brightness_store()),
            control: Rc::new(RefCell::new(None)),
        }
    }

    /// Discover all controllable displays from available backends.
    async fn discover_displays(&self) -> Vec<Display> {
        let mut displays = Vec::new();

        // Check brightnessctl backend
        if is_brightnessctl_available().await {
            debug!("[brightness] brightnessctl is available");
            match discover_backlight_devices().await {
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
        if is_ddcutil_available().await {
            debug!("[brightness] ddcutil is available");
            match discover_ddc_monitors().await {
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

impl Default for BrightnessPlugin {
    fn default() -> Self {
        Self::new()
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
impl Plugin for BrightnessPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::brightness")
    }

    async fn init(&mut self) -> Result<()> {
        // Discover available displays
        let displays = self.discover_displays().await;

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

        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
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
                    glib::spawn_future_local(async move {
                        if let Err(e) = set_brightness(&display_id_clone, brightness).await {
                            error!(
                                "[brightness] Failed to set brightness for {}: {}",
                                display_id_clone, e
                            );
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
