//! Systemd daemon -- system power, session management, and user service monitoring.
//!
//! Provides a session entity with the current user's name and display,
//! and user-service entities for systemd user services.
//! Handles power and session actions via D-Bus calls to systemd-logind.
//! Monitors user services via D-Bus on the session bus.
//!
//! Session actions:
//! - `lock` - Lock the current session
//! - `logout` - Terminate the current session
//! - `reboot` - Reboot the system
//! - `shutdown` - Power off the system
//! - `suspend` - Suspend the system
//!
//! User service actions:
//! - `start` - Start a user service
//! - `stop` - Stop a user service
//! - `enable` - Enable a user service on login
//! - `disable` - Disable a user service on login
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "systemd"
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use waft_i18n::I18n;
use waft_plugin::*;
use waft_protocol::entity::session::UserService;
use zbus::Connection;
use zbus::zvariant::OwnedValue;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/systemd.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/systemd.ftl")),
        ])
    })
}

const LOGIN1_DESTINATION: &str = "org.freedesktop.login1";
const LOGIN1_MANAGER_PATH: &str = "/org/freedesktop/login1";
const LOGIN1_MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
const LOGIN1_SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";

const SYSTEMD1_DESTINATION: &str = "org.freedesktop.systemd1";
const SYSTEMD1_MANAGER_PATH: &str = "/org/freedesktop/systemd1";
const SYSTEMD1_MANAGER_INTERFACE: &str = "org.freedesktop.systemd1.Manager";
const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// Resolve the current session's D-Bus object path.
///
/// Checks `XDG_SESSION_ID` environment variable and falls back to `/session/auto`.
fn get_session_path() -> String {
    if let Ok(session_id) = std::env::var("XDG_SESSION_ID") {
        format!("/org/freedesktop/login1/session/{}", session_id)
    } else {
        "/org/freedesktop/login1/session/auto".to_string()
    }
}

/// Get the current user's login name from the environment.
fn get_user_name() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
}

/// Get the current display from the environment.
fn get_screen_name() -> Option<String> {
    std::env::var("WAYLAND_DISPLAY")
        .or_else(|_| std::env::var("DISPLAY"))
        .ok()
}

/// Strip the `.service` suffix from a unit name for use as URN ID.
fn unit_to_urn_id(unit: &str) -> String {
    unit.strip_suffix(".service").unwrap_or(unit).to_string()
}

/// Reconstruct the full unit name from a URN ID.
fn urn_id_to_unit(id: &str) -> String {
    if id.ends_with(".service") {
        id.to_string()
    } else {
        format!("{}.service", id)
    }
}

/// Map `UnitFileState` string to a boolean `enabled` value.
/// "enabled" and "enabled-runtime" are considered enabled, all others are disabled.
fn unit_file_state_to_enabled(state: &str) -> bool {
    matches!(state, "enabled" | "enabled-runtime")
}

/// Systemd plugin.
///
/// Provides session entity (stateless) and user service entities (stateful).
/// Actions dispatch D-Bus calls to login1 (system bus) and systemd1 (session bus).
struct SystemdPlugin {
    system_conn: Connection,
    session_conn: Connection,
    session_path: String,
    user_name: Option<String>,
    screen_name: Option<String>,
    services: Arc<StdMutex<HashMap<String, UserService>>>,
}

impl SystemdPlugin {
    async fn new() -> Result<Self> {
        let system_conn = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        let session_conn = Connection::session()
            .await
            .context("Failed to connect to session D-Bus")?;

        let session_path = get_session_path();
        log::info!("[systemd] Using session path: {}", session_path);

        let services = Arc::new(StdMutex::new(HashMap::new()));

        let mut plugin = Self {
            system_conn,
            session_conn,
            session_path,
            user_name: get_user_name(),
            screen_name: get_screen_name(),
            services,
        };

        plugin.load_services().await;

        Ok(plugin)
    }

    /// Load user services from systemd1 on the session bus.
    async fn load_services(&mut self) {
        match self.list_services().await {
            Ok(svc_list) => {
                let mut services = match self.services.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("[systemd] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                for svc in svc_list {
                    services.insert(unit_to_urn_id(&svc.unit), svc);
                }
                log::info!("[systemd] Loaded {} user services", services.len());
            }
            Err(e) => {
                log::warn!("[systemd] Failed to list user services: {e}");
            }
        }
    }

