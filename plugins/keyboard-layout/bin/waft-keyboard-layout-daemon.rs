//! Keyboard layout daemon - displays current layout and allows switching.
//!
//! This daemon provides a keyboard layout indicator that shows the current layout
//! abbreviation (e.g., "US", "CZ") and cycles through available layouts when clicked.
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
use waft_plugin_keyboard_layout::backends::{
    detect_backend, KeyboardLayoutBackend, LayoutEvent,
};
use waft_plugin_sdk::*;
use zbus::Connection;

/// Keyboard layout daemon state.
struct KeyboardLayoutDaemon {
    backend: Arc<dyn KeyboardLayoutBackend>,
    /// Current layout and available layouts, shared with the event monitor task.
    /// Updated by both handle_action (user switches) and the external event monitor.
    layout_state: Arc<StdMutex<LayoutState>>,
}

/// Shared layout state.
struct LayoutState {
    current: String,
    available: Vec<String>,
}

impl KeyboardLayoutDaemon {
    async fn new() -> Result<Self> {
        // Connect to system bus for localed fallback
        let system_conn = Connection::system().await.ok();

        // Detect backend
        let backend = detect_backend(system_conn)
            .await
            .context("No keyboard layout backend available")?;

        log::info!(
            "Using {} backend for keyboard layout",
            backend.name()
        );

        // Query initial layout
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

    fn build_widget(&self) -> Widget {
        let state = self.layout_state.lock().unwrap();
        StatusCycleButtonBuilder::new("cycle_layout")
            .icon("input-keyboard-symbolic")
            .value(&state.current)
            .options(
                state
                    .available
                    .iter()
                    .map(|l| StatusOption {
                        id: l.clone(),
                        label: l.clone(),
                    })
                    .collect(),
            )
            .build()
    }
}

#[async_trait::async_trait]
impl PluginDaemon for KeyboardLayoutDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![NamedWidget {
            id: "keyboard-layout:indicator".to_string(),
            weight: 10,
            widget: self.build_widget(),
        }]
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.id.as_str() {
            "cycle_layout" => {
                // StatusCycleButton sends the next layout ID via ActionParams::String
                if let ActionParams::String(ref target) = action.params {
                    log::debug!("Switching to layout: {}", target);
                } else {
                    log::debug!("Cycling to next keyboard layout");
                }
                self.backend.switch_next().await?;

                // Query new layout and update shared state
                let info = self.backend.get_layout_info().await?;
                let mut state = self.layout_state.lock().unwrap();
                state.current = info.current.clone();
                state.available = info.available;
                log::info!("Switched to layout: {}", info.current);
            }
            other => {
                log::warn!("Unknown action: {}", other);
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    waft_plugin_sdk::init_daemon_logger("info");

    log::info!("Starting keyboard layout daemon...");

    // Create daemon
    let daemon = KeyboardLayoutDaemon::new().await?;

    // Grab shared handles before daemon is moved into the server
    let shared_state = daemon.shared_state();

    // Set up event subscription for layout changes from external sources
    let (event_tx, event_rx) = flume::unbounded::<LayoutEvent>();
    daemon.backend.subscribe(event_tx);

    // Create server and notifier
    let (server, notifier) = PluginServer::new("keyboard-layout-daemon", daemon);

    // Monitor for external layout changes (e.g., keyboard shortcuts, compositor events)
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv_async().await {
            match event {
                LayoutEvent::Changed(info) => {
                    log::info!("External layout change detected: {}", info.current);
                    let mut state = shared_state.lock().unwrap();
                    state.current = info.current;
                    state.available = info.available;
                    drop(state);
                    notifier.notify();
                }
                LayoutEvent::Error(e) => {
                    log::warn!("Backend subscription error: {}", e);
                }
            }
        }
        log::warn!("Layout event receiver closed");
    });

    // Run server
    server.run().await?;

    Ok(())
}
