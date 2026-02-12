//! Bluetooth daemon -- adapter power toggle with paired device management.
//!
//! Connects to the system bus and monitors BlueZ for adapter/device state changes.
//! Exposes BluetoothAdapter entities (one per adapter) and BluetoothDevice entities
//! (nested under their adapter, one per paired device).
//!
//! Entity types:
//! - `bluetooth-adapter` with actions: `toggle-power`
//! - `bluetooth-device` (nested under adapter) with actions: `toggle-connect`
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "blueman"
//! ```

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;
use waft_plugin_bluetooth::dbus;
use waft_plugin_bluetooth::signal_monitor::monitor_bluez_signals;
use waft_plugin_bluetooth::state::State;
use waft_protocol::entity::bluetooth::{BluetoothAdapter, BluetoothDevice};
use zbus::Connection;

/// Extract a stable adapter ID from the D-Bus object path.
///
/// e.g. `/org/bluez/hci0` -> `hci0`
fn adapter_id(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Extract a stable device ID from the D-Bus object path.
///
/// e.g. `/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF` -> `AA:BB:CC:DD:EE:FF`
fn device_id(path: &str) -> String {
    let segment = path.rsplit('/').next().unwrap_or(path);
    segment
        .strip_prefix("dev_")
        .unwrap_or(segment)
        .replace('_', ":")
}

struct BluemanPlugin {
    conn: Connection,
    state: Arc<StdMutex<State>>,
}

impl BluemanPlugin {
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

    fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[bluetooth] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
    }
}

#[async_trait::async_trait]
impl Plugin for BluemanPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        let mut entities = Vec::new();

        for adapter in &state.adapters {
            let aid = adapter_id(&adapter.path);
            let adapter_urn = Urn::new(
                "blueman",
                BluetoothAdapter::ENTITY_TYPE,
                aid,
            );

            // Adapter entity
            let adapter_entity = BluetoothAdapter {
                name: adapter.name.clone(),
                powered: adapter.powered,
            };
            entities.push(Entity::new(
                adapter_urn.clone(),
                BluetoothAdapter::ENTITY_TYPE,
                &adapter_entity,
            ));

            // Device entities (nested under adapter)
            for device in &adapter.devices {
                let did = device_id(&device.path);
                let device_urn = adapter_urn.child(BluetoothDevice::ENTITY_TYPE, &did);

                let device_entity = BluetoothDevice {
                    name: device.name.clone(),
                    device_type: device.icon.clone(),
                    connected: device.connected,
                    battery_percentage: device.battery_percentage,
                };
                entities.push(Entity::new(
                    device_urn,
                    BluetoothDevice::ENTITY_TYPE,
                    &device_entity,
                ));
            }
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let entity_type = urn.entity_type();

        if entity_type == BluetoothAdapter::ENTITY_TYPE {
            if action == "toggle-power" {
                let aid = urn.id().to_string();
                debug!("[bluetooth] Toggle adapter power: {}", aid);

                let adapter_path = {
                    let state = self.lock_state();
                    state
                        .adapters
                        .iter()
                        .find(|a| adapter_id(&a.path) == aid)
                        .map(|a| a.path.clone())
                };

                let adapter_path = match adapter_path {
                    Some(p) => p,
                    None => {
                        warn!("[bluetooth] Adapter not found: {}", aid);
                        return Ok(());
                    }
                };

                let current_powered = {
                    let state = self.lock_state();
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
                    return Err(e.into());
                }

                // Optimistic update (signal monitoring will also catch this)
                {
                    let mut state = self.lock_state();
                    if let Some(adapter) =
                        state.adapters.iter_mut().find(|a| a.path == adapter_path)
                    {
                        adapter.powered = new_powered;
                    }
                }
            } else {
                debug!("[bluetooth] Unknown adapter action: {}", action);
            }
        } else if entity_type == BluetoothDevice::ENTITY_TYPE {
            if action == "toggle-connect" {
                let did = urn.id().to_string();
                debug!("[bluetooth] Toggle device connection: {}", did);

                // Find the device path from the MAC-address-based ID
                let (device_path, currently_connected) = {
                    let state = self.lock_state();
                    let mut found = None;
                    for adapter in &state.adapters {
                        for device in &adapter.devices {
                            if device_id(&device.path) == did {
                                found = Some((device.path.clone(), device.connected));
                                break;
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                    match found {
                        Some(f) => f,
                        None => {
                            warn!("[bluetooth] Device not found: {}", did);
                            return Ok(());
                        }
                    }
                };

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
                    return Err(e.into());
                }

                // Optimistic update (signal will confirm)
                {
                    let mut state = self.lock_state();
                    for adapter in &mut state.adapters {
                        if let Some(device) =
                            adapter.devices.iter_mut().find(|d| d.path == device_path)
                        {
                            device.connected = !currently_connected;
                        }
                    }
                }
            } else {
                debug!("[bluetooth] Unknown device action: {}", action);
            }
        } else {
            debug!(
                "[bluetooth] Unknown entity type: {} (action: {})",
                entity_type, action
            );
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides(&[
        BluetoothAdapter::ENTITY_TYPE,
        BluetoothDevice::ENTITY_TYPE,
    ]) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    info!("Starting blueman plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = BluemanPlugin::new().await?;

        let shared_state = plugin.shared_state();
        let monitor_conn = plugin.conn.clone();

        let (runtime, notifier) = PluginRuntime::new("blueman", plugin);

        // Monitor BlueZ D-Bus signals
        tokio::spawn(async move {
            if let Err(e) = monitor_bluez_signals(monitor_conn, shared_state, notifier).await {
                error!("[bluetooth] D-Bus signal monitoring failed: {}", e);
            }
        });

        runtime.run().await?;
        Ok(())
    })
}
