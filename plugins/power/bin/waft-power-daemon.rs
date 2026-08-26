//! Power daemon - battery status and power profile management.
//!
//! Provides the existing `battery` entity via UPower and a new `power-profile`
//! entity via power-profiles-daemon. Updates are pushed from D-Bus property
//! change signals only; no polling.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "power"
//!
//! # One-release compatibility alias:
//! [[plugins]]
//! id = "battery"
//! ```

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result, anyhow, bail};
use waft_plugin::dbus_monitor::{SignalMonitorConfig, monitor_signal_async};
use waft_plugin::*;
use waft_protocol::JsonSchema;
use waft_protocol::description::{
    ActionDescription, ActionParamDescription, EntityTypeDescription, EnumVariantDescription,
    PluginDescription, PropertyDescription, PropertyValueType,
};
use zbus::Connection;
use zbus::zvariant::{OwnedValue, Value};

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| {
    waft_i18n::I18n::new(&[
        ("en-US", include_str!("../locales/en-US/battery.ftl")),
        ("cs-CZ", include_str!("../locales/cs-CZ/battery.ftl")),
    ])
});

fn i18n() -> &'static waft_i18n::I18n {
    &I18N
}

const PLUGIN_ID: &str = "power";
const BATTERY_ID: &str = "BAT0";
const POWER_PROFILE_ID: &str = "default";

const DBUS_PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

const UPOWER_DEST: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const UPOWER_DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

const POWER_PROFILES_DEST: &str = "org.freedesktop.UPower.PowerProfiles";
const POWER_PROFILES_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const POWER_PROFILES_IFACE: &str = "org.freedesktop.UPower.PowerProfiles";
const POWER_PROFILES_ACTION: &str = "set-profile";

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