    /// List loaded .service units from the user systemd instance.
    async fn list_services(&self) -> Result<Vec<UserService>> {
        let proxy = zbus::Proxy::new(
            &self.session_conn,
            SYSTEMD1_DESTINATION,
            SYSTEMD1_MANAGER_PATH,
            SYSTEMD1_MANAGER_INTERFACE,
        )
        .await
        .context("Failed to create systemd1 manager proxy")?;

        // ListUnitsByPatterns(states: as, patterns: as) -> a(ssssssouso)
        // Fields: name, description, load_state, active_state, sub_state,
        //         followed_by, object_path, queued_job_id, job_type, job_object_path
        let units: Vec<(
            String, String, String, String, String,
            String, zbus::zvariant::OwnedObjectPath, u32, String, zbus::zvariant::OwnedObjectPath,
        )> = proxy
            .call(
                "ListUnitsByPatterns",
                &(
                    vec!["loaded"] as Vec<&str>,
                    vec!["*.service"] as Vec<&str>,
                ),
            )
            .await
            .context("Failed to call ListUnitsByPatterns")?;

        let mut services = Vec::with_capacity(units.len());
        for (name, description, _load_state, active_state, sub_state, _, _, _, _, _) in &units {
            let enabled = self.get_unit_file_state(name).await;
            services.push(UserService {
                unit: name.clone(),
                description: description.clone(),
                active_state: active_state.clone(),
                enabled,
                sub_state: sub_state.clone(),
            });
        }

        Ok(services)
    }

    /// Get the UnitFileState for a unit and convert to bool.
    async fn get_unit_file_state(&self, unit: &str) -> bool {
        let proxy = zbus::Proxy::new(
            &self.session_conn,
            SYSTEMD1_DESTINATION,
            SYSTEMD1_MANAGER_PATH,
            SYSTEMD1_MANAGER_INTERFACE,
        )
        .await;

        let proxy = match proxy {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[systemd] Failed to create proxy for unit file state: {e}");
                return false;
            }
        };

        let state: Result<String, _> = proxy
            .call("GetUnitFileState", &(unit,))
            .await;

