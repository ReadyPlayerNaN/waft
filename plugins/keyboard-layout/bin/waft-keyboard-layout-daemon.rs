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

use std::sync::LazyLock;

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;
use waft_plugin_keyboard_layout::backends::{KeyboardLayoutBackend, LayoutEvent, detect_backend};
use zbus::Connection;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/keyboard-layout.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/keyboard-layout.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

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
}

#[async_trait::async_trait]
impl Plugin for KeyboardLayoutPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.layout_state.lock_or_recover();
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
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "cycle" => {
                log::debug!("Cycling to next keyboard layout");
                self.backend.switch_next().await?;

                // Query new layout and update shared state
                let info = self.backend.get_layout_info().await?;
                let mut state = self.layout_state.lock_or_recover();
                state.current = info.current.clone();
                state.available = info.available;
                log::info!("Switched to layout: {}", info.current);
            }
            other => {
                log::warn!("Unknown action: {other}");
            }
        }
        Ok(serde_json::Value::Null)
    }
}

fn main() -> Result<()> {
    PluginRunner::new("keyboard-layout", &[entity::keyboard::ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async move {
            let plugin = KeyboardLayoutPlugin::new().await?;

            // Grab shared handles before plugin is moved into the runtime
            let shared_state = plugin.shared_state();

            // Set up event subscription for layout changes from external sources
            let (event_tx, event_rx) = flume::unbounded::<LayoutEvent>();
            plugin.backend.subscribe(event_tx);

            // Monitor for external layout changes (e.g., keyboard shortcuts, compositor events)
            tokio::spawn(async move {
                while let Ok(event) = event_rx.recv_async().await {
                    match event {
                        LayoutEvent::Changed(info) => {
                            log::info!("External layout change detected: {}", info.current);
                            {
                                let mut state = shared_state.lock_or_recover();
                                state.current = info.current;
                                state.available = info.available;
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

            Ok(plugin)
        })
}