#[derive(Clone, Debug, Default, PartialEq)]
struct BatteryInfo {
    present: bool,
    percentage: f64,
    state: LocalBatteryState,
    icon_name: String,
    time_to_empty: i64,
    time_to_full: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PowerProfileInfo {
    active_profile: String,
    profiles: Vec<String>,
    performance_degraded: Option<String>,
}

fn owned_to_bool(value: &OwnedValue) -> Option<bool> {
    <bool as TryFrom<_>>::try_from(Value::from(value.clone())).ok()
}

fn owned_to_f64(value: &OwnedValue) -> Option<f64> {
    <f64 as TryFrom<_>>::try_from(Value::from(value.clone())).ok()
}

fn owned_to_i64(value: &OwnedValue) -> Option<i64> {
    <i64 as TryFrom<_>>::try_from(Value::from(value.clone())).ok()
}

fn owned_to_u32(value: &OwnedValue) -> Option<u32> {
    <u32 as TryFrom<_>>::try_from(Value::from(value.clone())).ok()
}

fn owned_to_string(value: &OwnedValue) -> Option<String> {
    <String as TryFrom<_>>::try_from(Value::from(value.clone())).ok()
}

async fn get_all_properties(
    conn: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
) -> Result<HashMap<String, OwnedValue>> {
    let proxy = zbus::Proxy::new(conn, destination, path, DBUS_PROPERTIES_IFACE)
        .await
        .context("failed to create D-Bus properties proxy")?;

    let (props,): (HashMap<String, OwnedValue>,) = proxy
        .call("GetAll", &(interface,))
        .await
        .with_context(|| format!("failed to read properties from {destination}:{path}"))?;

    Ok(props)
}

async fn get_battery_info(conn: &Connection) -> Result<BatteryInfo> {
    let props = get_all_properties(conn, UPOWER_DEST, DISPLAY_DEVICE_PATH, UPOWER_DEVICE_IFACE)
        .await
        .context("failed to read UPower DisplayDevice")?;

    Ok(BatteryInfo {
        present: props
            .get("IsPresent")
            .and_then(owned_to_bool)
            .unwrap_or(false),
        percentage: props
            .get("Percentage")
            .and_then(owned_to_f64)
            .unwrap_or(0.0),
        state: LocalBatteryState::from_u32(props.get("State").and_then(owned_to_u32).unwrap_or(0)),
        icon_name: props
            .get("IconName")
            .and_then(owned_to_string)
            .unwrap_or_default(),
        time_to_empty: props.get("TimeToEmpty").and_then(owned_to_i64).unwrap_or(0),
        time_to_full: props.get("TimeToFull").and_then(owned_to_i64).unwrap_or(0),
    })
}

fn normalize_profile_names(active_profile: &str, profiles: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for profile in profiles {
        if !profile.is_empty() && !normalized.iter().any(|existing| existing == profile) {
            normalized.push(profile.clone());
        }
    }
    if !active_profile.is_empty() && !normalized.iter().any(|profile| profile == active_profile) {
        normalized.push(active_profile.to_string());
    }
    normalized
}

fn parse_profile_entries(entries: Vec<HashMap<String, OwnedValue>>) -> Vec<String> {
    let profiles: Vec<String> = entries
        .into_iter()
        .filter_map(|entry| entry.get("Profile").and_then(owned_to_string))
        .collect();
    normalize_profile_names("", &profiles)
}

async fn get_power_profile_info(conn: &Connection) -> Result<PowerProfileInfo> {
    let props = get_all_properties(
        conn,
        POWER_PROFILES_DEST,
        POWER_PROFILES_PATH,
        POWER_PROFILES_IFACE,
    )
    .await
    .context("failed to read power-profiles-daemon state")?;

    let active_profile = props
        .get("ActiveProfile")
        .and_then(owned_to_string)
        .ok_or_else(|| anyhow!("missing ActiveProfile property"))?;
    let profile_entries = props
        .get("Profiles")
        .cloned()
        .map(Value::from)
        .and_then(|value| <Vec<HashMap<String, OwnedValue>> as TryFrom<_>>::try_from(value).ok())
        .unwrap_or_default();
    let profiles =
        normalize_profile_names(&active_profile, &parse_profile_entries(profile_entries));
    let performance_degraded = props
        .get("PerformanceDegraded")
        .and_then(owned_to_string)
        .and_then(|value| (!value.trim().is_empty()).then_some(value));

    Ok(PowerProfileInfo {
        active_profile,
        profiles,
        performance_degraded,
    })
}

fn battery_entity(info: &BatteryInfo) -> Option<Entity> {
    info.present.then(|| {
        let battery = entity::power::Battery {
            present: info.present,
            percentage: info.percentage,
            state: info.state.to_protocol(),
            icon_name: if info.icon_name.is_empty() {
                "battery-symbolic".to_string()
            } else {
                info.icon_name.clone()
            },
            time_to_empty: info.time_to_empty,
            time_to_full: info.time_to_full,
        };

        Entity::new(
            Urn::new(PLUGIN_ID, entity::power::ENTITY_TYPE, BATTERY_ID),
            entity::power::ENTITY_TYPE,
            &battery,
        )
    })
}

fn power_profile_entity(info: &PowerProfileInfo) -> Option<Entity> {
    (!info.profiles.is_empty()).then(|| {
        let profile = entity::power::PowerProfile {
            active_profile: info.active_profile.clone(),
            profiles: info.profiles.clone(),
            performance_degraded: info.performance_degraded.clone(),
        };

        Entity::new(
            Urn::new(
                PLUGIN_ID,
                entity::power::POWER_PROFILE_ENTITY_TYPE,
                POWER_PROFILE_ID,
            ),
            entity::power::POWER_PROFILE_ENTITY_TYPE,
            &profile,
        )
    })
}

#[cfg(test)]
fn toggle_target_profile(current: &str, profiles: &[String]) -> Option<String> {
    if current == "power-saver" {
        profiles
            .iter()
            .find(|profile| profile.as_str() == "balanced")
            .or_else(|| {
                profiles
                    .iter()
                    .find(|profile| profile.as_str() != "power-saver")
            })
            .cloned()
    } else {
        profiles
            .iter()
            .find(|profile| profile.as_str() == "power-saver")
            .cloned()
    }
}

struct PowerPlugin {
    battery: Arc<StdMutex<Option<BatteryInfo>>>,
    profile: Arc<StdMutex<Option<PowerProfileInfo>>>,
    conn: Connection,
}

impl PowerPlugin {
    async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("failed to connect to system bus")?;

        let battery = match get_battery_info(&conn).await {
            Ok(info) => {
                log::info!(
                    "Initial battery state: present={}, {:.0}%, {:?}",
                    info.present,
                    info.percentage,
                    info.state
                );
                Some(info)
            }
            Err(e) => {
                log::warn!("Failed to read initial battery info: {e}");
                None
            }
        };

