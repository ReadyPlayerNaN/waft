//! Bluetooth daemon -- adapter power toggle with paired device management.
//!
//! Connects to the system bus and monitors BlueZ for adapter/device state changes.
//! Provides one expandable FeatureToggle per adapter with device MenuRows.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin_bluetooth::dbus;
use waft_plugin_bluetooth::signal_monitor::monitor_bluez_signals;
use waft_plugin_bluetooth::state::State;
use waft_plugin_bluetooth::widget_builder;
use waft_plugin_sdk::*;
use zbus::Connection;

struct BluemanDaemon {
    conn: Connection,
    state: Arc<StdMutex<State>>,
}

impl BluemanDaemon {
    async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

        let state = match dbus::load_state(&conn).await {
            Ok(s) => {
                if s.adapters.is_empty() {
                    warn!("[bluetooth] No adapters found");
                }
                for adapter in &s.adapters {
                    info!(
                        "[bluetooth] Adapter: {} at {} (powered: {}, {} paired devices)",
                        adapter.name,
                        adapter.path,
                        adapter.powered,
                        adapter.devices.len()
                    );
                }
                s
            }
            Err(e) => {
                warn!("[bluetooth] Failed to load initial state: {}", e);
                State::default()
            }
        };

        Ok(Self {
            conn,
            state: Arc::new(StdMutex::new(state)),
        })
    }

    fn shared_state(&self) -> Arc<StdMutex<State>> {
        self.state.clone()
    }
}

#[async_trait::async_trait]
impl PluginDaemon for BluemanDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[bluetooth] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };
        widget_builder::build_widgets(&state)
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(adapter_path) = action.id.strip_prefix("toggle_adapter:") {
            let adapter_path = adapter_path.to_string();
            debug!("[bluetooth] Toggle adapter: {}", adapter_path);

            // Set busy
            {
                let mut state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                if let Some(adapter) = state.adapters.iter_mut().find(|a| a.path == adapter_path) {
                    adapter.busy = true;
                }
            }

            // Toggle powered state
            let current_powered = {
                let state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                state
                    .adapters
                    .iter()
                    .find(|a| a.path == adapter_path)
                    .map(|a| a.powered)
                    .unwrap_or(false)
            };

            let new_powered = !current_powered;
            if let Err(e) = dbus::set_powered(&self.conn, &adapter_path, new_powered).await {
                error!("[bluetooth] Failed to set powered: {}", e);
                let mut state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                if let Some(adapter) =
                    state.adapters.iter_mut().find(|a| a.path == adapter_path)
                {
                    adapter.busy = false;
                }
                return Err(e.into());
            }

            // Update state (signal monitoring will also catch this, but be optimistic)
            {
                let mut state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                if let Some(adapter) =
                    state.adapters.iter_mut().find(|a| a.path == adapter_path)
                {
                    adapter.powered = new_powered;
                    adapter.busy = false;
                }
            }
        } else if let Some(device_path) = action.id.strip_prefix("toggle_device:") {
            let device_path = device_path.to_string();
            debug!("[bluetooth] Toggle device: {}", device_path);

            let currently_connected = {
                let state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                state
                    .adapters
                    .iter()
                    .flat_map(|a| a.devices.iter())
                    .find(|d| d.path == device_path)
                    .map(|d| d.connected)
                    .unwrap_or(false)
            };

            // Set connecting state
            {
                let mut state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                for adapter in &mut state.adapters {
                    if let Some(device) =
                        adapter.devices.iter_mut().find(|d| d.path == device_path)
                    {
                        device.connecting = true;
                    }
                }
            }

            let result = if currently_connected {
                dbus::disconnect_device(&self.conn, &device_path).await
            } else {
                dbus::connect_device(&self.conn, &device_path).await
            };

            if let Err(e) = result {
                error!(
                    "[bluetooth] Failed to {} device: {}",
                    if currently_connected {
                        "disconnect"
                    } else {
                        "connect"
                    },
                    e
                );
                // Revert connecting state
                let mut state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                for adapter in &mut state.adapters {
                    if let Some(device) =
                        adapter.devices.iter_mut().find(|d| d.path == device_path)
                    {
                        device.connecting = false;
                    }
                }
                return Err(e.into());
            }

            // Optimistically update connected state (signal will confirm)
            {
                let mut state = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[bluetooth] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                for adapter in &mut state.adapters {
                    if let Some(device) =
                        adapter.devices.iter_mut().find(|d| d.path == device_path)
                    {
                        device.connected = !currently_connected;
                        device.connecting = false;
                    }
                }
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    waft_plugin_sdk::init_daemon_logger("info");

    info!("Starting blueman daemon...");

    let daemon = BluemanDaemon::new().await?;

    let shared_state = daemon.shared_state();
    let monitor_conn = daemon.conn.clone();

    let (server, notifier) = PluginServer::new("blueman-daemon", daemon);

    // Monitor BlueZ D-Bus signals
    tokio::spawn(async move {
        if let Err(e) = monitor_bluez_signals(monitor_conn, shared_state, notifier).await {
            error!("[bluetooth] D-Bus signal monitoring failed: {}", e);
        }
    });

    server.run().await?;

    Ok(())
}
