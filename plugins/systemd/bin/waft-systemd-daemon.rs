//! Systemd daemon -- system power, session management, user service monitoring,
//! and user timer management.
//!
//! Provides a session entity with the current user's name and display,
//! user-service entities for systemd user services, and user-timer entities
//! for systemd user timers defined in `~/.config/systemd/user/`.
//! Handles power and session actions via D-Bus calls to systemd-logind.
//! Monitors user services and timers via D-Bus on the session bus.
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
//! User timer actions:
//! - `enable` - Enable a user timer
//! - `disable` - Disable a user timer
//! - `start` - Run the timer's associated service now
//! - `stop` - Stop the timer's associated service
//! - `create` - Create a new timer from JSON params
//! - `update` - Update an existing timer's unit files
//! - `delete` - Stop, disable, and remove timer unit files
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "systemd"
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use waft_plugin::*;
use waft_protocol::entity::session::{RestartPolicy, ScheduleKind, UserService, UserTimer};
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use waft_plugin::StateLocker;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/systemd.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/systemd.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

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
/// "enabled", "enabled-runtime", and "transient" are considered enabled.
fn unit_file_state_to_enabled(state: &str) -> bool {
    matches!(state, "enabled" | "enabled-runtime" | "transient")
}

/// Systemd plugin.
///
/// Provides session entity (stateless), user service entities (stateful),
/// and user timer entities (stateful).
/// Actions dispatch D-Bus calls to login1 (system bus) and systemd1 (session bus).
struct SystemdPlugin {
    system_conn: Connection,
    session_conn: Connection,
    session_path: String,
    user_name: Option<String>,
    screen_name: Option<String>,
    services: Arc<StdMutex<HashMap<String, UserService>>>,
    timers: Arc<StdMutex<HashMap<String, UserTimer>>>,
    notifier: EntityNotifier,
}

impl SystemdPlugin {
    async fn new(notifier: EntityNotifier) -> Result<Self> {
        let system_conn = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        let session_conn = Connection::session()
            .await
            .context("Failed to connect to session D-Bus")?;

        let session_path = get_session_path();
        log::info!("[systemd] Using session path: {}", session_path);

        let services = Arc::new(StdMutex::new(HashMap::new()));
        let timers = Arc::new(StdMutex::new(HashMap::new()));

        let mut plugin = Self {
            system_conn,
            session_conn,
            session_path,
            user_name: get_user_name(),
            screen_name: get_screen_name(),
            services,
            timers,
            notifier,
        };

        plugin.load_services().await;
        plugin.load_timers().await;

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

    /// Load user timers from `~/.config/systemd/user/*.timer` and query D-Bus for status.
    async fn load_timers(&mut self) {
        match scan_user_timers(&self.session_conn).await {
            Ok(timer_list) => {
                let mut timers = self.timers.lock_or_recover();
                for (urn_id, timer) in timer_list {
                    timers.insert(urn_id, timer);
                }
                log::info!("[systemd] Loaded {} user timers", timers.len());
            }
            Err(e) => {
                log::warn!("[systemd] Failed to scan user timers: {e}");
            }
        }
    }
}

/// Get the user systemd unit directory (`~/.config/systemd/user/`).
fn user_unit_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("systemd").join("user"))
}

/// Parse a systemd unit file into sections with key-value pairs.
/// Keys can appear multiple times, so values are stored as `Vec<String>`.
fn parse_unit_file(content: &str) -> HashMap<String, HashMap<String, Vec<String>>> {
    let mut sections: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            continue;
        }
        if current_section.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current_section.clone())
                .or_default()
                .entry(key.trim().to_string())
                .or_default()
                .push(value.trim().to_string());
        }
    }

    sections
}

/// Get the first value for a key in a parsed unit file section.
fn section_get<'a>(
    sections: &'a HashMap<String, HashMap<String, Vec<String>>>,
    section: &str,
    key: &str,
) -> Option<&'a str> {
    sections
        .get(section)?
        .get(key)?
        .first()
        .map(|s| s.as_str())
}

/// Get all values for a key in a parsed unit file section.
fn section_get_all<'a>(
    sections: &'a HashMap<String, HashMap<String, Vec<String>>>,
    section: &str,
    key: &str,
) -> Vec<&'a str> {
    sections
        .get(section)
        .and_then(|s| s.get(key))
        .map(|v| v.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default()
}

