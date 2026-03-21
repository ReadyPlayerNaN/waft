//! Battery daemon - displays battery status from UPower.
//!
//! Provides a `battery` entity via the UPower DisplayDevice on the system bus.
//! Updates are pushed when D-Bus PropertiesChanged signals arrive (no polling).
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "battery"
//! ```

use std::sync::LazyLock;

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;

use waft_plugin::dbus_monitor::{SignalMonitorConfig, monitor_signal_async};
use waft_plugin::*;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/battery.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/battery.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

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
enum LocalBatteryState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl LocalBatteryState {
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

    fn to_protocol(&self) -> entity::power::BatteryState {
        match self {
            Self::Unknown => entity::power::BatteryState::Unknown,
            Self::Charging => entity::power::BatteryState::Charging,
            Self::Discharging => entity::power::BatteryState::Discharging,
            Self::Empty => entity::power::BatteryState::Empty,
            Self::FullyCharged => entity::power::BatteryState::FullyCharged,
            Self::PendingCharge => entity::power::BatteryState::PendingCharge,
            Self::PendingDischarge => entity::power::BatteryState::PendingDischarge,
        }
    }
}

/// Current battery information from UPower DisplayDevice.
#[derive(Clone, Debug, Default)]
struct BatteryInfo {
    present: bool,
    percentage: f64,
    state: LocalBatteryState,
    icon_name: String,
    time_to_empty: i64,
    time_to_full: i64,
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
            if let zbus::zvariant::Value::Str(s) = zbus::zvariant::Value::from(v.clone()) {
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
        state: LocalBatteryState::from_u32(state_u32),
        icon_name,
        time_to_empty,
        time_to_full,
    })
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

struct BatteryPlugin {
    info: Arc<StdMutex<BatteryInfo>>,
    conn: Connection,
}

impl BatteryPlugin {
    async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

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
        self.info.lock_or_recover().clone()
    }

    fn shared_info(&self) -> Arc<StdMutex<BatteryInfo>> {
        self.info.clone()
    }
}

#[async_trait::async_trait]
impl Plugin for BatteryPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let info = self.current_info();
        if !info.present {
            return vec![];
        }

        let battery = entity::power::Battery {
            present: info.present,
            percentage: info.percentage,
            state: info.state.to_protocol(),
            icon_name: if info.icon_name.is_empty() {
                "battery-symbolic".to_string()
            } else {
                info.icon_name
            },
            time_to_empty: info.time_to_empty,
            time_to_full: info.time_to_full,
        };

        vec![Entity::new(
            Urn::new("battery", entity::power::ENTITY_TYPE, "BAT0"),
            entity::power::ENTITY_TYPE,
            &battery,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        _action: String,
        _params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        // Battery is display-only, no actions to handle
        Ok(serde_json::Value::Null)
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
    notifier: EntityNotifier,
) -> Result<()> {
    let config = SignalMonitorConfig::builder()
        .sender(UPOWER_DEST)
        .path(DISPLAY_DEVICE_PATH)
        .interface("org.freedesktop.DBus.Properties")
        .member("PropertiesChanged")
        .build()?;

    // Clone conn for use inside async handler
    let conn_for_handler = conn.clone();

    monitor_signal_async(conn, config, info, notifier, move |msg, _state| {
        // Deserialize the full PropertiesChanged body (sa{sv}as) before entering the async block
        let body_check =
            msg.body()
                .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>();

        let conn = conn_for_handler.clone();
        Box::pin(async move {
            let (iface_name, _changed_props, _invalidated) = body_check?;
            if iface_name != IFACE_DEVICE {
                return Ok(None); // Skip this signal
            }

            // Re-read all properties for consistency
            let new_info = get_battery_info(&conn).await?;

            log::info!(
                "Battery updated: present={}, {:.0}%, {:?}",
                new_info.present,
                new_info.percentage,
                new_info.state
            );
            Ok(Some(new_info))
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    PluginRunner::new("battery", &[entity::power::ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async move {
            let plugin = BatteryPlugin::new().await?;

            // Grab shared handles before plugin is moved into the runtime
            let shared_info = plugin.shared_info();
            let monitor_conn = plugin.conn.clone();

            // Listen for D-Bus PropertiesChanged signals (instant, no polling)
            spawn_monitored("battery", monitor_battery_signals(monitor_conn, shared_info, notifier));

            Ok(plugin)
        })
}
