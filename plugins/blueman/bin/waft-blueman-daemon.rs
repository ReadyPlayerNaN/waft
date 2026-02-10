//! Bluetooth daemon — adapter power toggle with paired device management.
//!
//! Connects to the system bus and monitors BlueZ for adapter/device state changes.
//! Provides one expandable FeatureToggle per adapter with device MenuRows.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin_sdk::*;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

const BLUEZ_DEST: &str = "org.bluez";
const IFACE_ADAPTER1: &str = "org.bluez.Adapter1";
const IFACE_DEVICE1: &str = "org.bluez.Device1";
const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

// ---------------------------------------------------------------------------
// D-Bus property helpers
// ---------------------------------------------------------------------------

fn extract_prop<T: TryFrom<OwnedValue>>(
    props: &HashMap<String, OwnedValue>,
    key: &str,
    default: T,
) -> T {
    props
        .get(key)
        .and_then(|v| T::try_from(v.clone()).ok())
        .unwrap_or(default)
}

fn extract_prop_or(
    props: &HashMap<String, OwnedValue>,
    keys: &[&str],
    default: String,
) -> String {
    for key in keys {
        if let Some(v) = props.get(*key) {
            if let Ok(s) = String::try_from(v.clone()) {
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    default
}

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DeviceState {
    path: String,
    name: String,
    icon: String,
    connected: bool,
    connecting: bool,
}

#[derive(Debug, Clone)]
struct AdapterState {
    path: String,
    name: String,
    powered: bool,
    busy: bool,
    devices: Vec<DeviceState>,
}

#[derive(Debug, Clone, Default)]
struct State {
    adapters: Vec<AdapterState>,
}

// ---------------------------------------------------------------------------
// D-Bus operations
// ---------------------------------------------------------------------------

type ManagedObjects =
    HashMap<zbus::zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

async fn get_managed_objects(conn: &Connection) -> Result<ManagedObjects> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, "/", IFACE_OBJECT_MANAGER)
        .await
        .context("Failed to create ObjectManager proxy")?;

    let (objects,): (ManagedObjects,) = proxy
        .call("GetManagedObjects", &())
        .await
        .context("Failed to call GetManagedObjects")?;

    Ok(objects)
}

async fn load_state(conn: &Connection) -> Result<State> {
    let objects = get_managed_objects(conn).await?;

    let mut adapters: Vec<AdapterState> = Vec::new();

    // First pass: find all adapters
    for (path, interfaces) in &objects {
        if let Some(adapter_props) = interfaces.get(IFACE_ADAPTER1) {
            let name = extract_prop_or(adapter_props, &["Alias", "Name"], "Bluetooth".to_string());
            let powered = extract_prop(adapter_props, "Powered", false);

            adapters.push(AdapterState {
                path: path.to_string(),
                name,
                powered,
                busy: false,
                devices: Vec::new(),
            });
        }
    }

    // Sort adapters by path for stable ordering
    adapters.sort_by(|a, b| a.path.cmp(&b.path));

    // Second pass: find paired devices for each adapter
    for adapter in &mut adapters {
        let mut devices: Vec<DeviceState> = Vec::new();

        for (path, interfaces) in &objects {
            let path_str = path.to_string();

            if !path_str.starts_with(&adapter.path) {
                continue;
            }

            if let Some(device_props) = interfaces.get(IFACE_DEVICE1) {
                let paired = extract_prop(device_props, "Paired", false);
                if !paired {
                    continue;
                }

                let name = extract_prop_or(
                    device_props,
                    &["Alias", "Name"],
                    "Unknown Device".to_string(),
                );
                let icon = extract_prop(
                    device_props,
                    "Icon",
                    "bluetooth-symbolic".to_string(),
                );
                let connected = extract_prop(device_props, "Connected", false);

                devices.push(DeviceState {
                    path: path_str,
                    name,
                    icon,
                    connected,
                    connecting: false,
                });
            }
        }

        // Sort devices by name
        devices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        adapter.devices = devices;
    }

    Ok(State { adapters })
}

async fn set_powered(conn: &Connection, adapter_path: &str, powered: bool) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, adapter_path, IFACE_PROPERTIES)
        .await
        .context("Failed to create Properties proxy")?;

    let v = zbus::zvariant::Value::from(powered);
    let _: () = proxy
        .call("Set", &(IFACE_ADAPTER1, "Powered", v))
        .await
        .context("Failed to set Powered property")?;

    Ok(())
}

async fn connect_device(conn: &Connection, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, device_path, IFACE_DEVICE1)
        .await
        .context("Failed to create Device1 proxy")?;

    let _: () = proxy
        .call("Connect", &())
        .await
        .context("Failed to connect to device")?;

    Ok(())
}

async fn disconnect_device(conn: &Connection, device_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, BLUEZ_DEST, device_path, IFACE_DEVICE1)
        .await
        .context("Failed to create Device1 proxy")?;

    let _: () = proxy
        .call("Disconnect", &())
        .await
        .context("Failed to disconnect from device")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct BluemanDaemon {
    conn: Connection,
    state: Arc<StdMutex<State>>,
}

impl BluemanDaemon {
    async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