/// Parse a duration string like "300s", "5m", "1h" into seconds.
fn parse_duration_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix('s') {
        n.trim().parse().ok()
    } else if let Some(n) = s.strip_suffix('m') {
        n.trim().parse::<u64>().ok().map(|v| v * 60)
    } else if let Some(n) = s.strip_suffix('h') {
        n.trim().parse::<u64>().ok().map(|v| v * 3600)
    } else {
        // Bare number treated as seconds
        s.parse().ok()
    }
}

/// Parse the schedule from a timer unit file's [Timer] section.
fn parse_schedule(sections: &HashMap<String, HashMap<String, Vec<String>>>) -> ScheduleKind {
    if let Some(spec) = section_get(sections, "Timer", "OnCalendar") {
        let persistent = section_get(sections, "Timer", "Persistent")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "yes")
            .unwrap_or(false);
        ScheduleKind::Calendar {
            spec: spec.to_string(),
            persistent,
        }
    } else {
        ScheduleKind::Relative {
            on_boot_sec: section_get(sections, "Timer", "OnBootSec")
                .and_then(parse_duration_secs),
            on_startup_sec: section_get(sections, "Timer", "OnStartupSec")
                .and_then(parse_duration_secs),
            on_unit_active_sec: section_get(sections, "Timer", "OnUnitActiveSec")
                .and_then(parse_duration_secs),
        }
    }
}

/// Parse environment directives from a service file.
/// Handles `Environment="KEY=val"` and `Environment=KEY=val`.
fn parse_environment(values: &[&str]) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for val in values {
        // Strip surrounding quotes if present
        let val = val.trim_matches('"');
        if let Some((k, v)) = val.split_once('=') {
            env.push((k.to_string(), v.to_string()));
        }
    }
    env
}

/// Parse a `RestartPolicy` from a string value.
fn parse_restart_policy(s: &str) -> RestartPolicy {
    match s {
        "on-failure" => RestartPolicy::OnFailure,
        "always" => RestartPolicy::Always,
        _ => RestartPolicy::No,
    }
}

/// Enumerate all user timer units via D-Bus ListUnits and build `UserTimer` entities.
///
/// Uses systemd's D-Bus API instead of reading `~/.config/systemd/user/` directly,
/// so transient timers (in /run/user/) and system-user timers are all included.
async fn scan_user_timers(conn: &Connection) -> Result<Vec<(String, UserTimer)>> {
    let manager_proxy = zbus::Proxy::new(
        conn,
        SYSTEMD1_DESTINATION,
        SYSTEMD1_MANAGER_PATH,
        SYSTEMD1_MANAGER_INTERFACE,
    )
    .await
    .context("Failed to create systemd manager proxy for timer scan")?;

    // ListUnits returns an array of (name, description, load_state, active_state,
    // sub_state, following, object_path, job_id, job_type, job_object_path).
    type UnitRow = (
        String, String, String, String, String, String,
        zbus::zvariant::OwnedObjectPath, u32, String,
        zbus::zvariant::OwnedObjectPath,
    );
    let units: Vec<UnitRow> = manager_proxy
        .call("ListUnits", &())
        .await
        .context("Failed to call ListUnits")?;

    let mut timers = Vec::new();

    for (unit_name, description, _load, active_state, _sub, _following, _obj, _job_id, _job_type, _job_obj) in units {
        let name = match unit_name.strip_suffix(".timer") {
            Some(n) => n.to_string(),
            None => continue,
        };

        let active = active_state == "active";

        // Enabled state
        let enabled = {
            let state: Result<String, _> = manager_proxy
                .call("GetUnitFileState", &(unit_name.as_str(),))
                .await;
            state.map(|s| unit_file_state_to_enabled(&s)).unwrap_or(false)
        };

        // Get unit object path to read FragmentPath and timer properties
        let timer_path: Result<zbus::zvariant::OwnedObjectPath, _> =
            manager_proxy.call("GetUnit", &(unit_name.as_str(),)).await;

        let (last_trigger, next_elapse, fragment_path) = match timer_path {
            Ok(path) => {
                let fragment_path = if let Ok(unit_proxy) = zbus::Proxy::new(
                    conn, SYSTEMD1_DESTINATION, path.as_str(), "org.freedesktop.systemd1.Unit",
                ).await {
                    unit_proxy.get_property::<String>("FragmentPath").await.ok()
                        .filter(|s| !s.is_empty())
                } else {
                    None
                };

                let (last_trigger, next_elapse) = if let Ok(timer_iface) = zbus::Proxy::new(
                    conn, SYSTEMD1_DESTINATION, path.as_str(), "org.freedesktop.systemd1.Timer",
                ).await {
                    let last = timer_iface.get_property::<u64>("LastTriggerUSec").await.ok()
                        .and_then(|u| if u == 0 { None } else { Some((u / 1_000_000) as i64) });
                    let next = timer_iface.get_property::<u64>("NextElapseUSecRealtime").await.ok()
                        .and_then(|u| if u == 0 { None } else { Some((u / 1_000_000) as i64) });
                    (last, next)
                } else {
                    (None, None)
                };

                (last_trigger, next_elapse, fragment_path)
            }
            Err(_) => (None, None, None),
        };

        // Read unit file content for schedule/service config
        let (schedule, command, working_directory, environment, after, restart, cpu_quota, memory_limit) =
            if let Some(ref fp) = fragment_path {
                read_timer_unit_content(fp, &name).await
            } else {
                (
                    ScheduleKind::Calendar { spec: String::new(), persistent: false },
                    String::new(), None, Vec::new(), Vec::new(), RestartPolicy::No, None, None,
                )
            };

        // Get last exit code from paired service
        let last_exit_code = {
            let svc_unit = format!("{name}.service");
            let svc_path: Result<zbus::zvariant::OwnedObjectPath, _> =
                manager_proxy.call("GetUnit", &(svc_unit.as_str(),)).await;
            match svc_path {
                Ok(p) => match zbus::Proxy::new(conn, SYSTEMD1_DESTINATION, p.as_str(), "org.freedesktop.systemd1.Service").await {
                    Ok(proxy) => proxy.get_property::<i32>("ExecMainStatus").await.ok(),
                    Err(_) => None,
                },
                Err(_) => None,
            }
        };

        timers.push((name.clone(), UserTimer {
            name,
            description,
            enabled,
            active,
            schedule,
            last_trigger,
            next_elapse,
            last_exit_code,
            command,
            working_directory,
            environment,
            after,
            restart,
            cpu_quota,
            memory_limit,
        }));
    }

    Ok(timers)
}

