//! Battery daemon - displays battery status from UPower.
//!
//! This daemon monitors the UPower DisplayDevice on the system bus and
//! provides a widget showing battery percentage, icon, and time remaining.
//! Updates are pushed to connected clients when D-Bus PropertiesChanged
//! signals arrive (no polling).

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin_sdk::*;
use zbus::Connection;

const UPOWER_DEST: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const IFACE_DEVICE: &str = "org.freedesktop.UPower.Device";

// ---------------------------------------------------------------------------
// Battery state types
// ---------------------------------------------------------------------------

/// UPower device state.
///
/// Maps to the `State` property (u32) on `org.freedesktop.UPower.Device`:
/// 0=Unknown, 1=Charging, 2=Discharging, 3=Empty,
/// 4=FullyCharged, 5=PendingCharge, 6=PendingDischarge.
#[derive(Clone, Debug, Default, PartialEq)]
enum BatteryState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl BatteryState {
    fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::Empty => "Empty",
            Self::FullyCharged => "Fully charged",
            Self::PendingCharge => "Pending charge",
            Self::PendingDischarge => "Pending discharge",
        }
    }
}

/// Current battery information from UPower DisplayDevice.
#[derive(Clone, Debug, Default)]
struct BatteryInfo {
    present: bool,
    percentage: f64,
    state: BatteryState,
    icon_name: String,
    time_to_empty: i64,
    time_to_full: i64,
}

impl BatteryInfo {
    /// Human-readable status text for the sublabel.
    fn status_text(&self) -> String {
        match self.state {
            BatteryState::Discharging if self.time_to_empty > 0 => {
                format!("{} remaining", format_time_remaining(self.time_to_empty))
            }
            BatteryState::Charging if self.time_to_full > 0 => {
                format!("{} to full", format_time_remaining(self.time_to_full))
            }
            _ => self.state.label().to_string(),
        }
    }
}

