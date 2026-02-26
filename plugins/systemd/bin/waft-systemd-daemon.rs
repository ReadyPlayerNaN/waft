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

use waft_plugin::StateLocker;

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

// D-Bus tuple type for ListUnitsByPatterns response fields:
// name, description, load_state, active_state, sub_state,
// followed_by, object_path, queued_job_id, job_type, job_object_path
type UnitTuple = (
    String, String, String, String, String,
    String, zbus::zvariant::OwnedObjectPath, u32, String, zbus::zvariant::OwnedObjectPath,
);

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
                let mut services = self.services.lock_or_recover();
                for svc in svc_list {
                    services.insert(unit_to_urn_id(&svc.unit), svc);
                }
                log::info!("[systemd] Loaded {} user services", services.len());
            }
            Err(e) => {
                log::warn!("[systemd] Failed to list user services: {e}");
            }
        }

        // Discover additional enabled/disabled unit files not currently loaded
        match self.list_unit_files().await {
            Ok(unit_files) => {
                let mut services = self.services.lock_or_recover();
                let mut added = 0usize;
                for (unit_name, file_state) in unit_files {
                    let urn_id = unit_to_urn_id(&unit_name);
                    if services.contains_key(&urn_id) {
                        continue;
                    }
                    services.insert(urn_id, UserService {
                        unit: unit_name,
                        description: String::new(),
                        active_state: "inactive".to_string(),
                        sub_state: "dead".to_string(),
                        enabled: unit_file_state_to_enabled(&file_state),
                    });
                    added += 1;
                }
                if added > 0 {
                    log::info!("[systemd] Discovered {} additional unit files", added);
                }
            }
            Err(e) => {
                log::warn!("[systemd] Failed to list unit files (continuing with loaded services only): {e}");
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
        let units: Vec<UnitTuple> = proxy
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

    /// List unit files matching `*.service` from the user systemd instance.
    /// Calls `ListUnitFilesByPatterns([], ["*.service"])` which returns `a(ss)` pairs
    /// of (unit_file_path, state). Extracts unit name from file path basename.
    /// Filters out template units (names containing `@`).
    async fn list_unit_files(&self) -> Result<Vec<(String, String)>> {
        let proxy = zbus::Proxy::new(
            &self.session_conn,
            SYSTEMD1_DESTINATION,
            SYSTEMD1_MANAGER_PATH,
            SYSTEMD1_MANAGER_INTERFACE,
        )
        .await
        .context("Failed to create systemd1 manager proxy")?;

        let files: Vec<(String, String)> = proxy
            .call(
                "ListUnitFilesByPatterns",
                &(
                    Vec::<&str>::new(),
                    vec!["*.service"] as Vec<&str>,
                ),
            )
            .await
            .context("Failed to call ListUnitFilesByPatterns")?;

        let mut result = Vec::with_capacity(files.len());
        for (file_path, state) in files {
            let basename = file_path
                .rsplit('/')
                .next()
                .unwrap_or(&file_path);
            // Filter out template units
            if basename.contains('@') {
                continue;
            }
            result.push((basename.to_string(), state));
        }

        Ok(result)
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
                // Unit unloaded -- update in-place to inactive/dead, preserve identity
                let enabled = self.get_unit_file_state(unit).await;
                let mut services = self.services.lock_or_recover();
                let urn_id = unit_to_urn_id(unit);
                if let Some(svc) = services.get_mut(&urn_id) {
                    let was_different = svc.active_state != "inactive"
                        || svc.sub_state != "dead"
                        || svc.enabled != enabled;
                    svc.active_state = "inactive".to_string();
                    svc.sub_state = "dead".to_string();
                    svc.enabled = enabled;
                    return was_different;
                }
                return false;
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

        let mut services = self.services.lock_or_recover();

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

        let services = self.services.lock_or_recover();

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

/// Refresh the `enabled` field for all known services by re-querying `GetUnitFileState`.
/// Clones the service keys to avoid holding the mutex during D-Bus calls.
/// Returns true if any service's enabled state changed.
async fn refresh_all_enabled_states(
    services: &Arc<StdMutex<HashMap<String, UserService>>>,
    conn: &Connection,
) -> bool {
    let keys: Vec<(String, String)> = {
        let svc = services.lock_or_recover();
        svc.iter().map(|(k, v)| (k.clone(), v.unit.clone())).collect()
    };

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
            log::warn!("[systemd] Failed to create proxy for enabled state refresh: {e}");
            return false;
        }
    };

    let mut updates: Vec<(String, bool)> = Vec::new();
    for (urn_id, unit_name) in &keys {
        let state: Result<String, _> = proxy.call("GetUnitFileState", &(unit_name.as_str(),)).await;
        let enabled = match state {
            Ok(s) => unit_file_state_to_enabled(&s),
            Err(e) => {
                log::debug!("[systemd] Failed to get unit file state for {}: {e}", unit_name);
                false
            }
        };
        updates.push((urn_id.clone(), enabled));
    }

    let mut svc = services.lock_or_recover();

    let mut any_changed = false;
    for (urn_id, enabled) in updates {
        if let Some(service) = svc.get_mut(&urn_id)
            && service.enabled != enabled {
                log::info!(
                    "[systemd] {} enabled: {} -> {}",
                    service.unit, service.enabled, enabled,
                );
                service.enabled = enabled;
                any_changed = true;
            }
    }

    any_changed
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
            let mut svc = services.lock_or_recover();
            if let Some(service) = svc.get_mut(&urn_id)
                && (service.active_state != "inactive" || service.sub_state != "dead") {
                    service.active_state = "inactive".to_string();
                    service.sub_state = "dead".to_string();
                    changed = true;
                }
        } else if iface == SYSTEMD1_MANAGER_INTERFACE && member == "UnitFilesChanged" {
            log::info!("[systemd] UnitFilesChanged signal received");
            changed = refresh_all_enabled_states(&services, &conn).await;
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

    let mut svc = services.lock_or_recover();

    let Some(service) = svc.get_mut(&urn_id) else {
        return false;
    };

    let mut changed = false;

    if let Some(active_val) = props.get("ActiveState")
        && let Ok(active_state) = String::try_from(active_val.clone())
            && service.active_state != active_state {
                log::info!(
                    "[systemd] {} active_state: {} -> {}",
                    unit_name, service.active_state, active_state,
                );
                service.active_state = active_state;
                changed = true;
            }

    if let Some(sub_val) = props.get("SubState")
        && let Ok(sub_state) = String::try_from(sub_val.clone())
            && service.sub_state != sub_state {
                log::debug!(
                    "[systemd] {} sub_state: {} -> {}",
                    unit_name, service.sub_state, sub_state,
                );
                service.sub_state = sub_state;
                changed = true;
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
        let svc = services.lock_or_recover();
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

    let mut services = services.lock_or_recover();
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

    /// Helper: simulates the UnitRemoved in-place update logic from monitor_service_signals().
    /// Returns (changed, service_still_in_map).
    fn simulate_unit_removed(
        services: &mut HashMap<String, UserService>,
        unit_name: &str,
    ) -> (bool, bool) {
        let urn_id = unit_to_urn_id(unit_name);
        let mut changed = false;
        if let Some(service) = services.get_mut(&urn_id) {
            if service.active_state != "inactive" || service.sub_state != "dead" {
                service.active_state = "inactive".to_string();
                service.sub_state = "dead".to_string();
                changed = true;
            }
        }
        let still_present = services.contains_key(&urn_id);
        (changed, still_present)
    }

    /// Helper: simulates the refresh_service() GetUnit failure in-place update logic.
    /// Returns whether the service was considered changed.
    fn simulate_refresh_failure_update(
        services: &mut HashMap<String, UserService>,
        unit: &str,
        enabled: bool,
    ) -> bool {
        let urn_id = unit_to_urn_id(unit);
        if let Some(svc) = services.get_mut(&urn_id) {
            let was_different = svc.active_state != "inactive"
                || svc.sub_state != "dead"
                || svc.enabled != enabled;
            svc.active_state = "inactive".to_string();
            svc.sub_state = "dead".to_string();
            svc.enabled = enabled;
            return was_different;
        }
        false
    }

    fn make_service(active_state: &str, sub_state: &str, enabled: bool) -> UserService {
        UserService {
            unit: "test.service".to_string(),
            description: "Test service".to_string(),
            active_state: active_state.to_string(),
            enabled,
            sub_state: sub_state.to_string(),
        }
    }

    #[test]
    fn unit_removed_updates_active_service_in_place() {
        let mut services = HashMap::new();
        services.insert("test".to_string(), make_service("active", "running", true));

        let (changed, still_present) = simulate_unit_removed(&mut services, "test.service");

        assert!(changed, "should report changed when transitioning from active to inactive");
        assert!(still_present, "service must NOT be removed from the map");
        let svc = &services["test"];
        assert_eq!(svc.active_state, "inactive");
        assert_eq!(svc.sub_state, "dead");
        // enabled and description must be preserved
        assert!(svc.enabled);
        assert_eq!(svc.description, "Test service");
    }

    #[test]
    fn unit_removed_noop_for_already_inactive() {
        let mut services = HashMap::new();
        services.insert("test".to_string(), make_service("inactive", "dead", false));

        let (changed, still_present) = simulate_unit_removed(&mut services, "test.service");

        assert!(!changed, "should not report changed when already inactive/dead");
        assert!(still_present, "service must remain in the map");
    }

    #[test]
    fn unit_removed_ignores_unknown_service() {
        let mut services = HashMap::new();
        services.insert("known".to_string(), make_service("active", "running", true));

        let (changed, still_present) = simulate_unit_removed(&mut services, "unknown.service");

        assert!(!changed, "should not report changed for unknown service");
        assert!(!still_present, "unknown service should not appear in map");
        // known service must be untouched
        assert_eq!(services["known"].active_state, "active");
    }

    #[test]
    fn refresh_failure_updates_active_service_in_place() {
        let mut services = HashMap::new();
        services.insert("pipewire".to_string(), make_service("active", "running", true));

        let changed = simulate_refresh_failure_update(&mut services, "pipewire.service", false);

        assert!(changed, "should report changed when state differs");
        let svc = &services["pipewire"];
        assert_eq!(svc.active_state, "inactive");
        assert_eq!(svc.sub_state, "dead");
        assert!(!svc.enabled, "enabled should be updated to the queried value");
        assert_eq!(svc.description, "Test service", "description must be preserved");
    }

    #[test]
    fn refresh_failure_detects_enabled_change_only() {
        let mut services = HashMap::new();
        services.insert("test".to_string(), make_service("inactive", "dead", true));

        let changed = simulate_refresh_failure_update(&mut services, "test.service", false);

        assert!(changed, "should report changed when only enabled differs");
        assert!(!services["test"].enabled);
    }

    #[test]
    fn refresh_failure_noop_when_already_matching() {
        let mut services = HashMap::new();
        services.insert("test".to_string(), make_service("inactive", "dead", false));

        let changed = simulate_refresh_failure_update(&mut services, "test.service", false);

        assert!(!changed, "should not report changed when state already matches");
    }

    #[test]
    fn refresh_failure_returns_false_for_unknown_service() {
        let mut services = HashMap::new();

        let changed = simulate_refresh_failure_update(&mut services, "unknown.service", true);

        assert!(!changed, "should return false for unknown service");
        assert!(!services.contains_key("unknown"), "should not insert unknown service");
    }

    /// Helper: simulates the basename extraction + template filtering from list_unit_files().
    fn filter_unit_file_path(file_path: &str) -> Option<String> {
        let basename = file_path.rsplit('/').next().unwrap_or(file_path);
        if basename.contains('@') {
            return None;
        }
        Some(basename.to_string())
    }

    /// Helper: simulates the load_services() merge logic for unit files.
    /// Inserts new services with inactive/dead state, skips existing entries.
    /// Returns count of added services.
    fn simulate_unit_file_merge(
        services: &mut HashMap<String, UserService>,
        unit_files: &[(&str, &str)],
    ) -> usize {
        let mut added = 0;
        for (unit_name, file_state) in unit_files {
            let urn_id = unit_to_urn_id(unit_name);
            if services.contains_key(&urn_id) {
                continue;
            }
            services.insert(urn_id, UserService {
                unit: unit_name.to_string(),
                description: String::new(),
                active_state: "inactive".to_string(),
                sub_state: "dead".to_string(),
                enabled: unit_file_state_to_enabled(file_state),
            });
            added += 1;
        }
        added
    }

    /// Helper: simulates the refresh_all_enabled_states() update logic.
    /// Given a list of (urn_id, new_enabled) pairs, applies updates to the map.
    /// Returns true if any service changed.
    fn simulate_enabled_state_refresh(
        services: &mut HashMap<String, UserService>,
        updates: &[(&str, bool)],
    ) -> bool {
        let mut any_changed = false;
        for (urn_id, enabled) in updates {
            if let Some(service) = services.get_mut(*urn_id) {
                if service.enabled != *enabled {
                    service.enabled = *enabled;
                    any_changed = true;
                }
            }
        }
        any_changed
    }

    #[test]
    fn filter_unit_file_extracts_basename() {
        assert_eq!(
            filter_unit_file_path("/usr/lib/systemd/user/pipewire.service"),
            Some("pipewire.service".to_string()),
        );
    }

    #[test]
    fn filter_unit_file_handles_bare_name() {
        assert_eq!(
            filter_unit_file_path("pipewire.service"),
            Some("pipewire.service".to_string()),
        );
    }

    #[test]
    fn filter_unit_file_rejects_template_units() {
        assert_eq!(
            filter_unit_file_path("/usr/lib/systemd/user/dbus-broker@.service"),
            None,
        );
        assert_eq!(
            filter_unit_file_path("/usr/lib/systemd/user/app-flatpak-com.example@123.service"),
            None,
        );
    }

    #[test]
    fn unit_file_merge_inserts_new_services() {
        let mut services = HashMap::new();

        let added = simulate_unit_file_merge(
            &mut services,
            &[
                ("pipewire.service", "enabled"),
                ("wireplumber.service", "disabled"),
            ],
        );

        assert_eq!(added, 2);
        assert_eq!(services.len(), 2);
        let pw = &services["pipewire"];
        assert_eq!(pw.active_state, "inactive");
        assert_eq!(pw.sub_state, "dead");
        assert!(pw.enabled);
        assert!(pw.description.is_empty());

        let wp = &services["wireplumber"];
        assert!(!wp.enabled);
    }

    #[test]
    fn unit_file_merge_skips_existing_services() {
        let mut services = HashMap::new();
        services.insert("pipewire".to_string(), make_service("active", "running", true));

        let added = simulate_unit_file_merge(
            &mut services,
            &[
                ("pipewire.service", "disabled"),
                ("wireplumber.service", "enabled"),
            ],
        );

        assert_eq!(added, 1, "should only add wireplumber, not overwrite pipewire");
        assert_eq!(services.len(), 2);
        // pipewire must retain its original state
        assert_eq!(services["pipewire"].active_state, "active");
        assert!(services["pipewire"].enabled, "existing pipewire must not be overwritten");
    }

    #[test]
    fn enabled_state_refresh_updates_changed_services() {
        let mut services = HashMap::new();
        services.insert("pipewire".to_string(), make_service("active", "running", true));
        services.insert("wireplumber".to_string(), make_service("active", "running", false));

        let changed = simulate_enabled_state_refresh(
            &mut services,
            &[("pipewire", false), ("wireplumber", true)],
        );

        assert!(changed);
        assert!(!services["pipewire"].enabled);
        assert!(services["wireplumber"].enabled);
    }

    #[test]
    fn enabled_state_refresh_noop_when_unchanged() {
        let mut services = HashMap::new();
        services.insert("pipewire".to_string(), make_service("active", "running", true));

        let changed = simulate_enabled_state_refresh(
            &mut services,
            &[("pipewire", true)],
        );

        assert!(!changed, "should not report changed when enabled state is the same");
    }

    #[test]
    fn enabled_state_refresh_ignores_unknown_services() {
        let mut services = HashMap::new();
        services.insert("pipewire".to_string(), make_service("active", "running", true));

        let changed = simulate_enabled_state_refresh(
            &mut services,
            &[("unknown", false)],
        );

        assert!(!changed, "should not report changed for unknown service");
        assert!(!services.contains_key("unknown"));
    }

    /// Task 3 / Test 5.1: After the GetUnit-failure update path runs, the service
    /// remains in the map with active_state "inactive" and sub_state "dead".
    /// Uses the extracted pure function `simulate_refresh_failure_update`.
    #[test]
    fn refresh_service_preserves_on_get_unit_failure() {
        let mut services = HashMap::new();
        services.insert(
            "pipewire".to_string(),
            UserService {
                unit: "pipewire.service".to_string(),
                description: "PipeWire Multimedia Service".to_string(),
                active_state: "active".to_string(),
                enabled: true,
                sub_state: "running".to_string(),
            },
        );

        let changed = simulate_refresh_failure_update(&mut services, "pipewire.service", true);

        assert!(changed);
        assert!(services.contains_key("pipewire"), "service must remain in map");
        let svc = &services["pipewire"];
        assert_eq!(svc.active_state, "inactive");
        assert_eq!(svc.sub_state, "dead");
        assert_eq!(svc.unit, "pipewire.service", "unit must be preserved");
        assert_eq!(svc.description, "PipeWire Multimedia Service", "description must be preserved");
    }

    /// Task 3 / Test 5.2: Given an active service, simulating UnitRemoved sets
    /// active_state to "inactive" and sub_state to "dead" without removing the entry.
    /// Uses the extracted pure function `simulate_unit_removed`.
    #[test]
    fn unit_removed_preserves_tracked_service() {
        let mut services = HashMap::new();
        services.insert(
            "wireplumber".to_string(),
            UserService {
                unit: "wireplumber.service".to_string(),
                description: "WirePlumber Session Manager".to_string(),
                active_state: "active".to_string(),
                enabled: true,
                sub_state: "running".to_string(),
            },
        );

        let (changed, still_present) = simulate_unit_removed(&mut services, "wireplumber.service");

        assert!(changed, "active -> inactive should report changed");
        assert!(still_present, "service must NOT be removed from the map");
        let svc = &services["wireplumber"];
        assert_eq!(svc.active_state, "inactive");
        assert_eq!(svc.sub_state, "dead");
        assert_eq!(svc.unit, "wireplumber.service", "unit must be preserved");
        assert_eq!(svc.description, "WirePlumber Session Manager", "description must be preserved");
        assert!(svc.enabled, "enabled must be preserved");
    }

    /// Task 3 / Test 5.3: Calling enabled-state update with "disabled" sets enabled
    /// to false and returns true (changed). Calling again returns false (unchanged).
    /// Uses the extracted pure function `simulate_enabled_state_refresh`.
    #[test]
    fn unit_file_state_refresh_updates_enabled() {
        let mut services = HashMap::new();
        services.insert("xdg-desktop-portal".to_string(), UserService {
            unit: "xdg-desktop-portal.service".to_string(),
            description: "Portal service".to_string(),
            active_state: "active".to_string(),
            enabled: true,
            sub_state: "running".to_string(),
        });

        // First call: enabled true -> false should report changed
        let changed = simulate_enabled_state_refresh(
            &mut services,
            &[("xdg-desktop-portal", false)],
        );
        assert!(changed, "enabled true->false should report changed");
        assert!(!services["xdg-desktop-portal"].enabled);

        // Second call: enabled false -> false should report no change
        let changed = simulate_enabled_state_refresh(
            &mut services,
            &[("xdg-desktop-portal", false)],
        );
        assert!(!changed, "same enabled state should not report changed");
    }
}

fn main() -> Result<()> {
    PluginRunner::new(
        "systemd",
        &[
            entity::session::SESSION_ENTITY_TYPE,
            entity::session::USER_SERVICE_ENTITY_TYPE,
        ],
    )
    .i18n(i18n(), "plugin-name", "plugin-description")
    .run(|notifier| async move {
        let plugin = SystemdPlugin::new().await?;

        let services = plugin.services.clone();
        let session_conn = plugin.session_conn.clone();

        // Spawn signal monitoring task
        spawn_monitored_anyhow(
            "systemd",
            monitor_service_signals(session_conn, services, notifier),
        );

        Ok(plugin)
    })
}