/// Read schedule and service configuration from a timer's unit file and its paired .service.
async fn read_timer_unit_content(
    fragment_path: &str,
    name: &str,
) -> (ScheduleKind, String, Option<String>, Vec<(String, String)>, Vec<String>, RestartPolicy, Option<String>, Option<String>) {
    let empty = || (
        ScheduleKind::Calendar { spec: String::new(), persistent: false },
        String::new(), None, Vec::new(), Vec::new(), RestartPolicy::No, None, None,
    );

    let timer_content = match tokio::fs::read_to_string(fragment_path).await {
        Ok(c) => c,
        Err(e) => {
            log::debug!("[systemd] Failed to read {fragment_path}: {e}");
            return empty();
        }
    };

    let timer_sections = parse_unit_file(&timer_content);
    let schedule = parse_schedule(&timer_sections);

    // Paired .service file lives in the same directory as the .timer file
    let service_path = std::path::Path::new(fragment_path)
        .parent()
        .map(|p| p.join(format!("{name}.service")));

    let (command, working_directory, environment, after, restart, cpu_quota, memory_limit) =
        match service_path.and_then(|p| Some(p)) {
            Some(sp) => match tokio::fs::read_to_string(&sp).await {
                Ok(content) => {
                    let s = parse_unit_file(&content);
                    (
                        section_get(&s, "Service", "ExecStart").unwrap_or("").to_string(),
                        section_get(&s, "Service", "WorkingDirectory").map(|v| v.to_string()),
                        parse_environment(&section_get_all(&s, "Service", "Environment")),
                        section_get_all(&s, "Unit", "After")
                            .iter().flat_map(|v| v.split_whitespace()).map(|v| v.to_string()).collect(),
                        section_get(&s, "Service", "Restart").map(parse_restart_policy).unwrap_or(RestartPolicy::No),
                        section_get(&s, "Service", "CPUQuota").map(|v| v.to_string()),
                        section_get(&s, "Service", "MemoryLimit").map(|v| v.to_string()),
                    )
                }
                Err(_) => (String::new(), None, Vec::new(), Vec::new(), RestartPolicy::No, None, None),
            },
            None => (String::new(), None, Vec::new(), Vec::new(), RestartPolicy::No, None, None),
        };

    (schedule, command, working_directory, environment, after, restart, cpu_quota, memory_limit)
}

