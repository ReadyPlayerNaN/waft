//! Keyboard layout daemon - displays current layout and allows switching.
//!
//! Provides a `keyboard-layout` entity with the current layout and available
//! alternatives. Supports cycling through layouts via the "cycle" action.
//!
//! ## Multi-Backend Support
//!
//! The daemon automatically detects the appropriate backend:
//! 1. **Niri** - Detected via `NIRI_SOCKET` environment variable
//! 2. **Sway** - Detected via `SWAYSOCK` environment variable
//! 3. **Hyprland** - Detected via `HYPRLAND_INSTANCE_SIGNATURE` environment variable
//! 4. **systemd-localed** - Fallback via D-Bus for other systems

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;
use waft_plugin_keyboard_layout::backends::{KeyboardLayoutBackend, LayoutEvent, detect_backend};
use zbus::Connection;

/// Shared layout state.
struct LayoutState {
    current: String,
    available: Vec<String>,
}

/// Keyboard layout plugin.
struct KeyboardLayoutPlugin {
    backend: Arc<dyn KeyboardLayoutBackend>,
    layout_state: Arc<StdMutex<LayoutState>>,
}

impl KeyboardLayoutPlugin {
    async fn new() -> Result<Self> {
        let system_conn = Connection::system().await.ok();

        let backend = detect_backend(system_conn)
            .await
            .context("No keyboard layout backend available")?;

        log::info!("Using {} backend for keyboard layout", backend.name());

        let info = backend
            .get_layout_info()
            .await
            .context("Failed to query initial layout")?;

        log::info!("Initial keyboard layout: {}", info.current);

        Ok(Self {
            backend,
            layout_state: Arc::new(StdMutex::new(LayoutState {
                current: info.current,
                available: info.available,
            })),
        })
    }

    fn shared_state(&self) -> Arc<StdMutex<LayoutState>> {
        self.layout_state.clone()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, LayoutState> {
        match self.layout_state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[keyboard-layout] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
    }
}

#[async_trait::async_trait]
impl Plugin for KeyboardLayoutPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        let layout = entity::keyboard::KeyboardLayout {
            current: state.current.clone(),
            available: state.available.clone(),
        };
        vec![Entity::new(
            Urn::new("keyboard-layout", entity::keyboard::ENTITY_TYPE, "default"),
            entity::keyboard::ENTITY_TYPE,
            &layout,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "cycle" => {
                log::debug!("Cycling to next keyboard layout");
                self.backend.switch_next().await?;

                // Query new layout and update shared state
                let info = self.backend.get_layout_info().await?;
                let mut state = self.lock_state();
                state.current = info.current.clone();
                state.available = info.available;
                log::info!("Switched to layout: {}", info.current);
            }
            other => {
                log::warn!("Unknown action: {other}");
            }
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[entity::keyboard::ENTITY_TYPE]) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting keyboard layout plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = KeyboardLayoutPlugin::new().await?;

        // Grab shared handles before plugin is moved into the runtime
        let shared_state = plugin.shared_state();

        // Set up event subscription for layout changes from external sources
        let (event_tx, event_rx) = flume::unbounded::<LayoutEvent>();
        plugin.backend.subscribe(event_tx);

        let (runtime, notifier) = PluginRuntime::new("keyboard-layout", plugin);

        // Monitor for external layout changes (e.g., keyboard shortcuts, compositor events)
        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv_async().await {
                match event {
                    LayoutEvent::Changed(info) => {
                        log::info!("External layout change detected: {}", info.current);
                        match shared_state.lock() {
                            Ok(mut state) => {
                                state.current = info.current;
                                state.available = info.available;
                            }
                            Err(e) => {
                                log::warn!("[keyboard-layout] mutex poisoned, recovering: {e}");
                                let mut state = e.into_inner();
                                state.current = info.current;
                                state.available = info.available;
                            }
                        }
                        notifier.notify();
                    }
                    LayoutEvent::Error(e) => {
                        log::warn!("Backend subscription error: {e}");
                    }
                }
            }
            log::warn!("Layout event receiver closed");
        });

        runtime.run().await?;
        Ok(())
    })
}