        let profile = match get_power_profile_info(&conn).await {
            Ok(info) => {
                log::info!(
                    "Initial power profile state: active={}, available={:?}",
                    info.active_profile,
                    info.profiles
                );
                Some(info)
            }
            Err(e) => {
                log::info!("Power profiles unavailable: {e}");
                None
            }
        };

        Ok(Self {
            battery: Arc::new(StdMutex::new(battery)),
            profile: Arc::new(StdMutex::new(profile)),
            conn,
        })
    }

    fn shared_battery(&self) -> Arc<StdMutex<Option<BatteryInfo>>> {
        self.battery.clone()
    }

    fn shared_profile(&self) -> Arc<StdMutex<Option<PowerProfileInfo>>> {
        self.profile.clone()
    }

    async fn set_active_profile(&self, profile: &str) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            POWER_PROFILES_DEST,
            POWER_PROFILES_PATH,
            DBUS_PROPERTIES_IFACE,
        )
        .await
        .context("failed to create power profile properties proxy")?;

        let value = Value::from(profile.to_string());
        let _: () = proxy
            .call("Set", &(POWER_PROFILES_IFACE, "ActiveProfile", value))
            .await
            .with_context(|| format!("failed to set power profile to '{profile}'"))?;
        Ok(())
    }

    fn current_battery(&self) -> Option<BatteryInfo> {
        self.battery.lock_or_recover().clone()
    }

    fn current_profile(&self) -> Option<PowerProfileInfo> {
        self.profile.lock_or_recover().clone()
    }
}

#[async_trait::async_trait]
impl Plugin for PowerPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let mut entities = Vec::new();
        if let Some(info) = self.current_battery().as_ref().and_then(battery_entity) {
            entities.push(info);
        }
        if let Some(info) = self
            .current_profile()
            .as_ref()
            .and_then(power_profile_entity)
        {
            entities.push(info);
        }
        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if urn.entity_type() != entity::power::POWER_PROFILE_ENTITY_TYPE {
            bail!("unsupported entity type: {}", urn.entity_type());
        }
        if action != POWER_PROFILES_ACTION {
            bail!("unsupported action: {action}");
        }

        let profile = params
            .get("profile")
            .and_then(serde_json::Value::as_str)
            .filter(|profile| !profile.is_empty())
            .ok_or_else(|| anyhow!("missing required string param: profile"))?
            .to_string();

        let current = self
            .current_profile()
            .ok_or_else(|| anyhow!("power profiles are unavailable"))?;
        if !current
            .profiles
            .iter()
            .any(|candidate| candidate == &profile)
        {
            bail!("unsupported power profile: {profile}");
        }

        self.set_active_profile(&profile).await?;
        let refreshed = get_power_profile_info(&self.conn).await?;
        *self.profile.lock_or_recover() = Some(refreshed);
        Ok(serde_json::Value::Null)
    }

    fn describe(&self) -> Option<PluginDescription> {
        describe_power_plugin()
    }
}