/// Query D-Bus for a timer's runtime state: active, enabled, last trigger, next elapse, exit code.
async fn query_timer_dbus_state(
    conn: &Connection,
    name: &str,
    timer_unit: &str,
) -> (bool, bool, Option<i64>, Option<i64>, Option<i32>) {
    let manager_proxy = match zbus::Proxy::new(
        conn,
        SYSTEMD1_DESTINATION,
        SYSTEMD1_MANAGER_PATH,
        SYSTEMD1_MANAGER_INTERFACE,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            log::debug!("[systemd] Failed to create proxy for timer {name}: {e}");
            return (false, false, None, None, None);
        }
    };

    // Check enabled state
    let enabled = {
        let state: Result<String, _> = manager_proxy
            .call("GetUnitFileState", &(timer_unit,))
            .await;
        match state {
            Ok(s) => unit_file_state_to_enabled(&s),
            Err(_) => false,
        }
    };

    // Get timer unit object path
    let timer_path: Result<zbus::zvariant::OwnedObjectPath, _> =
        manager_proxy.call("GetUnit", &(timer_unit,)).await;

    let (active, last_trigger, next_elapse) = match timer_path {
        Ok(path) => {
            let timer_proxy = match zbus::Proxy::new(
                conn,
                SYSTEMD1_DESTINATION,
                path.as_str(),
                "org.freedesktop.systemd1.Unit",
            )
            .await
            {
                Ok(p) => p,
                Err(_) => return (false, enabled, None, None, None),
            };

            let active_state: String = timer_proxy
                .get_property("ActiveState")
                .await
                .unwrap_or_else(|_| "inactive".to_string());
            let active = active_state == "active";

            // Read timer-specific properties from the Timer interface
            let timer_iface_proxy = match zbus::Proxy::new(
                conn,
                SYSTEMD1_DESTINATION,
                path.as_str(),
                "org.freedesktop.systemd1.Timer",
            )
            .await
            {
                Ok(p) => p,
                Err(_) => return (active, enabled, None, None, None),
            };

            let last_trigger = timer_iface_proxy
                .get_property::<u64>("LastTriggerUSec")
                .await
                .ok()
                .and_then(|usec| {
                    if usec == 0 {
                        None
                    } else {
                        Some((usec / 1_000_000) as i64)
                    }
                });

            let next_elapse = timer_iface_proxy
                .get_property::<u64>("NextElapseUSecRealtime")
                .await
                .ok()
                .and_then(|usec| {
                    if usec == 0 {
                        None
                    } else {
                        Some((usec / 1_000_000) as i64)
                    }
                });

            (active, last_trigger, next_elapse)
        }
        Err(_) => (false, None, None),
    };

    // Get the service's last exit code
    let service_unit = format!("{name}.service");
    let svc_path_result: Result<zbus::zvariant::OwnedObjectPath, _> =
        manager_proxy.call("GetUnit", &(&service_unit,)).await;
    let last_exit_code = match svc_path_result {
        Ok(svc_path) => {
            match zbus::Proxy::new(
                conn,
                SYSTEMD1_DESTINATION,
                svc_path.as_str(),
                "org.freedesktop.systemd1.Service",
            )
            .await
            {
                Ok(p) => p.get_property::<i32>("ExecMainStatus").await.ok(),
                Err(_) => None,
            }
        }
        Err(_) => None,
    };

    (active, enabled, last_trigger, next_elapse, last_exit_code)
}

/// Write timer and service unit files to `~/.config/systemd/user/`.
/// Validate an OnCalendar= expression using `systemd-analyze calendar`.
///
/// Returns `Ok(())` if valid, `Err(...)` with systemd's error message if not.
async fn validate_calendar_spec(spec: &str) -> Result<()> {
    let output = tokio::process::Command::new("systemd-analyze")
        .args(["calendar", spec])
        .output()
        .await
        .context("Failed to run systemd-analyze calendar")?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if !stderr.is_empty() { stderr } else { stdout };
        Err(anyhow::anyhow!("Invalid OnCalendar expression {:?}: {}", spec, msg))
    }
}