/// Format seconds into a human-readable duration like `"2h 30min"`.
///
/// Omits hours when 0, shows `"< 1min"` for values under 60 seconds.
fn format_time_remaining(seconds: i64) -> String {
    if seconds <= 0 {
        return "< 1min".to_string();
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours == 0 && minutes == 0 {
        return "< 1min".to_string();
    }

    if hours == 0 {
        return format!("{}min", minutes);
    }

    if minutes == 0 {
        return format!("{}h", hours);
    }

    format!("{}h {}min", hours, minutes)
}

// ---------------------------------------------------------------------------
// D-Bus helpers
// ---------------------------------------------------------------------------

/// Read all battery properties from the UPower DisplayDevice.
async fn get_battery_info(conn: &Connection) -> Result<BatteryInfo> {
    let proxy = zbus::Proxy::new(
        conn,
        UPOWER_DEST,
        DISPLAY_DEVICE_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .context("Failed to create D-Bus proxy")?;

    let (props,): (std::collections::HashMap<String, zbus::zvariant::OwnedValue>,) = proxy
        .call("GetAll", &(IFACE_DEVICE,))
        .await
        .context("Failed to get UPower DisplayDevice properties")?;

    let present = props
        .get("IsPresent")
        .and_then(|v| <bool as TryFrom<_>>::try_from(zbus::zvariant::Value::from(v.clone())).ok())
        .unwrap_or(false);
    let percentage = props
        .get("Percentage")
        .and_then(|v| <f64 as TryFrom<_>>::try_from(zbus::zvariant::Value::from(v.clone())).ok())
        .unwrap_or(0.0);
    let state_u32 = props
        .get("State")
        .and_then(|v| <u32 as TryFrom<_>>::try_from(zbus::zvariant::Value::from(v.clone())).ok())
        .unwrap_or(0);
    let icon_name = props
        .get("IconName")
        .and_then(|v| {
            if let zbus::zvariant::Value::Str(s) =
                zbus::zvariant::Value::from(v.clone())
            {
                Some(s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();
    let time_to_empty = props
        .get("TimeToEmpty")
        .and_then(|v| <i64 as TryFrom<_>>::try_from(zbus::zvariant::Value::from(v.clone())).ok())
        .unwrap_or(0);
    let time_to_full = props
        .get("TimeToFull")
        .and_then(|v| <i64 as TryFrom<_>>::try_from(zbus::zvariant::Value::from(v.clone())).ok())
        .unwrap_or(0);

    Ok(BatteryInfo {
        present,
        percentage,
        state: BatteryState::from_u32(state_u32),
        icon_name,
        time_to_empty,
        time_to_full,
    })
}

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct BatteryDaemon {
    info: Arc<StdMutex<BatteryInfo>>,
    conn: Connection,
}

impl BatteryDaemon {
    async fn new() -> Result<Self> {
        // Connect to system bus (UPower lives on system bus)
        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

        // Get initial battery info
        let info = match get_battery_info(&conn).await {
            Ok(info) => {
                log::info!(
                    "Initial battery state: present={}, {:.0}%, {:?}",
                    info.present,
                    info.percentage,
                    info.state
                );
                info
            }
            Err(e) => {
                log::warn!("Failed to read initial battery info: {e}");
                BatteryInfo::default()
            }
        };

        Ok(Self {
            info: Arc::new(StdMutex::new(info)),
            conn,
        })
    }

    fn current_info(&self) -> BatteryInfo {
        self.info.lock().unwrap().clone()
    }

    fn shared_info(&self) -> Arc<StdMutex<BatteryInfo>> {
        self.info.clone()
    }

    fn build_battery_widget(&self) -> Widget {
        let info = self.current_info();

        let icon = if info.icon_name.is_empty() {
            "battery-symbolic".to_string()
        } else {
            info.icon_name.clone()
        };

        InfoCardBuilder::new(format!("{:.0}%", info.percentage))
            .icon(icon)
            .description(info.status_text())
            .build()
    }
}

#[async_trait::async_trait]
impl PluginDaemon for BatteryDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let info = self.current_info();
        if !info.present {
            return vec![];
        }

        vec![NamedWidget {
            id: "battery:main".to_string(),
            weight: 30,
            widget: self.build_battery_widget(),
        }]
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        _action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Battery is display-only, no actions to handle
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// D-Bus signal monitoring
// ---------------------------------------------------------------------------

/// Listen for PropertiesChanged signals on the UPower DisplayDevice and
/// update shared state. On each change re-reads all properties for consistency.
async fn monitor_battery_signals(
    conn: Connection,
    info: Arc<StdMutex<BatteryInfo>>,
    notifier: WidgetNotifier,
) -> Result<()> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .path(DISPLAY_DEVICE_PATH)?
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

    log::info!("Listening for UPower PropertiesChanged signals on DisplayDevice");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("D-Bus stream error: {}", e);
                continue;
            }
        };

        let header = msg.header();
        if header.member().map(|m| m.as_str()) == Some("PropertiesChanged")
            && header.interface().map(|i| i.as_str())
                == Some("org.freedesktop.DBus.Properties")
        {
            // Check that the first argument (interface name) matches the device interface
            if let Ok((iface_name,)) = msg.body().deserialize::<(String,)>() {
                if iface_name != IFACE_DEVICE {
                    continue;
                }
            }

            // Re-read all properties for consistency
            match get_battery_info(&conn).await {
                Ok(new_info) => {
                    log::info!(
                        "Battery updated: present={}, {:.0}%, {:?}",
                        new_info.present,
                        new_info.percentage,
                        new_info.state
                    );
                    *info.lock().unwrap() = new_info;
                    notifier.notify();
                }
                Err(e) => {
                    log::warn!("Failed to re-read battery info after signal: {e}");
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    waft_plugin_sdk::init_daemon_logger("info");

    log::info!("Starting battery daemon...");

    let daemon = BatteryDaemon::new().await?;

    // Grab shared handles before daemon is moved into the server
    let shared_info = daemon.shared_info();
    let monitor_conn = daemon.conn.clone();

    let (server, notifier) = PluginServer::new("battery-daemon", daemon);

    // Listen for D-Bus PropertiesChanged signals (instant, no polling)
    tokio::spawn(async move {
        if let Err(e) = monitor_battery_signals(monitor_conn, shared_info, notifier).await {
            log::error!("D-Bus signal monitoring failed: {}", e);
        }
    });

    server.run().await?;

    Ok(())
}