fn describe_power_plugin() -> Option<PluginDescription> {
    Some(PluginDescription {
        name: PLUGIN_ID.to_string(),
        display_name: "Power".to_string(),
        description: "Battery monitoring and power profile management".to_string(),
        entity_types: vec![
            EntityTypeDescription {
                entity_type: entity::power::ENTITY_TYPE.to_string(),
                display_name: "Battery".to_string(),
                description: "Battery status from the UPower DisplayDevice".to_string(),
                properties: vec![
                    PropertyDescription {
                        name: "present".to_string(),
                        label: "Present".to_string(),
                        description: "Whether a battery is present".to_string(),
                        value_type: PropertyValueType::Bool,
                    },
                    PropertyDescription {
                        name: "percentage".to_string(),
                        label: "Percentage".to_string(),
                        description: "Current charge percentage".to_string(),
                        value_type: PropertyValueType::Percent,
                    },
                    PropertyDescription {
                        name: "state".to_string(),
                        label: "State".to_string(),
                        description: "Current battery charge state".to_string(),
                        value_type: PropertyValueType::Enum {
                            variants: vec![
                                "Unknown",
                                "Charging",
                                "Discharging",
                                "Empty",
                                "FullyCharged",
                                "PendingCharge",
                                "PendingDischarge",
                            ]
                            .into_iter()
                            .map(|name| EnumVariantDescription {
                                name: name.to_string(),
                                label: name.to_string(),
                            })
                            .collect(),
                        },
                    },
                    PropertyDescription {
                        name: "icon_name".to_string(),
                        label: "Icon".to_string(),
                        description: "UPower battery icon name".to_string(),
                        value_type: PropertyValueType::String,
                    },
                    PropertyDescription {
                        name: "time_to_empty".to_string(),
                        label: "Time to Empty".to_string(),
                        description: "Seconds until empty when discharging".to_string(),
                        value_type: PropertyValueType::Number,
                    },
                    PropertyDescription {
                        name: "time_to_full".to_string(),
                        label: "Time to Full".to_string(),
                        description: "Seconds until full when charging".to_string(),
                        value_type: PropertyValueType::Number,
                    },
                ],
                actions: vec![],
                data_schema: Some(
                    JsonSchema::object()
                        .with_property("present", JsonSchema::boolean(), true)
                        .with_property("percentage", JsonSchema::number(), true)
                        .with_property("state", JsonSchema::string(), true)
                        .with_property("icon_name", JsonSchema::string(), true)
                        .with_property("time_to_empty", JsonSchema::number(), true)
                        .with_property("time_to_full", JsonSchema::number(), true)
                        .closed(),
                ),
            },
            EntityTypeDescription {
                entity_type: entity::power::POWER_PROFILE_ENTITY_TYPE.to_string(),
                display_name: "Power Profile".to_string(),
                description: "Power profile selection state from power-profiles-daemon".to_string(),
                properties: vec![
                    PropertyDescription {
                        name: "active_profile".to_string(),
                        label: "Active Profile".to_string(),
                        description: "Current backend-native profile name".to_string(),
                        value_type: PropertyValueType::String,
                    },
                    PropertyDescription {
                        name: "profiles".to_string(),
                        label: "Profiles".to_string(),
                        description: "Available backend-native profile names".to_string(),
                        value_type: PropertyValueType::Array,
                    },
                    PropertyDescription {
                        name: "performance_degraded".to_string(),
                        label: "Performance Degraded".to_string(),
                        description: "Optional degraded-performance reason".to_string(),
                        value_type: PropertyValueType::String,
                    },
                ],
                actions: vec![ActionDescription {
                    name: POWER_PROFILES_ACTION.to_string(),
                    label: "Set Profile".to_string(),
                    description: "Switch the active power profile".to_string(),
                    params: vec![ActionParamDescription {
                        name: "profile".to_string(),
                        label: "Profile".to_string(),
                        description: "Backend-native profile name to activate".to_string(),
                        required: true,
                        value_type: PropertyValueType::String,
                    }],
                    params_schema: Some(
                        JsonSchema::object()
                            .with_property("profile", JsonSchema::string(), true)
                            .closed(),
                    ),
                    result_schema: None,
                    error_codes: vec!["action.invalid-params".to_string()],
                }],
                data_schema: Some(
                    JsonSchema::object()
                        .with_property("active_profile", JsonSchema::string(), true)
                        .with_property("profiles", JsonSchema::array(JsonSchema::string()), true)
                        .with_property("performance_degraded", JsonSchema::string(), false)
                        .closed(),
                ),
            },
        ],
    })
}