async fn write_timer_unit_files(name: &str, timer: &UserTimer) -> Result<()> {
    // Validate calendar spec before touching the filesystem
    if let waft_protocol::entity::session::ScheduleKind::Calendar { ref spec, .. } = timer.schedule {
        validate_calendar_spec(spec).await?;
    }

    let unit_dir = user_unit_dir().context("Cannot determine config directory")?;
    tokio::fs::create_dir_all(&unit_dir)
        .await
        .with_context(|| format!("Failed to create {}", unit_dir.display()))?;

    // Write .timer file
    let timer_content = build_timer_unit_content(timer);
    let timer_path = unit_dir.join(format!("{name}.timer"));
    tokio::fs::write(&timer_path, &timer_content)
        .await
        .with_context(|| format!("Failed to write {}", timer_path.display()))?;

    // Write .service file
    let service_content = build_service_unit_content(timer);
    let service_path = unit_dir.join(format!("{name}.service"));
    tokio::fs::write(&service_path, &service_content)
        .await
        .with_context(|| format!("Failed to write {}", service_path.display()))?;

    // Reload systemd
    daemon_reload().await?;

    Ok(())
}

/// Build the content of a .timer unit file.
fn build_timer_unit_content(timer: &UserTimer) -> String {
    let mut content = format!("[Unit]\nDescription={}\n\n[Timer]\n", timer.description);

    match &timer.schedule {
        ScheduleKind::Calendar { spec, persistent } => {
            content.push_str(&format!("OnCalendar={spec}\n"));
            content.push_str(&format!(
                "Persistent={}\n",
                if *persistent { "true" } else { "false" }
            ));
        }
        ScheduleKind::Relative {
            on_boot_sec,
            on_startup_sec,
            on_unit_active_sec,
        } => {
            if let Some(secs) = on_boot_sec {
                content.push_str(&format!("OnBootSec={secs}s\n"));
            }
            if let Some(secs) = on_startup_sec {
                content.push_str(&format!("OnStartupSec={secs}s\n"));
            }
            if let Some(secs) = on_unit_active_sec {
                content.push_str(&format!("OnUnitActiveSec={secs}s\n"));
            }
        }
    }

    content.push_str("\n[Install]\nWantedBy=timers.target\n");
    content
}

/// Build the content of a .service unit file.
fn build_service_unit_content(timer: &UserTimer) -> String {
    let mut content = format!(
        "[Unit]\nDescription={} (service)\n",
        timer.description
    );

    if !timer.after.is_empty() {
        content.push_str(&format!("After={}\n", timer.after.join(" ")));
    }

    content.push_str(&format!(
        "\n[Service]\nType=oneshot\nExecStart={}\n",
        timer.command
    ));

    if let Some(dir) = &timer.working_directory {
        content.push_str(&format!("WorkingDirectory={dir}\n"));
    }

    for (key, val) in &timer.environment {
        content.push_str(&format!("Environment=\"{key}={val}\"\n"));
    }

    let restart_str = match timer.restart {
        RestartPolicy::No => "no",
        RestartPolicy::OnFailure => "on-failure",
        RestartPolicy::Always => "always",
    };
    content.push_str(&format!("Restart={restart_str}\n"));

    if let Some(quota) = &timer.cpu_quota {
        content.push_str(&format!("CPUQuota={quota}\n"));
    }
    if let Some(limit) = &timer.memory_limit {
        content.push_str(&format!("MemoryLimit={limit}\n"));
    }

    content
}

/// Run `systemctl --user daemon-reload`.
async fn daemon_reload() -> Result<()> {
    let status = tokio::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .await
        .context("Failed to run systemctl --user daemon-reload")?;

    if !status.success() {
        anyhow::bail!("systemctl --user daemon-reload failed with {status}");
    }
    Ok(())
}