        let state = match load_state(&conn).await {
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

    fn build_widgets(state: &State) -> Vec<NamedWidget> {
        let mut widgets = Vec::new();

        for adapter in &state.adapters {
            let connected_count = adapter.devices.iter().filter(|d| d.connected).count();

            let details = if connected_count > 0 {
                Some(format!(
                    "{} connected",
                    connected_count
                ))
            } else {
                None
            };

            // Build device rows as expanded content
            let device_rows: Vec<Widget> = adapter
                .devices
                .iter()
                .map(|device| {
                    let sublabel = if device.connecting {
                        Some("Connecting...".to_string())
                    } else if device.connected {
                        Some("Connected".to_string())
                    } else {
                        None
                    };

                    let trailing = if device.connecting {
                        Some(Widget::Spinner { spinning: true })
                    } else {
                        Some(
                            SwitchBuilder::new()
                                .active(device.connected)
                                .on_toggle(format!("toggle_device:{}", device.path))
                                .build(),
                        )
                    };

                    MenuRowBuilder::new(&device.name)
                        .icon(&device.icon)
                        .sublabel(sublabel.unwrap_or_default())
                        .trailing(trailing.unwrap())
                        .on_click(format!("toggle_device:{}", device.path))
                        .build()
                })
                .collect();

            let expanded_content = if !device_rows.is_empty() {
                Some(
                    ContainerBuilder::new(Orientation::Vertical)
                        .spacing(4)
                        .children(device_rows)
                        .build(),
                )
            } else {
                None
            };

            let mut toggle = FeatureToggleBuilder::new(&adapter.name)
                .icon("bluetooth-symbolic")
                .active(adapter.powered)
                .busy(adapter.busy)
                .on_toggle(format!("toggle_adapter:{}", adapter.path));

            if let Some(d) = &details {
                toggle = toggle.details(d);
            }

            if let Some(content) = expanded_content {
                toggle = toggle.expanded_content(content);
            } else {
                toggle = toggle.expandable(true);
            }

            widgets.push(NamedWidget {
                id: format!("bluetooth:{}", adapter.path),
                weight: 100,
                widget: toggle.build(),
            });
        }

        widgets
    }
}

#[async_trait::async_trait]
impl PluginDaemon for BluemanDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = self.state.lock().unwrap();
        Self::build_widgets(&state)
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
                let mut state = self.state.lock().unwrap();
                if let Some(adapter) = state.adapters.iter_mut().find(|a| a.path == adapter_path) {
                    adapter.busy = true;
                }
            }

            // Toggle powered state
            let current_powered = {
                let state = self.state.lock().unwrap();
                state
                    .adapters
                    .iter()
                    .find(|a| a.path == adapter_path)
                    .map(|a| a.powered)
                    .unwrap_or(false)
            };

            let new_powered = !current_powered;
            if let Err(e) = set_powered(&self.conn, &adapter_path, new_powered).await {
                error!("[bluetooth] Failed to set powered: {}", e);
                let mut state = self.state.lock().unwrap();
                if let Some(adapter) =
                    state.adapters.iter_mut().find(|a| a.path == adapter_path)
                {
                    adapter.busy = false;
                }
                return Err(e.into());
            }

            // Update state (signal monitoring will also catch this, but be optimistic)
            {
                let mut state = self.state.lock().unwrap();
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
                let state = self.state.lock().unwrap();
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
                let mut state = self.state.lock().unwrap();
                for adapter in &mut state.adapters {
                    if let Some(device) =
                        adapter.devices.iter_mut().find(|d| d.path == device_path)
                    {
                        device.connecting = true;
                    }
                }
            }

            let result = if currently_connected {
                disconnect_device(&self.conn, &device_path).await
            } else {
                connect_device(&self.conn, &device_path).await
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
                let mut state = self.state.lock().unwrap();
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
                let mut state = self.state.lock().unwrap();
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

// ---------------------------------------------------------------------------
// Signal monitoring
// ---------------------------------------------------------------------------

async fn monitor_bluez_signals(
    conn: Connection,
    state: Arc<StdMutex<State>>,
    notifier: WidgetNotifier,
) -> Result<()> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BLUEZ_DEST)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add match rule")?;

    info!("[bluetooth] Listening for BlueZ PropertiesChanged signals");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("[bluetooth] D-Bus stream error: {}", e);
                continue;
            }
        };

        let header = msg.header();
        if header.member().map(|m| m.as_str()) != Some("PropertiesChanged")
            || header.interface().map(|i| i.as_str())
                != Some("org.freedesktop.DBus.Properties")
        {
            continue;
        }

        let obj_path = match header.path() {
            Some(p) => p.to_string(),
            None => continue,
        };

        let Ok((iface, props, _invalidated)) = msg.body().deserialize::<(
            String,
            HashMap<String, OwnedValue>,
            Vec<String>,
        )>() else {
            continue;
        };

        let mut changed = false;

        if iface == IFACE_ADAPTER1 {
            if let Some(powered_val) = props.get("Powered") {
                if let Ok(powered) = <bool>::try_from(powered_val.clone()) {
                    let mut st = state.lock().unwrap();
                    if let Some(adapter) =
                        st.adapters.iter_mut().find(|a| a.path == obj_path)
                    {
                        if adapter.powered != powered {
                            info!(
                                "[bluetooth] Adapter {} powered: {}",
                                obj_path, powered
                            );
                            adapter.powered = powered;
                            adapter.busy = false;
                            changed = true;
                        }
                    }
                }
            }
        } else if iface == IFACE_DEVICE1 {
            if let Some(connected_val) = props.get("Connected") {
                if let Ok(connected) = <bool>::try_from(connected_val.clone()) {
                    let mut st = state.lock().unwrap();
                    for adapter in &mut st.adapters {
                        if let Some(device) =
                            adapter.devices.iter_mut().find(|d| d.path == obj_path)
                        {
                            if device.connected != connected || device.connecting {
                                info!(
                                    "[bluetooth] Device {} connected: {}",
                                    obj_path, connected
                                );
                                device.connected = connected;
                                device.connecting = false;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }

        if changed {
            notifier.notify();
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

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