        match state {
            Ok(s) => unit_file_state_to_enabled(&s),
            Err(e) => {
                log::debug!("[systemd] Failed to get unit file state for {}: {e}", unit);
                false
            }
        }
    }

    /// Refresh a single service's state and update the services map.
    /// Returns true if the service state changed.
    async fn refresh_service(&self, unit: &str) -> bool {
        let proxy = match zbus::Proxy::new(
            &self.session_conn,
            SYSTEMD1_DESTINATION,
            SYSTEMD1_MANAGER_PATH,
            SYSTEMD1_MANAGER_INTERFACE,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[systemd] Failed to create proxy for refresh: {e}");
                return false;
            }
        };

        // GetUnit returns the object path for the unit
        let unit_path: Result<zbus::zvariant::OwnedObjectPath, _> =
            proxy.call("GetUnit", &(unit,)).await;

        let unit_path = match unit_path {
            Ok(p) => p,
            Err(e) => {
                log::debug!("[systemd] Failed to GetUnit {}: {e}", unit);
                // Unit might have been unloaded - remove from services
                let mut services = match self.services.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("[systemd] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                let urn_id = unit_to_urn_id(unit);
                return services.remove(&urn_id).is_some();
            }
        };

        // Read properties from the unit object
        let props_proxy = match zbus::Proxy::new(
            &self.session_conn,
            SYSTEMD1_DESTINATION,
            unit_path.as_str(),
            "org.freedesktop.systemd1.Unit",
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[systemd] Failed to create unit proxy for {}: {e}", unit);
                return false;
            }
        };

        let active_state: Result<String, _> = props_proxy
            .get_property("ActiveState")
            .await;
        let sub_state: Result<String, _> = props_proxy
            .get_property("SubState")
            .await;
        let description: Result<String, _> = props_proxy
            .get_property("Description")
            .await;

        let active_state = active_state.unwrap_or_else(|_| "unknown".to_string());
        let sub_state = sub_state.unwrap_or_else(|_| "unknown".to_string());
        let description = description.unwrap_or_default();
        let enabled = self.get_unit_file_state(unit).await;

        let urn_id = unit_to_urn_id(unit);
        let new_svc = UserService {
            unit: unit.to_string(),
            description,
            active_state,
            enabled,
            sub_state,
        };

        let mut services = match self.services.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[systemd] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };

        let changed = services.get(&urn_id) != Some(&new_svc);
        services.insert(urn_id, new_svc);
        changed
    }

    /// Call a method on the login1 session interface (no arguments).
    async fn call_session_method(&self, method: &str) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &self.system_conn,
            LOGIN1_DESTINATION,
            self.session_path.as_str(),
            LOGIN1_SESSION_INTERFACE,
        )
        .await
        .context("Failed to create session proxy")?;

        let _: () = proxy
            .call(method, &())
            .await
            .with_context(|| format!("Failed to call Session.{}", method))?;

        log::info!("[systemd] Session.{}() executed", method);
        Ok(())
    }

    /// Call a method on the login1 manager interface with an `interactive: bool` argument.
    async fn call_login1_manager_method(&self, method: &str, interactive: bool) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &self.system_conn,
            LOGIN1_DESTINATION,
            LOGIN1_MANAGER_PATH,
            LOGIN1_MANAGER_INTERFACE,
        )
        .await
        .context("Failed to create manager proxy")?;

        let _: () = proxy
            .call(method, &(interactive,))
            .await
            .with_context(|| format!("Failed to call Manager.{}", method))?;

        log::info!("[systemd] Manager.{}(interactive={}) executed", method, interactive);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Plugin for SystemdPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let session = entity::session::Session {
            user_name: self.user_name.clone(),
            screen_name: self.screen_name.clone(),
        };

        let mut entities = vec![Entity::new(
            Urn::new(
                "systemd",
                entity::session::SESSION_ENTITY_TYPE,
                "default",
            ),
            entity::session::SESSION_ENTITY_TYPE,
            &session,
        )];

        let services = match self.services.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[systemd] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };

        for (urn_id, svc) in services.iter() {
            entities.push(Entity::new(
                Urn::new(
                    "systemd",
                    entity::session::USER_SERVICE_ENTITY_TYPE,
                    urn_id,
                ),
                entity::session::USER_SERVICE_ENTITY_TYPE,
                svc,
            ));
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

        if entity_type == entity::session::SESSION_ENTITY_TYPE {
            match action.as_str() {
                "lock" => self.call_session_method("Lock").await?,
                "logout" => self.call_session_method("Terminate").await?,
                "reboot" => self.call_login1_manager_method("Reboot", true).await?,
                "shutdown" => self.call_login1_manager_method("PowerOff", true).await?,
                "suspend" => self.call_login1_manager_method("Suspend", true).await?,
                other => log::warn!("[systemd] Unknown session action: {}", other),
            }
        } else if entity_type == entity::session::USER_SERVICE_ENTITY_TYPE {
            let unit = urn_id_to_unit(urn.id());
            let proxy = zbus::Proxy::new(
                &self.session_conn,
                SYSTEMD1_DESTINATION,
                SYSTEMD1_MANAGER_PATH,
                SYSTEMD1_MANAGER_INTERFACE,
            )
            .await
            .context("Failed to create systemd1 manager proxy")?;

            match action.as_str() {
                "start" => {
                    let _: zbus::zvariant::OwnedObjectPath = proxy
                        .call("StartUnit", &(&unit, "replace"))
                        .await
                        .with_context(|| format!("Failed to start {}", unit))?;
                    log::info!("[systemd] Started {}", unit);
                }
                "stop" => {
                    let _: zbus::zvariant::OwnedObjectPath = proxy
                        .call("StopUnit", &(&unit, "replace"))
                        .await
                        .with_context(|| format!("Failed to stop {}", unit))?;
                    log::info!("[systemd] Stopped {}", unit);
                }
                "enable" => {
                    let _: (bool, Vec<(String, String, String)>) = proxy
                        .call("EnableUnitFiles", &(vec![&unit as &str], false, true))
                        .await
                        .with_context(|| format!("Failed to enable {}", unit))?;
                    log::info!("[systemd] Enabled {}", unit);
                }
                "disable" => {
                    let _: Vec<(String, String, String)> = proxy
                        .call("DisableUnitFiles", &(vec![&unit as &str], false))
                        .await
                        .with_context(|| format!("Failed to disable {}", unit))?;
                    log::info!("[systemd] Disabled {}", unit);
                }
                other => log::warn!("[systemd] Unknown user-service action: {}", other),
            }

            // Refresh this service's state after the action
            self.refresh_service(&unit).await;
        } else {
            log::warn!("[systemd] Unknown entity type: {}", entity_type);
        }

        Ok(())
    }
}

