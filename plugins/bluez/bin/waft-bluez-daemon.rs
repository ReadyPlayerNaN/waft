//! Bluetooth daemon -- adapter and device management with full settings support.
//!
//! Connects to the system bus and monitors BlueZ for adapter/device state changes.
//! Exposes BluetoothAdapter entities (one per adapter) and BluetoothDevice entities
//! (nested under their adapter).
//!
//! Entity types:
//! - `bluetooth-adapter` with actions: `toggle-power`, `toggle-discoverable`,
//!   `set-alias`, `start-discovery`, `stop-discovery`
//! - `bluetooth-device` (nested under adapter) with actions: `toggle-connect`,
//!   `pair-device`, `remove-device`
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "bluez"
//! ```

use std::sync::LazyLock;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;
use waft_plugin_bluetooth::dbus;
use waft_plugin_bluetooth::signal_monitor::monitor_bluez_signals;
use waft_plugin_bluetooth::state::State;
use waft_protocol::entity::bluetooth::{BluetoothAdapter, BluetoothDevice, ConnectionState};
use zbus::Connection;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/bluez.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/bluez.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

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

struct BluezPlugin {
    conn: Connection,
    state: Arc<StdMutex<State>>,
    notifier: EntityNotifier,
}

impl BluezPlugin {
    async fn new(notifier: EntityNotifier) -> Result<Self> {
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
                        "[bluetooth] Adapter: {} at {} (powered: {}, {} devices)",
                        adapter.name,
                        adapter.path,
                        adapter.powered,
                        adapter.devices.len()
                    );
                }
                s
            }
            Err(e) => {
                warn!("[bluetooth] Failed to load initial state: {e}");
                State::default()
            }
        };

        Ok(Self {
            conn,
            state: Arc::new(StdMutex::new(state)),
            notifier,
        })
    }

    fn shared_state(&self) -> Arc<StdMutex<State>> {
        self.state.clone()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock_or_recover()
    }

    /// Push entity updates to the overview via the notifier.
    fn notify(&self) {
        self.notifier.notify();
    }

    /// Find the adapter D-Bus path for a given adapter ID.
    fn find_adapter_path(&self, aid: &str) -> Option<String> {
        let state = self.lock_state();
        state
            .adapters
            .iter()
            .find(|a| adapter_id(&a.path) == aid)
            .map(|a| a.path.clone())
    }

    /// Find the device D-Bus path and its parent adapter path for a given device ID.
    fn find_device_paths(&self, did: &str) -> Option<(String, String)> {
        let state = self.lock_state();
        for adapter in &state.adapters {
            for device in &adapter.devices {
                if device_id(&device.path) == did {
                    return Some((device.path.clone(), adapter.path.clone()));
                }
            }
        }
        None
    }
}