/// Run a systemctl --user command (enable/disable/start/stop) on a unit.
async fn systemctl_user(action: &str, unit: &str) -> Result<()> {
    let status = tokio::process::Command::new("systemctl")
        .args(["--user", action, unit])
        .status()
        .await
        .with_context(|| format!("Failed to run systemctl --user {action} {unit}"))?;

    if !status.success() {
        anyhow::bail!("systemctl --user {action} {unit} failed with {status}");
    }
    Ok(())
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

        let timers = self.timers.lock_or_recover();

        for (urn_id, timer) in timers.iter() {
            entities.push(Entity::new(
                Urn::new(
                    "systemd",
                    entity::session::USER_TIMER_ENTITY_TYPE,
                    urn_id,
                ),
                entity::session::USER_TIMER_ENTITY_TYPE,
                timer,
            ));
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
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
        } else if entity_type == entity::session::USER_TIMER_ENTITY_TYPE {
            let name = urn.id().to_string();
            let timer_unit = format!("{name}.timer");
            let service_unit = format!("{name}.service");

            match action.as_str() {
                "enable" => {
                    systemctl_user("enable", &timer_unit).await?;
                    log::info!("[systemd] Enabled timer {timer_unit}");
                }
                "disable" => {
                    systemctl_user("disable", &timer_unit).await?;
                    log::info!("[systemd] Disabled timer {timer_unit}");
                }
                "start" => {
                    // Use --no-block so systemctl returns immediately after submitting the
                    // job to systemd. Without this, systemctl blocks until the oneshot
                    // service process exits, which causes the 5-second action timeout to
                    // fire even on successful runs. The last_exit_code is picked up on the
                    // next D-Bus state refresh cycle.
                    let status = tokio::process::Command::new("systemctl")
                        .args(["--user", "--no-block", "start", &service_unit])
                        .status()
                        .await
                        .with_context(|| format!("Failed to run systemctl --user --no-block start {service_unit}"))?;
                    if !status.success() {
                        return Err(anyhow::anyhow!("systemctl --user --no-block start {service_unit} failed with {status}").into());
                    }
                    log::info!("[systemd] Started service {service_unit} (no-block)");
                }
                "stop" => {
                    systemctl_user("stop", &service_unit).await?;
                    log::info!("[systemd] Stopped service {service_unit}");
                }
                "delete" => {
                    // Stop and disable first, ignoring errors
                    let _ = systemctl_user("stop", &service_unit).await;
                    let _ = systemctl_user("stop", &timer_unit).await;
                    let _ = systemctl_user("disable", &timer_unit).await;

                    // Remove unit files
                    if let Some(unit_dir) = user_unit_dir() {
                        let timer_path = unit_dir.join(&timer_unit);
                        let service_path = unit_dir.join(&service_unit);
                        if timer_path.exists() {
                            tokio::fs::remove_file(&timer_path).await?;
                        }
                        if service_path.exists() {
                            tokio::fs::remove_file(&service_path).await?;
                        }
                    }

                    daemon_reload().await?;

                    // Remove from state
                    self.timers.lock_or_recover().remove(&name);
                    log::info!("[systemd] Deleted timer {name}");
                }
                "create" => {
                    let timer: UserTimer = serde_json::from_value(params)
                        .context("Invalid create params")?;
                    write_timer_unit_files(&timer.name, &timer).await?;
                    systemctl_user("enable", &format!("{}.timer", timer.name)).await?;
                    // Start the timer unit so it begins tracking the next elapse immediately.
                    // The timer unit (not the service) is started here — it arms the schedule
                    // without running the command. The service is only started via "start" action.
                    systemctl_user("start", &format!("{}.timer", timer.name)).await?;

                    let timer_name = timer.name.clone();
                    self.timers.lock_or_recover().insert(timer_name, timer);
                    log::info!("[systemd] Created timer {name}");
                }
                "update" => {
                    let timer: UserTimer = serde_json::from_value(params)
                        .context("Invalid update params")?;
                    write_timer_unit_files(&name, &timer).await?;

                    self.timers.lock_or_recover().insert(name.clone(), timer);
                    log::info!("[systemd] Updated timer {name}");
                }
                other => log::warn!("[systemd] Unknown user-timer action: {other}"),
            }

            // Refresh timer state from D-Bus after actions
            if action != "delete" {
                let (active, enabled, last_trigger, next_elapse, last_exit_code) =
                    query_timer_dbus_state(&self.session_conn, &name, &timer_unit).await;
                let mut timers = self.timers.lock_or_recover();
                if let Some(t) = timers.get_mut(&name) {
                    t.active = active;
                    t.enabled = enabled;
                    t.last_trigger = last_trigger;
                    t.next_elapse = next_elapse;
                    t.last_exit_code = last_exit_code;
                }
            }

            // Push updated entity state to subscribed apps immediately.
            // Without this, the UI waits for the next 5-second polling cycle.
            self.notifier.notify();
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

/// Periodically re-scan user timers from disk and D-Bus, emitting updates on changes.
async fn monitor_timer_files(
    conn: Connection,
    timers: Arc<StdMutex<HashMap<String, UserTimer>>>,
    notifier: EntityNotifier,
) -> Result<()> {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let new_timers = match scan_user_timers(&conn).await {
            Ok(t) => t,
            Err(e) => {
                log::debug!("[systemd] Timer re-scan failed: {e}");
                continue;
            }
        };

        let new_map: HashMap<String, UserTimer> = new_timers.into_iter().collect();
        let mut current = timers.lock_or_recover();

        // Detect changes: added, modified, or removed timers
        let mut changed = false;

        // Check for removed timers
        let old_keys: Vec<String> = current.keys().cloned().collect();
        for key in &old_keys {
            if !new_map.contains_key(key) {
                current.remove(key);
                changed = true;
                log::info!("[systemd] Timer removed from disk: {key}");
            }
        }

        // Check for added or modified timers
        for (key, new_timer) in new_map {
            match current.get(&key) {
                Some(existing) if existing == &new_timer => {}
                _ => {
                    current.insert(key, new_timer);
                    changed = true;
                }
            }
        }

        drop(current);

        if changed {
            notifier.notify();
        }
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

    // --- Timer helper tests ---

    #[test]
    fn parse_unit_file_basic() {
        let content = "\
[Unit]
Description=My timer

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
";
        let sections = parse_unit_file(content);
        assert_eq!(section_get(&sections, "Unit", "Description"), Some("My timer"));
        assert_eq!(section_get(&sections, "Timer", "OnCalendar"), Some("daily"));
        assert_eq!(section_get(&sections, "Timer", "Persistent"), Some("true"));
        assert_eq!(section_get(&sections, "Install", "WantedBy"), Some("timers.target"));
    }

    #[test]
    fn parse_unit_file_ignores_comments() {
        let content = "\
[Unit]
# This is a comment
Description=Test
; Another comment
";
        let sections = parse_unit_file(content);
        assert_eq!(section_get(&sections, "Unit", "Description"), Some("Test"));
    }

    #[test]
    fn parse_unit_file_multiple_values() {
        let content = "\
[Service]
Environment=\"FOO=bar\"
Environment=\"BAZ=qux\"
";
        let sections = parse_unit_file(content);
        let envs = section_get_all(&sections, "Service", "Environment");
        assert_eq!(envs.len(), 2);
        assert_eq!(envs[0], "\"FOO=bar\"");
        assert_eq!(envs[1], "\"BAZ=qux\"");
    }

    #[test]
    fn parse_duration_secs_various() {
        assert_eq!(parse_duration_secs("300s"), Some(300));
        assert_eq!(parse_duration_secs("5m"), Some(300));
        assert_eq!(parse_duration_secs("1h"), Some(3600));
        assert_eq!(parse_duration_secs("42"), Some(42));
        assert_eq!(parse_duration_secs("abc"), None);
    }

    #[test]
    fn parse_schedule_calendar() {
        let content = "\
[Timer]
OnCalendar=*-*-* 03:00:00
Persistent=true
";
        let sections = parse_unit_file(content);
        let schedule = parse_schedule(&sections);
        assert_eq!(
            schedule,
            ScheduleKind::Calendar {
                spec: "*-*-* 03:00:00".to_string(),
                persistent: true,
            }
        );
    }

    #[test]
    fn parse_schedule_relative() {
        let content = "\
[Timer]
OnBootSec=300s
OnUnitActiveSec=1h
";
        let sections = parse_unit_file(content);
        let schedule = parse_schedule(&sections);
        assert_eq!(
            schedule,
            ScheduleKind::Relative {
                on_boot_sec: Some(300),
                on_startup_sec: None,
                on_unit_active_sec: Some(3600),
            }
        );
    }

    #[test]
    fn parse_environment_values() {
        let envs = parse_environment(&["\"FOO=bar\"", "BAZ=qux"]);
        assert_eq!(envs, vec![
            ("FOO".to_string(), "bar".to_string()),
            ("BAZ".to_string(), "qux".to_string()),
        ]);
    }

    #[test]
    fn parse_restart_policy_values() {
        assert_eq!(parse_restart_policy("no"), RestartPolicy::No);
        assert_eq!(parse_restart_policy("on-failure"), RestartPolicy::OnFailure);
        assert_eq!(parse_restart_policy("always"), RestartPolicy::Always);
        assert_eq!(parse_restart_policy("unknown"), RestartPolicy::No);
    }

    #[test]
    fn build_timer_unit_calendar() {
        let timer = UserTimer {
            name: "test".to_string(),
            description: "Test timer".to_string(),
            enabled: true,
            active: false,
            schedule: ScheduleKind::Calendar {
                spec: "daily".to_string(),
                persistent: true,
            },
            last_trigger: None,
            next_elapse: None,
            last_exit_code: None,
            command: "/bin/true".to_string(),
            working_directory: None,
            environment: vec![],
            after: vec![],
            restart: RestartPolicy::No,
            cpu_quota: None,
            memory_limit: None,
        };

        let content = build_timer_unit_content(&timer);
        assert!(content.contains("OnCalendar=daily"));
        assert!(content.contains("Persistent=true"));
        assert!(content.contains("Description=Test timer"));
        assert!(content.contains("WantedBy=timers.target"));
    }

    #[test]
    fn build_timer_unit_relative() {
        let timer = UserTimer {
            name: "test".to_string(),
            description: "Relative timer".to_string(),
            enabled: true,
            active: false,
            schedule: ScheduleKind::Relative {
                on_boot_sec: Some(60),
                on_startup_sec: None,
                on_unit_active_sec: Some(3600),
            },
            last_trigger: None,
            next_elapse: None,
            last_exit_code: None,
            command: "/bin/true".to_string(),
            working_directory: None,
            environment: vec![],
            after: vec![],
            restart: RestartPolicy::No,
            cpu_quota: None,
            memory_limit: None,
        };

        let content = build_timer_unit_content(&timer);
        assert!(content.contains("OnBootSec=60s"));
        assert!(content.contains("OnUnitActiveSec=3600s"));
        assert!(!content.contains("OnStartupSec"));
    }

    #[test]
    fn build_service_unit_content_full() {
        let timer = UserTimer {
            name: "backup".to_string(),
            description: "Daily backup".to_string(),
            enabled: true,
            active: false,
            schedule: ScheduleKind::Calendar {
                spec: "daily".to_string(),
                persistent: true,
            },
            last_trigger: None,
            next_elapse: None,
            last_exit_code: None,
            command: "/usr/bin/backup.sh".to_string(),
            working_directory: Some("/home/user".to_string()),
            environment: vec![("BACKUP_DIR".to_string(), "/mnt/backup".to_string())],
            after: vec!["network-online.target".to_string()],
            restart: RestartPolicy::OnFailure,
            cpu_quota: Some("50%".to_string()),
            memory_limit: Some("512M".to_string()),
        };

        let content = build_service_unit_content(&timer);
        assert!(content.contains("Description=Daily backup (service)"));
        assert!(content.contains("After=network-online.target"));
        assert!(content.contains("Type=oneshot"));
        assert!(content.contains("ExecStart=/usr/bin/backup.sh"));
        assert!(content.contains("WorkingDirectory=/home/user"));
        assert!(content.contains("Environment=\"BACKUP_DIR=/mnt/backup\""));
        assert!(content.contains("Restart=on-failure"));
        assert!(content.contains("CPUQuota=50%"));
        assert!(content.contains("MemoryLimit=512M"));
    }

    #[test]
    fn build_service_unit_content_minimal() {
        let timer = UserTimer {
            name: "simple".to_string(),
            description: "Simple".to_string(),
            enabled: false,
            active: false,
            schedule: ScheduleKind::Calendar {
                spec: "daily".to_string(),
                persistent: false,
            },
            last_trigger: None,
            next_elapse: None,
            last_exit_code: None,
            command: "/bin/true".to_string(),
            working_directory: None,
            environment: vec![],
            after: vec![],
            restart: RestartPolicy::No,
            cpu_quota: None,
            memory_limit: None,
        };

        let content = build_service_unit_content(&timer);
        assert!(content.contains("ExecStart=/bin/true"));
        assert!(content.contains("Restart=no"));
        assert!(!content.contains("WorkingDirectory"));
        assert!(!content.contains("Environment"));
        assert!(!content.contains("CPUQuota"));
        assert!(!content.contains("MemoryLimit"));
        assert!(!content.contains("After="));
    }
}

fn main() -> Result<()> {
    PluginRunner::new(
        "systemd",
        &[
            entity::session::SESSION_ENTITY_TYPE,
            entity::session::USER_SERVICE_ENTITY_TYPE,
            entity::session::USER_TIMER_ENTITY_TYPE,
        ],
    )
    .i18n(i18n(), "plugin-name", "plugin-description")
    .run(|notifier| async move {
        let plugin = SystemdPlugin::new(notifier.clone()).await?;

        let services = plugin.services.clone();
        let timers = plugin.timers.clone();
        let session_conn = plugin.session_conn.clone();

        // Spawn signal monitoring task for services
        spawn_monitored_anyhow(
            "systemd",
            monitor_service_signals(session_conn.clone(), services, notifier.clone()),
        );

        // Spawn timer file monitoring task
        spawn_monitored_anyhow(
            "systemd-timers",
            monitor_timer_files(session_conn, timers, notifier),
        );

        Ok(plugin)
    })
}