/// Monitor PropertiesChanged, UnitNew, and UnitRemoved signals on the session bus.
async fn monitor_service_signals(
    conn: Connection,
    services: Arc<StdMutex<HashMap<String, UserService>>>,
    notifier: EntityNotifier,
) -> Result<()> {
    // Match PropertiesChanged on the systemd1 namespace
    let props_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(SYSTEMD1_DESTINATION)?
        .interface(IFACE_PROPERTIES)?
        .member("PropertiesChanged")?
        .build();

    // Match UnitNew and UnitRemoved from the manager
    let manager_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(SYSTEMD1_DESTINATION)?
        .interface(SYSTEMD1_MANAGER_INTERFACE)?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(props_rule)
        .await
        .context("Failed to add PropertiesChanged match rule")?;

    dbus_proxy
        .add_match_rule(manager_rule)
        .await
        .context("Failed to add Manager signal match rule")?;

    log::info!("[systemd] Listening for user service signals");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("[systemd] D-Bus stream error: {e}");
                continue;
            }
        };

        let header = msg.header();
        let member = match header.member() {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let iface = match header.interface() {
            Some(i) => i.as_str().to_string(),
            None => continue,
        };

        let mut changed = false;

        if iface == IFACE_PROPERTIES && member == "PropertiesChanged" {
            let obj_path = match header.path() {
                Some(p) => p.to_string(),
                None => continue,
            };

            // Filter to only .service unit paths
            // systemd1 unit paths look like /org/freedesktop/systemd1/unit/pipewire_2eservice
            if !obj_path.contains("_2eservice") {
                continue;
            }

            let Ok((prop_iface, props, _invalidated)) =
                msg.body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                continue;
            };

            if prop_iface != "org.freedesktop.systemd1.Unit" {
                continue;
            }

            // Extract unit name from changed properties or find it from the path
            changed = handle_unit_properties_changed(&services, &props, &obj_path, &conn).await;
        } else if iface == SYSTEMD1_MANAGER_INTERFACE && member == "UnitNew" {
            let Ok((unit_name, _obj_path)) =
                msg.body().deserialize::<(String, zbus::zvariant::OwnedObjectPath)>()
            else {
                continue;
            };

            if !unit_name.ends_with(".service") {
                continue;
            }

            log::info!("[systemd] UnitNew: {}", unit_name);
            changed = handle_unit_new(&services, &unit_name, &conn).await;
        } else if iface == SYSTEMD1_MANAGER_INTERFACE && member == "UnitRemoved" {
            let Ok((unit_name, _obj_path)) =
                msg.body().deserialize::<(String, zbus::zvariant::OwnedObjectPath)>()
            else {
                continue;
            };

            if !unit_name.ends_with(".service") {
                continue;
            }

            log::info!("[systemd] UnitRemoved: {}", unit_name);
            let urn_id = unit_to_urn_id(&unit_name);
            let mut svc = match services.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::warn!("[systemd] mutex poisoned, recovering: {e}");
                    e.into_inner()
                }
            };
            changed = svc.remove(&urn_id).is_some();
        }

        if changed {
            notifier.notify();
        }
    }

    log::warn!("[systemd] D-Bus signal stream ended -- service monitoring is now unresponsive");

    Ok(())
}

/// Handle PropertiesChanged for a unit. Read updated state from the unit object.
async fn handle_unit_properties_changed(
    services: &Arc<StdMutex<HashMap<String, UserService>>>,
    props: &HashMap<String, OwnedValue>,
    obj_path: &str,
    conn: &Connection,
) -> bool {
    // Try to read Id property from the unit to find the unit name
    let unit_proxy = match zbus::Proxy::new(
        conn,
        SYSTEMD1_DESTINATION,
        obj_path,
        "org.freedesktop.systemd1.Unit",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            log::debug!("[systemd] Failed to create unit proxy for {}: {e}", obj_path);
            return false;
        }
    };

    let unit_name: String = match unit_proxy.get_property("Id").await {
        Ok(id) => id,
        Err(e) => {
            log::debug!("[systemd] Failed to get Id for {}: {e}", obj_path);
            return false;
        }
    };

    if !unit_name.ends_with(".service") {
        return false;
    }

    let urn_id = unit_to_urn_id(&unit_name);

    let mut svc = match services.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("[systemd] mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };

    let Some(service) = svc.get_mut(&urn_id) else {
        return false;
    };

    let mut changed = false;

    if let Some(active_val) = props.get("ActiveState") {
        if let Ok(active_state) = String::try_from(active_val.clone()) {
            if service.active_state != active_state {
                log::info!(
                    "[systemd] {} active_state: {} -> {}",
                    unit_name, service.active_state, active_state,
                );
                service.active_state = active_state;
                changed = true;
            }
        }
    }

    if let Some(sub_val) = props.get("SubState") {
        if let Ok(sub_state) = String::try_from(sub_val.clone()) {
            if service.sub_state != sub_state {
                log::debug!(
                    "[systemd] {} sub_state: {} -> {}",
                    unit_name, service.sub_state, sub_state,
                );
                service.sub_state = sub_state;
                changed = true;
            }
        }
    }

    changed
}