async fn monitor_backend_signals<T, F>(
    conn: Connection,
    sender: &str,
    path: &str,
    watched_interface: &str,
    label: &str,
    state: Arc<StdMutex<Option<T>>>,
    notifier: EntityNotifier,
    refresh: F,
) -> Result<()>
where
    T: Clone + Send + 'static,
    F: Fn(Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>
        + Send
        + Sync
        + 'static,
{
    let config = SignalMonitorConfig::builder()
        .sender(sender)
        .path(path)
        .interface(DBUS_PROPERTIES_IFACE)
        .member("PropertiesChanged")
        .build()?;
    let refresh = Arc::new(refresh);
    let conn_for_handler = conn.clone();
    let watched_interface = watched_interface.to_string();
    let label = label.to_string();

    monitor_signal_async(conn, config, state, notifier, move |msg, current_state| {
        let body = msg
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>();
        let refresh = refresh.clone();
        let conn = conn_for_handler.clone();
        let watched_interface = watched_interface.clone();
        let label = label.clone();

        Box::pin(async move {
            let (iface_name, _changed, _invalidated) = body?;
            if iface_name != watched_interface {
                return Ok(None);
            }

            match refresh(conn).await {
                Ok(new_state) => {
                    log::info!("{label} updated");
                    Ok(Some(Some(new_state)))
                }
                Err(err) => {
                    log::warn!("{label} refresh failed: {err}");
                    let had_state = current_state.lock_or_recover().is_some();
                    if had_state { Ok(Some(None)) } else { Ok(None) }
                }
            }
        })
    })
    .await
}

fn main() -> Result<()> {
    PluginRunner::new(
        PLUGIN_ID,
        &[
            entity::power::ENTITY_TYPE,
            entity::power::POWER_PROFILE_ENTITY_TYPE,
        ],
    )
    .i18n(i18n(), "plugin-name", "plugin-description")
    .describe(describe_power_plugin)
    .run(|notifier| async move {
        let plugin = PowerPlugin::new().await?;

        let battery = plugin.shared_battery();
        let profile = plugin.shared_profile();
        let battery_conn = plugin.conn.clone();
        let profile_conn = plugin.conn.clone();

        spawn_monitored(
            "power/battery",
            monitor_backend_signals(
                battery_conn,
                UPOWER_DEST,
                DISPLAY_DEVICE_PATH,
                UPOWER_DEVICE_IFACE,
                "battery",
                battery,
                notifier.clone(),
                |conn| Box::pin(async move { get_battery_info(&conn).await }),
            ),
        );

        spawn_monitored(
            "power/power-profile",
            monitor_backend_signals(
                profile_conn,
                POWER_PROFILES_DEST,
                POWER_PROFILES_PATH,
                POWER_PROFILES_IFACE,
                "power profile",
                profile,
                notifier,
                |conn| Box::pin(async move { get_power_profile_info(&conn).await }),
            ),
        );

        Ok(plugin)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_profile_names_deduplicates_and_keeps_active() {
        let normalized = normalize_profile_names(
            "balanced",
            &[
                "power-saver".to_string(),
                "balanced".to_string(),
                "balanced".to_string(),
            ],
        );
        assert_eq!(normalized, vec!["power-saver", "balanced"]);

        let with_missing_active = normalize_profile_names("performance", &["balanced".to_string()]);
        assert_eq!(with_missing_active, vec!["balanced", "performance"]);
    }

    #[test]
    fn parse_profile_entries_extracts_profile_names() {
        let entries = vec![
            HashMap::from([(
                "Profile".to_string(),
                OwnedValue::try_from(Value::from("power-saver".to_string())).expect("owned value"),
            )]),
            HashMap::from([(
                "Profile".to_string(),
                OwnedValue::try_from(Value::from("balanced".to_string())).expect("owned value"),
            )]),
        ];

        assert_eq!(
            parse_profile_entries(entries),
            vec!["power-saver", "balanced"]
        );
    }

    #[test]
    fn entity_emission_handles_all_availability_combinations() {
        let battery = BatteryInfo {
            present: true,
            percentage: 87.0,
            state: LocalBatteryState::Discharging,
            icon_name: String::new(),
            time_to_empty: 0,
            time_to_full: 0,
        };
        let profile = PowerProfileInfo {
            active_profile: "balanced".to_string(),
            profiles: vec!["power-saver".to_string(), "balanced".to_string()],
            performance_degraded: None,
        };

        assert!(battery_entity(&battery).is_some());
        assert!(power_profile_entity(&profile).is_some());
        assert!(
            battery_entity(&BatteryInfo {
                present: false,
                ..battery.clone()
            })
            .is_none()
        );
        assert!(
            power_profile_entity(&PowerProfileInfo {
                profiles: vec![],
                ..profile
            })
            .is_none()
        );
    }

    #[test]
    fn toggle_target_profile_prefers_balanced_then_first_non_saver() {
        let profiles = vec![
            "power-saver".to_string(),
            "balanced".to_string(),
            "performance".to_string(),
        ];
        assert_eq!(
            toggle_target_profile("power-saver", &profiles),
            Some("balanced".to_string())
        );
        assert_eq!(
            toggle_target_profile("balanced", &profiles),
            Some("power-saver".to_string())
        );

        let no_balanced = vec!["power-saver".to_string(), "performance".to_string()];
        assert_eq!(
            toggle_target_profile("power-saver", &no_balanced),
            Some("performance".to_string())
        );
        assert_eq!(
            toggle_target_profile("performance", &no_balanced),
            Some("power-saver".to_string())
        );

        let no_saver = vec!["balanced".to_string()];
        assert_eq!(toggle_target_profile("balanced", &no_saver), None);
    }
}