#[async_trait::async_trait]
impl Plugin for BluezPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        let mut entities = Vec::new();

        for adapter in &state.adapters {
            let aid = adapter_id(&adapter.path);
            let adapter_urn = Urn::new("bluez", BluetoothAdapter::ENTITY_TYPE, aid);

            // Adapter entity
            let adapter_entity = BluetoothAdapter {
                name: adapter.name.clone(),
                powered: adapter.powered,
                discoverable: adapter.discoverable,
                discovering: adapter.discovering,
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
                    connection_state: device.connection_state,
                    battery_percentage: device.battery_percentage,
                    paired: device.paired,
                    trusted: device.trusted,
                    rssi: device.rssi,
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
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let entity_type = urn.entity_type();

        if entity_type == BluetoothAdapter::ENTITY_TYPE {
            let aid = urn.id().to_string();

            match action.as_str() {
                "toggle-power" => {
                    debug!("[bluetooth] Toggle adapter power: {aid}");

                    let Some(adapter_path) = self.find_adapter_path(&aid) else {
                        warn!("[bluetooth] Adapter not found: {aid}");
                        return Ok(serde_json::Value::Null);
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
                    if let Err(e) = dbus::set_powered(&self.conn, &adapter_path, new_powered).await
                    {
                        error!("[bluetooth] Failed to set powered: {e}");
                        return Err(e);
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
                }

                "toggle-discoverable" => {
                    debug!("[bluetooth] Toggle adapter discoverable: {aid}");

                    let Some(adapter_path) = self.find_adapter_path(&aid) else {
                        warn!("[bluetooth] Adapter not found: {aid}");
                        return Ok(serde_json::Value::Null);
                    };

                    let current_discoverable = {
                        let state = self.lock_state();
                        state
                            .adapters
                            .iter()
                            .find(|a| a.path == adapter_path)
                            .map(|a| a.discoverable)
                            .unwrap_or(false)
                    };

                    let new_discoverable = !current_discoverable;
                    if let Err(e) =
                        dbus::set_discoverable(&self.conn, &adapter_path, new_discoverable).await
                    {
                        error!("[bluetooth] Failed to set discoverable: {e}");
                        return Err(e);
                    }

                    // Optimistic update
                    {
                        let mut state = self.lock_state();
                        if let Some(adapter) =
                            state.adapters.iter_mut().find(|a| a.path == adapter_path)
                        {
                            adapter.discoverable = new_discoverable;
                        }
                    }
                }

                "set-alias" => {
                    let Some(alias) = params["alias"].as_str().map(str::to_string) else {
                        warn!("[bluetooth] set-alias action missing 'alias' param");
                        return Ok(serde_json::Value::Null);
                    };
                    debug!("[bluetooth] Set adapter alias: {aid} -> {alias}");

                    let Some(adapter_path) = self.find_adapter_path(&aid) else {
                        warn!("[bluetooth] Adapter not found: {aid}");
                        return Ok(serde_json::Value::Null);
                    };

                    if let Err(e) = dbus::set_adapter_alias(&self.conn, &adapter_path, &alias).await
                    {
                        error!("[bluetooth] Failed to set alias: {e}");
                        return Err(e);
                    }

                    // Optimistic update
                    {
                        let mut state = self.lock_state();
                        if let Some(adapter) =
                            state.adapters.iter_mut().find(|a| a.path == adapter_path)
                        {
                            adapter.name = alias;
                        }
                    }
                }

                "start-discovery" => {
                    debug!("[bluetooth] Start discovery: {aid}");

                    let Some(adapter_path) = self.find_adapter_path(&aid) else {
                        warn!("[bluetooth] Adapter not found: {aid}");
                        return Ok(serde_json::Value::Null);
                    };

                    if let Err(e) = dbus::start_discovery(&self.conn, &adapter_path).await {
                        error!("[bluetooth] Failed to start discovery: {e}");
                        return Err(e);
                    }

                    // Optimistic update
                    {
                        let mut state = self.lock_state();
                        if let Some(adapter) =
                            state.adapters.iter_mut().find(|a| a.path == adapter_path)
                        {
                            adapter.discovering = true;
                        }
                    }
                }

                "stop-discovery" => {
                    debug!("[bluetooth] Stop discovery: {aid}");

                    let Some(adapter_path) = self.find_adapter_path(&aid) else {
                        warn!("[bluetooth] Adapter not found: {aid}");
                        return Ok(serde_json::Value::Null);
                    };

                    if let Err(e) = dbus::stop_discovery(&self.conn, &adapter_path).await {
                        error!("[bluetooth] Failed to stop discovery: {e}");
                        return Err(e);
                    }

                    // Optimistic update
                    {
                        let mut state = self.lock_state();
                        if let Some(adapter) =
                            state.adapters.iter_mut().find(|a| a.path == adapter_path)
                        {
                            adapter.discovering = false;
                        }
                    }
                }

                _ => {
                    debug!("[bluetooth] Unknown adapter action: {action}");
                }
            }
        } else if entity_type == BluetoothDevice::ENTITY_TYPE {
            let did = urn.id().to_string();

            match action.as_str() {
                "toggle-connect" => {
                    debug!("[bluetooth] Toggle device connection: {did}");

                    // Find the device path and current connection state
                    let (device_path, current_state) = {
                        let state = self.lock_state();
                        let mut found = None;
                        for adapter in &state.adapters {
                            for device in &adapter.devices {
                                if device_id(&device.path) == did {
                                    found = Some((device.path.clone(), device.connection_state));
                                    break;
                                }
                            }
                            if found.is_some() {
                                break;
                            }
                        }
                        let Some(f) = found else {
                            warn!("[bluetooth] Device not found: {did}");
                            return Ok(serde_json::Value::Null);
                        };
                        f
                    };

                    // Set intermediate state
                    let intermediate_state = match current_state {
                        ConnectionState::Connected => ConnectionState::Disconnecting,
                        _ => ConnectionState::Connecting,
                    };
                    {
                        let mut state = self.lock_state();
                        for adapter in &mut state.adapters {
                            if let Some(device) =
                                adapter.devices.iter_mut().find(|d| d.path == device_path)
                            {
                                device.connection_state = intermediate_state;
                            }
                        }
                    }
                    self.notify(); // Push intermediate state to UI

                    // Perform the D-Bus operation
                    let result = match current_state {
                        ConnectionState::Connected => {
                            dbus::disconnect_device(&self.conn, &device_path).await
                        }
                        _ => dbus::connect_device(&self.conn, &device_path).await,
                    };

                    if let Err(e) = result {
                        error!(
                            "[bluetooth] Failed to {} device: {}",
                            if current_state == ConnectionState::Connected {
                                "disconnect"
                            } else {
                                "connect"
                            },
                            e
                        );
                        // Revert to previous state on failure
                        {
                            let mut state = self.lock_state();
                            for adapter in &mut state.adapters {
                                if let Some(device) =
                                    adapter.devices.iter_mut().find(|d| d.path == device_path)
                                {
                                    device.connection_state = current_state;
                                }
                            }
                        }
                        self.notify(); // Push reverted state to UI
                        return Err(e);
                    }
                    // On success: signal monitor will catch the Connected property change
                    // and update the state to Connected/Disconnected
                }

                "pair-device" => {
                    debug!("[bluetooth] Pair device: {did}");

                    let Some((device_path, _)) = self.find_device_paths(&did) else {
                        warn!("[bluetooth] Device not found: {did}");
                        return Ok(serde_json::Value::Null);
                    };

                    if let Err(e) = dbus::pair_device(&self.conn, &device_path).await {
                        error!("[bluetooth] Failed to pair device: {e}");
                        return Err(e);
                    }
                    // Signal monitor will catch the Paired property change
                }

                "remove-device" => {
                    debug!("[bluetooth] Remove device: {did}");

                    let Some((device_path, adapter_path)) = self.find_device_paths(&did) else {
                        warn!("[bluetooth] Device not found: {did}");
                        return Ok(serde_json::Value::Null);
                    };

                    if let Err(e) =
                        dbus::remove_device(&self.conn, &adapter_path, &device_path).await
                    {
                        error!("[bluetooth] Failed to remove device: {e}");
                        return Err(e);
                    }

                    // Remove device from state
                    {
                        let mut state = self.lock_state();
                        for adapter in &mut state.adapters {
                            adapter.devices.retain(|d| d.path != device_path);
                        }
                    }
                    self.notify();
                }

                _ => {
                    debug!("[bluetooth] Unknown device action: {action}");
                }
            }
        } else {
            debug!(
                "[bluetooth] Unknown entity type: {entity_type} (action: {action})"
            );
        }

        Ok(serde_json::Value::Null)
    }
}

fn main() -> Result<()> {
    PluginRunner::new(
        "bluez",
        &[BluetoothAdapter::ENTITY_TYPE, BluetoothDevice::ENTITY_TYPE],
    )
    .i18n(i18n(), "plugin-name", "plugin-description")
    .run(|notifier| async move {
        let plugin = BluezPlugin::new(notifier.clone()).await?;

        let shared_state = plugin.shared_state();
        let monitor_conn = plugin.conn.clone();

        // Monitor BlueZ D-Bus signals
        spawn_monitored("bluetooth/signal-monitor", async move {
            monitor_bluez_signals(monitor_conn, shared_state, notifier).await
        });

        Ok(plugin)
    })
}