/// Handle UnitNew signal: add a newly loaded service to the map.
async fn handle_unit_new(
    services: &Arc<StdMutex<HashMap<String, UserService>>>,
    unit_name: &str,
    conn: &Connection,
) -> bool {
    let urn_id = unit_to_urn_id(unit_name);

    // Check if we already have it
    {
        let svc = match services.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[systemd] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };
        if svc.contains_key(&urn_id) {
            return false;
        }
    }

    // Read properties for the new unit
    let proxy = match zbus::Proxy::new(
        conn,
        SYSTEMD1_DESTINATION,
        SYSTEMD1_MANAGER_PATH,
        SYSTEMD1_MANAGER_INTERFACE,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[systemd] Failed to create proxy for UnitNew: {e}");
            return false;
        }
    };

    let unit_path: zbus::zvariant::OwnedObjectPath = match proxy
        .call("GetUnit", &(unit_name,))
        .await
    {
        Ok(p) => p,
        Err(e) => {
            log::debug!("[systemd] Failed to GetUnit {} on UnitNew: {e}", unit_name);
            return false;
        }
    };

    let unit_proxy = match zbus::Proxy::new(
        conn,
        SYSTEMD1_DESTINATION,
        unit_path.as_str(),
        "org.freedesktop.systemd1.Unit",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[systemd] Failed to create unit proxy for {}: {e}", unit_name);
            return false;
        }
    };

    let description = unit_proxy.get_property::<String>("Description").await.unwrap_or_default();
    let active_state = unit_proxy.get_property::<String>("ActiveState").await.unwrap_or_else(|_| "unknown".to_string());
    let sub_state = unit_proxy.get_property::<String>("SubState").await.unwrap_or_else(|_| "unknown".to_string());

    // Get unit file state for enabled
    let enabled = {
        let state: Result<String, _> = proxy.call("GetUnitFileState", &(unit_name,)).await;
        match state {
            Ok(s) => unit_file_state_to_enabled(&s),
            Err(_) => false,
        }
    };

    let svc = UserService {
        unit: unit_name.to_string(),
        description,
        active_state,
        enabled,
        sub_state,
    };

    let mut services = match services.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("[systemd] mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };
    services.insert(urn_id, svc);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_to_urn_id_strips_service_suffix() {
        assert_eq!(unit_to_urn_id("pipewire.service"), "pipewire");
        assert_eq!(unit_to_urn_id("wireplumber.service"), "wireplumber");
    }

    #[test]
    fn unit_to_urn_id_preserves_non_service() {
        assert_eq!(unit_to_urn_id("pipewire.socket"), "pipewire.socket");
        assert_eq!(unit_to_urn_id("pipewire"), "pipewire");
    }

    #[test]
    fn urn_id_to_unit_appends_service_suffix() {
        assert_eq!(urn_id_to_unit("pipewire"), "pipewire.service");
        assert_eq!(urn_id_to_unit("wireplumber"), "wireplumber.service");
    }

    #[test]
    fn urn_id_to_unit_preserves_existing_suffix() {
        assert_eq!(urn_id_to_unit("pipewire.service"), "pipewire.service");
    }

    #[test]
    fn enabled_state_mapping() {
        assert!(unit_file_state_to_enabled("enabled"));
        assert!(unit_file_state_to_enabled("enabled-runtime"));
        assert!(!unit_file_state_to_enabled("disabled"));
        assert!(!unit_file_state_to_enabled("static"));
        assert!(!unit_file_state_to_enabled("masked"));
        assert!(!unit_file_state_to_enabled("indirect"));
    }
}

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides_i18n(
        &[
            entity::session::SESSION_ENTITY_TYPE,
            entity::session::USER_SERVICE_ENTITY_TYPE,
        ],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    log::info!("[systemd] Starting systemd plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = SystemdPlugin::new().await?;

        let services = plugin.services.clone();
        let session_conn = plugin.session_conn.clone();

        let (runtime, notifier) = PluginRuntime::new("systemd", plugin);

        // Spawn signal monitoring task
        let monitor_notifier = notifier.clone();
        tokio::spawn(async move {
            if let Err(e) = monitor_service_signals(session_conn, services, monitor_notifier).await
            {
                log::warn!("[systemd] Signal monitoring task error: {e}");
            }
            log::debug!("[systemd] Signal monitoring task stopped");
        });

        runtime.run().await?;
        Ok(())
    })
}
