//! Keyboard layout plugin - provides quick access to view and switch keyboard layouts.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.
//! This plugin adds a keyboard layout indicator button to the main overlay header
//! that displays the current layout (e.g., "US", "DE") and cycles through available
//! layouts when clicked.
//!
//! ## Multi-Backend Support
//!
//! The plugin automatically detects the appropriate backend for keyboard layout control:
//!
//! 1. **Niri** - For Niri compositor users (detected via `NIRI_SOCKET`)
//! 2. **Sway** - For Sway users (detected via `SWAYSOCK`)
//! 3. **Hyprland** - For Hyprland users (detected via `HYPRLAND_INSTANCE_SIGNATURE`)
//! 4. **systemd-localed** - Fallback via D-Bus for other systems
//!
//! If no backend is available, the widget will display a fallback indicator ("??")
//! and the plugin will not crash the application.

mod backends;
mod widget;

use anyhow::Result;
use async_trait::async_trait;
use flume::unbounded;
use gtk::prelude::*;
use log::{info, warn};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Mutex;

use waft_core::dbus::DbusHandle;
use waft_core::menu_state::MenuStore;
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, Slot, Widget, WidgetRegistrar};

use backends::{detect_backend, KeyboardLayoutBackend, LayoutEvent};
use widget::KeyboardLayoutWidget;

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::keyboard-layout", "Keyboard Layout", "0.1.0");
waft_plugin_api::export_overview_plugin!(KeyboardLayoutPlugin::new());

/// Keyboard layout plugin for viewing and switching keyboard layouts.
///
/// This plugin provides a button in the overlay header that:
/// - Displays the current keyboard layout (e.g., "US", "DE", "FR")
/// - Cycles through available layouts when clicked
/// - Automatically updates when layout changes externally
/// - Works across different Wayland compositors (Niri, Sway, Hyprland) and systemd-localed
///
/// ## Backend Detection
///
/// The plugin auto-detects the appropriate backend based on environment variables:
/// - `NIRI_SOCKET` → Niri backend
/// - `SWAYSOCK` → Sway backend
/// - `HYPRLAND_INSTANCE_SIGNATURE` → Hyprland backend
/// - D-Bus locale1 available → systemd-localed backend
pub struct KeyboardLayoutPlugin {
    backend: Arc<Mutex<Option<Arc<dyn KeyboardLayoutBackend>>>>,
    dbus_handle: Option<Arc<DbusHandle>>,
    tokio_handle: Option<tokio::runtime::Handle>,
}

impl Default for KeyboardLayoutPlugin {
    fn default() -> Self {
        Self {
            backend: Arc::new(Mutex::new(None)),
            dbus_handle: None,
            tokio_handle: None,
        }
    }
}

impl KeyboardLayoutPlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for KeyboardLayoutPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::keyboard-layout")
    }

    async fn init(&mut self, resources: &PluginResources) -> Result<()> {
        let tokio_handle = resources
            .tokio_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?;
        let _guard = tokio_handle.enter();

        self.tokio_handle = Some(tokio_handle.clone());

        // Use the system dbus connection provided by the host (for localed fallback)
        let dbus = resources.system_dbus.clone();
        self.dbus_handle = dbus.clone();

        // Detect and initialize the appropriate backend
        let backend = detect_backend(dbus).await;

        if let Some(ref b) = backend {
            info!("[keyboard-layout] Using {} backend", b.name());
        } else {
            warn!(
                "[keyboard-layout] No backend available, plugin will show fallback indicator"
            );
        }

        *self.backend.lock().await = backend;
        Ok(())
    }

    async fn create_elements(
        &mut self,
        app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let _guard = self.tokio_handle.as_ref().map(|h| h.enter());

        // Check if backend is available
        let backend_option = self.backend.lock().await;
        let backend = match backend_option.as_ref() {
            Some(b) => b.clone(),
            None => {
                warn!("[keyboard-layout] Skipping widget creation, no backend available");
                return Ok(());
            }
        };
        drop(backend_option);

        // Create event channel for layout change notifications
        let (event_tx, event_rx) = unbounded::<LayoutEvent>();

        // Start the backend subscription
        backend.subscribe(event_tx);

        // Create keyboard layout widget
        let tokio_handle = self.tokio_handle.as_ref()
            .ok_or_else(|| anyhow::anyhow!("tokio_handle not provided"))?
            .clone();
        let keyboard_widget = KeyboardLayoutWidget::new(self.backend.clone(), app.clone(), tokio_handle);

        // Clone references for the event handler
        let label = keyboard_widget.label.clone();
        let root = keyboard_widget.root.clone();

        // Handle layout change events from backend
        info!("[keyboard-layout] Starting event receiver loop");
        glib::spawn_future_local(async move {
            info!("[keyboard-layout] Event receiver future started");
            while let Ok(event) = event_rx.recv_async().await {
                match event {
                    LayoutEvent::Changed(info) => {
                        info!(
                            "[keyboard-layout] Received layout change event: {}",
                            info.current
                        );
                        label.set_label(&info.current);
                        root.update_property(&[gtk::accessible::Property::Description(
                            &format!("Current layout: {}", info.current),
                        )]);
                    }
                    LayoutEvent::Error(e) => {
                        warn!("[keyboard-layout] Subscription error: {}", e);
                    }
                }
            }
            warn!("[keyboard-layout] Event receiver closed unexpectedly");
        });

        // Register widget in actions slot with weight 10 (first in actions group)
        registrar.register_widget(Rc::new(Widget {
            id: "keyboard-layout:indicator".to_string(),
            slot: Slot::Actions,
            el: keyboard_widget.root.clone().into(),
            weight: 10,
        }));

        info!("[keyboard-layout] Widget registered in actions slot");
        Ok(())
    }
}
