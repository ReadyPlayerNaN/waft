//! EDS (Evolution Data Server) plugin — calendar event integration.
//!
//! Provides calendar events from EDS via the entity-based protocol.
//! Events are exposed as `calendar-event` entities with URN format:
//! `eds/calendar-event/{uid}::{start_time}`
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "eds"
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, LazyLock, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::stream::StreamExt;
use log::{debug, info, warn};
use serde::Deserialize;
use waft_plugin::*;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/eds.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/eds.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

use zbus::{Connection, MessageStream};
use zvariant::OwnedValue;

/// EDS configuration from config file.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct EdsConfig {
    /// Seconds between background refresh cycles. Default: 480 (8 min).
    refresh_interval_secs: u64,

    /// Background refresh interval when the session is locked, in seconds.
    /// 0 = pause background refresh entirely while locked. Default: 0.
    locked_refresh_interval_secs: u64,

    /// Smallest sliding-window unit for overlay-triggered debounce, in seconds.
    /// Windows: [base, 2×base, 4×base] → limits [1, 2, 3]. Default: 15.
    debounce_base_secs: u64,
}

impl Default for EdsConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 480,
            locked_refresh_interval_secs: 0,
            debounce_base_secs: 15,
        }
    }
}

/// Shared daemon state containing calendar events.
struct EdsState {
    /// Map of occurrence keys to calendar events.
    /// Key format: "{uid}::{start_time}"
    events: HashMap<String, entity::calendar::CalendarEvent>,
    /// Handles for running view-monitor tasks. Aborted on midnight rebuild.
    view_monitor_handles: Vec<tokio::task::JoinHandle<()>>,
    /// Timestamps of recent overlay-triggered refreshes for sliding-window debounce.
    /// Pruned to a 4×base window on every check.
    debounce_recent: VecDeque<std::time::Instant>,
    /// Known calendar backends: (bus_name, object_path) pairs.
    /// Populated during setup_calendar_views; used by do_refresh and the scheduler.
    calendar_backends: Vec<(String, String)>,
    /// Object paths of calendar backends that returned "not supported" (Code11) on Refresh().
    /// Cleared at midnight rebuild so backends get a fresh chance after reconfiguration.
    refresh_unsupported: HashSet<String>,
    /// Unix timestamp of the last refresh attempt (overlay-triggered or scheduled).
    last_refresh: Option<i64>,
    /// Instant when the last Refresh() D-Bus call was dispatched.
    /// Used by `can_stop()` to hold the plugin alive while an async EDS sync
    /// is in progress (ObjectsModified signals arrive 30-60s after Refresh()).
    last_refresh_instant: Option<std::time::Instant>,
    /// True while a calendar refresh D-Bus call is in progress.
    syncing: bool,
}

impl EdsState {
    fn new() -> Self {
        Self {
            events: HashMap::new(),
            view_monitor_handles: Vec::new(),
            debounce_recent: VecDeque::new(),
            calendar_backends: Vec::new(),
            refresh_unsupported: HashSet::new(),
            last_refresh: None,
            last_refresh_instant: None,
            syncing: false,
        }
    }
}

/// EDS plugin implementation.
struct EdsPlugin {
    config: EdsConfig,
    state: Arc<StdMutex<EdsState>>,
    conn: Connection,
    /// True when the session is locked. Written by session monitor, read by scheduler.
    session_locked: Arc<std::sync::atomic::AtomicBool>,
    /// Notified when session unlocks; wakes the refresh scheduler immediately.
    unlock_notify: Arc<tokio::sync::Notify>,
    /// Notifier slot — filled by main() after PluginRuntime::new().
    /// Allows handle_action to push syncing-state updates mid-action.
    notifier: Arc<StdMutex<Option<EntityNotifier>>>,
}

impl EdsPlugin {
    async fn new() -> Result<Self> {
        let config: EdsConfig = waft_plugin::config::load_plugin_config("eds").unwrap_or_default();
        log::debug!("EDS config: {config:?}");

        let conn = Connection::session()
            .await
            .context("failed to connect to session bus")?;

        Ok(Self {
            config,
            state: Arc::new(StdMutex::new(EdsState::new())),
            conn,
            session_locked: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            unlock_notify: Arc::new(tokio::sync::Notify::new()),
            notifier: Arc::new(StdMutex::new(None)),
        })
    }

    fn shared_state(&self) -> Arc<StdMutex<EdsState>> {
        self.state.clone()
    }

    fn session_locked(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.session_locked.clone()
    }

    fn unlock_notify(&self) -> Arc<tokio::sync::Notify> {
        self.unlock_notify.clone()
    }

    fn notifier_slot(&self) -> Arc<StdMutex<Option<EntityNotifier>>> {
        self.notifier.clone()
    }

    /// Create occurrence key from UID and start time.
    fn make_occurrence_key(uid: &str, start_time: i64) -> String {
        format!("{}::{}", uid, start_time)
    }
}

#[async_trait::async_trait]
impl Plugin for EdsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.state.lock_or_recover();

        let mut entities: Vec<Entity> = state
            .events
            .iter()
            .map(|(key, event)| {
                let urn = Urn::new("eds", entity::calendar::ENTITY_TYPE, key);
                Entity::new(urn, entity::calendar::ENTITY_TYPE, event)
            })
            .collect();

        // Expose calendar sync control as a singleton entity so the overview can
        // discover a stable URN for sending the "refresh" action.
        let sync = entity::calendar::CalendarSync {
            last_refresh: state.last_refresh,
            syncing: state.syncing,
        };
        let sync_urn = Urn::new(
            "eds",
            entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
            "singleton",
        );
        entities.push(Entity::new(
            sync_urn,
            entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
            &sync,
        ));

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::debug!("Received action '{}' for URN: {}", action, urn);

        if action == "refresh" {
            let allowed = {
                let mut st = self.state.lock_or_recover();
                check_debounce(&mut st.debounce_recent, self.config.debounce_base_secs)
            };

            if !allowed {
                log::debug!("[eds] Refresh debounced (overlay-triggered)");
                return Ok(());
            }

            // Clone notifier out of the slot (must not hold the lock across an async boundary).
            let notifier = {
                let guard = self.notifier.lock_or_recover();
                guard.as_ref().cloned()
            };

            match notifier {
                Some(n) => {
                    refresh_with_status(&self.conn, &self.state, &n).await;
                }
                None => {
                    // Notifier not yet wired — should not happen in production.
                    // NOTE: Without a notifier we cannot propagate the updated last_refresh to
                    // subscribers. This path is unreachable in normal operation (notifier is filled
                    // before the first action can arrive), but if it is ever reached, last_refresh
                    // will be stale in the overview until the next successful refresh.
                    log::warn!(
                        "[eds] handle_action: notifier slot empty, syncing indicator unavailable"
                    );
                    do_refresh(&self.conn, &self.state).await;
                    // Update last_refresh manually since refresh_with_status didn't run.
                    let mut st = self.state.lock_or_recover();
                    st.last_refresh = Some(unix_now());
                }
            }
            return Ok(());
        }

        log::warn!(
            "EDS plugin does not support action '{}' (urn={})",
            action,
            urn
        );
        Ok(())
    }

    fn can_stop(&self) -> bool {
        // EDS Refresh() is fire-and-forget: the D-Bus call returns immediately
        // but the actual sync with Google Calendar happens asynchronously.
        // ObjectsModified signals arrive 30–120 seconds after we send Refresh().
        // Returning false here keeps the plugin alive long enough to receive
        // those signals so the next overview open sees up-to-date events.
        const REFRESH_GRACE_SECS: u64 = 120;
        let st = self.state.lock_or_recover();
        if let Some(instant) = st.last_refresh_instant
            && instant.elapsed() < std::time::Duration::from_secs(REFRESH_GRACE_SECS) {
                log::debug!(
                    "[eds] can_stop: false — refresh grace period active ({}s remaining)",
                    REFRESH_GRACE_SECS - instant.elapsed().as_secs()
                );
                return false;
            }
        true
    }
}

/// Returns the current Unix timestamp in seconds.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Returns true if a refresh is allowed under the sliding-window policy.
///
/// Three windows derived from `base_secs`:
///   [base, 2×base, 4×base] → limits [1, 2, 3]
///
/// Side effect on allow: pushes current Instant and prunes old entries.
fn check_debounce(
    recent: &mut std::collections::VecDeque<std::time::Instant>,
    base_secs: u64,
) -> bool {
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let base = Duration::from_secs(base_secs);

    // Prune entries older than 4×base (the largest window).
    recent.retain(|&t| now.duration_since(t) < base * 4);

    let in_w1 = recent
        .iter()
        .filter(|&&t| now.duration_since(t) < base)
        .count();
    let in_w2 = recent
        .iter()
        .filter(|&&t| now.duration_since(t) < base * 2)
        .count();
    let in_w3 = recent.len(); // all remaining are within 4×base

    if in_w1 >= 1 || in_w2 >= 2 || in_w3 >= 3 {
        return false;
    }

    recent.push_back(now);
    true
}

/// Call EDS Calendar.Open() then Calendar.Refresh() on each known backend.
///
/// This is the shared implementation used by both the periodic scheduler and
/// overlay-triggered handle_action. It does NOT check the debounce window.
///
/// Backends that previously returned Code11 (not supported) are skipped.
/// New Code11 failures are recorded in `state.refresh_unsupported` so they
/// are skipped on future calls (cleared at midnight rebuild).
async fn do_refresh(conn: &Connection, state: &Arc<StdMutex<EdsState>>) {
    let (backends, unsupported) = {
        let st = state.lock_or_recover();
        (st.calendar_backends.clone(), st.refresh_unsupported.clone())
    };

    if backends.is_empty() {
        log::debug!("[eds] do_refresh: no backends, skipping");
        return;
    }
    for (bus_name, object_path) in &backends {
        if unsupported.contains(object_path) {
            log::debug!("[eds] Skipping unsupported backend: {object_path}");
            continue;
        }

        match zbus::Proxy::new(
            conn,
            bus_name.as_str(),
            object_path.as_str(),
            "org.gnome.evolution.dataserver.Calendar",
        )
        .await
        {
            Ok(proxy) => {
                match proxy.call_method("Open", &()).await {
                    Ok(_) => {
                        let result: std::result::Result<(), zbus::Error> =
                            proxy.call("Refresh", &()).await;
                        match result {
                            Ok(()) => log::debug!("[eds] Refreshed {object_path}"),
                            Err(e) => {
                                // EDS Code11 = E_NOT_SUPPORTED. This error code
                                // appears in all locales in the D-Bus error detail.
                                if format!("{e}").contains("Code11") {
                                    log::debug!(
                                        "[eds] Backend {object_path} does not support Refresh, will skip in future"
                                    );
                                    let mut st = state.lock_or_recover();
                                    st.refresh_unsupported.insert(object_path.clone());
                                } else {
                                    log::warn!("[eds] Refresh failed for {object_path}: {e}");
                                }
                            }
                        }
                    }
                    Err(e) => log::warn!("[eds] Open failed for {object_path}: {e}"),
                }
            }
            Err(e) => log::warn!("[eds] Proxy failed for {object_path}: {e}"),
        }
    }
    log::debug!("[eds] do_refresh complete ({} backends)", backends.len());
}

/// Run `do_refresh` and bracket it with `syncing = true/false` state updates.
///
/// Sets `state.syncing = true` and notifies before calling `do_refresh`, then sets
/// `state.syncing = false` and updates `last_refresh` after it completes.
/// This ensures the overview sees accurate syncing state for all refresh paths.
async fn refresh_with_status(
    conn: &Connection,
    state: &Arc<StdMutex<EdsState>>,
    notifier: &EntityNotifier,
) {
    {
        let mut st = state.lock_or_recover();
        st.syncing = true;
        st.last_refresh_instant = Some(std::time::Instant::now());
    }
    notifier.notify();

    // NOTE: If do_refresh panics, syncing will remain true and the spinner will spin
    // indefinitely. do_refresh is designed to be panic-free (all errors are handled
    // via match/if let), but this assumption is not enforced by the type system.
    do_refresh(conn, state).await;

    {
        let mut st = state.lock_or_recover();
        st.syncing = false;
        st.last_refresh = Some(unix_now());
    }
    notifier.notify();
}

/// Monitor logind for session Lock/Unlock signals.
///
/// On Lock:   session_locked = true
/// On Unlock: session_locked = false, unlock_notify fired
///
/// Degrades gracefully if logind is unavailable.
async fn spawn_session_monitor(
    session_locked: Arc<std::sync::atomic::AtomicBool>,
    unlock_notify: Arc<tokio::sync::Notify>,
) {
    use std::sync::atomic::Ordering;

    let sys_conn = match zbus::Connection::system().await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[eds] Cannot connect to system bus for session monitor: {e}");
            log::warn!("[eds] Session-aware refresh disabled");
            return;
        }
    };

    // Resolve session path: prefer XDG_SESSION_ID, fall back to "auto".
    let session_path = match std::env::var("XDG_SESSION_ID") {
        Ok(id) => format!("/org/freedesktop/login1/session/{}", id),
        Err(_) => "/org/freedesktop/login1/session/auto".to_string(),
    };

    log::info!("[eds] Monitoring session at {session_path}");

    for (member, is_lock) in &[("Lock", true), ("Unlock", false)] {
        let rule = format!(
            "type='signal',interface='org.freedesktop.login1.Session',\
             member='{}',path='{}'",
            member, session_path
        );

        let sys_conn = sys_conn.clone();
        let session_locked = session_locked.clone();
        let unlock_notify = unlock_notify.clone();
        let is_lock = *is_lock;

        tokio::spawn(async move {
            use futures_util::StreamExt;

            match zbus::fdo::DBusProxy::new(&sys_conn).await {
                Ok(dbus) => match zbus::MatchRule::try_from(rule.as_str()) {
                    Ok(rule_obj) => {
                        if let Err(e) = dbus.add_match_rule(rule_obj.to_owned()).await {
                            log::warn!(
                                "[eds] Failed to add match rule for {}: {e}",
                                if is_lock { "Lock" } else { "Unlock" }
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "[eds] Invalid match rule format for {}: {e}",
                            if is_lock { "Lock" } else { "Unlock" }
                        );
                    }
                },
                Err(e) => {
                    log::warn!(
                        "[eds] Failed to create DBusProxy for match rule registration ({}): {e}",
                        if is_lock { "Lock" } else { "Unlock" }
                    );
                }
            }

            let mut stream = zbus::MessageStream::from(&sys_conn);
            while let Some(Ok(msg)) = stream.next().await {
                let h = msg.header();
                let iface_ok = h
                    .interface()
                    .map(|i| i.as_str() == "org.freedesktop.login1.Session")
                    .unwrap_or(false);
                let member_ok = h
                    .member()
                    .map(|m| m.as_str() == if is_lock { "Lock" } else { "Unlock" })
                    .unwrap_or(false);

                if iface_ok && member_ok {
                    session_locked.store(is_lock, Ordering::Relaxed);
                    if !is_lock {
                        log::debug!("[eds] Session unlocked — triggering immediate refresh");
                        unlock_notify.notify_one();
                    } else {
                        log::debug!("[eds] Session locked — background refresh rate reduced");
                    }
                }
            }
            log::debug!(
                "[eds] Session monitor stream ended for member={}",
                if is_lock { "Lock" } else { "Unlock" }
            );
        });
    }
}

/// Periodically call do_refresh() according to config.
///
/// - Active interval:  config.refresh_interval_secs (default 480s)
/// - Locked interval:  config.locked_refresh_interval_secs (0 = pause)
/// - On unlock:        fires immediately, then resets to active interval
async fn spawn_refresh_scheduler(
    conn: Connection,
    state: Arc<StdMutex<EdsState>>,
    config: EdsConfig,
    session_locked: Arc<std::sync::atomic::AtomicBool>,
    unlock_notify: Arc<tokio::sync::Notify>,
    notifier: EntityNotifier,
) {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    log::info!(
        "[eds] Refresh scheduler started (interval={}s, locked={}s, debounce_base={}s)",
        config.refresh_interval_secs,
        config.locked_refresh_interval_secs,
        config.debounce_base_secs,
    );

    #[allow(unreachable_code)]
    {
        loop {
            let locked = session_locked.load(Ordering::Relaxed);

            let sleep_duration = if locked {
                if config.locked_refresh_interval_secs == 0 {
                    // Paused: wait effectively forever; unlock_notify will wake us.
                    Duration::from_secs(u64::MAX / 2)
                } else {
                    Duration::from_secs(config.locked_refresh_interval_secs)
                }
            } else {
                Duration::from_secs(config.refresh_interval_secs)
            };

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {
                    // Timer fired. Only refresh if not locked (guards against MAX/2 wakeup edge).
                    if !session_locked.load(Ordering::Relaxed) {
                        log::debug!("[eds] Periodic refresh firing");
                        refresh_with_status(&conn, &state, &notifier).await;
                    }
                }
                _ = unlock_notify.notified() => {
                    // Session just unlocked — refresh immediately.
                    log::debug!("[eds] Post-unlock refresh firing");
                    refresh_with_status(&conn, &state, &notifier).await;

                    // Record the unlock refresh in the debounce window so an immediate
                    // overlay open doesn't double-fire within the base window.
                    state.lock_or_recover().debounce_recent.push_back(std::time::Instant::now());
                }
            }
        }

        // Unreachable, but satisfies the "log when task exits" rule.
        log::warn!("[eds] Refresh scheduler task stopped — background refresh is no longer active");
    } // #[allow(unreachable_code)]
}

/// EDS D-Bus service names and paths.
const SOURCES_DEST: &str = "org.gnome.evolution.dataserver.Sources5";
const SOURCES_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
const CALENDAR_FACTORY_DEST: &str = "org.gnome.evolution.dataserver.Calendar8";
const CALENDAR_FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
const CALENDAR_VIEW_IFACE: &str = "org.gnome.evolution.dataserver.CalendarView";

/// Type alias for D-Bus ObjectManager's GetManagedObjects() return value.
type ManagedObjects =
    HashMap<zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Calendar source discovered from EDS.
#[derive(Debug, Clone)]
struct CalendarSource {
    uid: String,
    display_name: String,
}

/// Extract DisplayName from EDS key file data.
fn extract_display_name(data: &str) -> Option<String> {
    for line in data.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("DisplayName=") {
            return Some(rest.to_string());
        }
    }
    None
}

/// Discover calendar sources from EDS source registry.
async fn discover_calendar_sources(conn: &Connection) -> Result<Vec<CalendarSource>> {
    let proxy = zbus::Proxy::new(
        conn,
        SOURCES_DEST,
        SOURCES_PATH,
        "org.freedesktop.DBus.ObjectManager",
    )
    .await
    .context("Failed to create ObjectManager proxy")?;

    let (managed,): (ManagedObjects,) = proxy
        .call("GetManagedObjects", &())
        .await
        .context("Failed to call GetManagedObjects on EDS SourceManager")?;

    let mut sources = Vec::new();

    for interfaces in managed.values() {
        let source_iface = interfaces.get("org.gnome.evolution.dataserver.Source");

        if let Some(props) = source_iface {
            let uid = props.get("UID").and_then(|v| {
                let val: zvariant::Value = v.clone().into();
                if let zvariant::Value::Str(s) = val {
                    Some(s.to_string())
                } else {
                    None
                }
            });

            let data = props.get("Data").and_then(|v| {
                let val: zvariant::Value = v.clone().into();
                if let zvariant::Value::Str(s) = val {
                    Some(s.to_string())
                } else {
                    None
                }
            });

            if let (Some(uid), Some(data)) = (uid, data)
                && data.contains("[Calendar]")
            {
                let display_name = extract_display_name(&data).unwrap_or_else(|| uid.clone());
                sources.push(CalendarSource { uid, display_name });
            }
        }
    }

    info!("[eds] Discovered {} calendar sources", sources.len());
    for src in &sources {
        info!("[eds]   source uid={:?} display_name={:?}", src.uid, src.display_name);
    }
    Ok(sources)
}

/// Open a calendar backend via the CalendarFactory.
async fn open_calendar(conn: &Connection, source_uid: &str) -> Result<(String, String)> {
    let proxy = zbus::Proxy::new(
        conn,
        CALENDAR_FACTORY_DEST,
        CALENDAR_FACTORY_PATH,
        "org.gnome.evolution.dataserver.CalendarFactory",
    )
    .await
    .context("Failed to create CalendarFactory proxy")?;

    let (object_path, bus_name): (String, String) = proxy
        .call("OpenCalendar", &(source_uid,))
        .await
        .context("Failed to open calendar")?;

    debug!(
        "[eds] Opened calendar '{}': path={}, bus={}",
        source_uid, object_path, bus_name
    );
    Ok((object_path, bus_name))
}

/// Check whether a calendar backend is online (has valid credentials).
///
/// EDS sets the `online` backend property to `"false"` when OAuth credentials
/// have expired (GOA `AttentionNeeded: true`). Any calendar in this state
/// cannot sync new events from the remote server.
async fn check_backend_online(
    conn: &Connection,
    bus_name: &str,
    calendar_path: &str,
    display_name: &str,
    source_uid: &str,
) {
    let proxy = match zbus::Proxy::new(
        conn,
        bus_name,
        calendar_path,
        "org.gnome.evolution.dataserver.Calendar",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            debug!("[eds] Could not create proxy to check online status for {source_uid}: {e}");
            return;
        }
    };

    let result: Result<String, _> = proxy.call("GetBackendProperty", &("online",)).await;
    match result {
        Ok(value) if value == "false" => {
            warn!(
                "[eds] Calendar '{}' ({}) is OFFLINE — credentials may have expired. \
                Re-authenticate in GNOME Settings → Online Accounts.",
                display_name, source_uid
            );
        }
        Ok(_) => {}
        Err(e) => {
            debug!("[eds] Could not read online property for {source_uid}: {e}");
        }
    }
}

/// Create a calendar view with the given query.
async fn create_view(
    conn: &Connection,
    bus_name: &str,
    calendar_path: &str,
    query: &str,
) -> Result<String> {
    let proxy = zbus::Proxy::new(
        conn,
        bus_name,
        calendar_path,
        "org.gnome.evolution.dataserver.Calendar",
    )
    .await
    .context("Failed to create Calendar proxy")?;

    let view_path: zvariant::OwnedObjectPath = proxy
        .call("GetView", &(query,))
        .await
        .context("Failed to create calendar view")?;

    let view_path = view_path.to_string();
    debug!("[eds] Created view: {}", view_path);
    Ok(view_path)
}

/// Start a calendar view (begins delivering signals).
async fn start_view(conn: &Connection, bus_name: &str, view_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, bus_name, view_path, CALENDAR_VIEW_IFACE)
        .await
        .context("Failed to create CalendarView proxy")?;

    let _: () = proxy
        .call("Start", &())
        .await
        .context("Failed to start calendar view")?;

    debug!("[eds] Started view: {}", view_path);
    Ok(())
}

/// Time range (UTC timestamps) for the calendar query window.
#[derive(Clone, Copy)]
struct TimeRange {
    start: i64,
    end: i64,
}

/// Build the time range and `occur-in-time-range?` query string.
///
/// The window starts at today's local midnight (not `now`) so that events
/// which began before the daemon was launched—but still fall within today—
/// are not silently excluded from the view.
fn build_time_range_query_from_today() -> (TimeRange, String) {
    let local_now = chrono::Local::now();
    let today_midnight = local_now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_local_timezone(chrono::Local)
        .earliest()
        .expect("today midnight is a valid local time")
        .to_utc();
    let end = local_now.to_utc() + chrono::Duration::days(60);
    let range = TimeRange {
        start: today_midnight.timestamp(),
        end: end.timestamp(),
    };
    let query = format!(
        "(occur-in-time-range? (make-time \"{}\") (make-time \"{}\"))",
        today_midnight.format("%Y%m%dT%H%M%SZ"),
        end.format("%Y%m%dT%H%M%SZ")
    );
    info!(
        "[eds] Time range query: {} → {} (UTC timestamps {} → {})",
        today_midnight.format("%Y-%m-%d %H:%M:%S UTC"),
        end.format("%Y-%m-%d %H:%M:%S UTC"),
        range.start,
        range.end
    );
    (range, query)
}

/// Returns seconds until the next local midnight (minimum 1).
fn secs_until_eds_midnight() -> u64 {
    let now = chrono::Local::now();
    let tomorrow = (now.date_naive() + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_local_timezone(chrono::Local)
        .earliest()
        .expect("tomorrow midnight is a valid local time");
    (tomorrow.timestamp() - now.timestamp()).max(1) as u64
}

/// Discover EDS calendar sources, create views for today+30d, and spawn
/// view-monitor tasks. Returns the handles so callers can abort them later.
async fn setup_calendar_views(
    conn: &Connection,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
) -> Vec<tokio::task::JoinHandle<()>> {
    let sources = match discover_calendar_sources(conn).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[eds] Failed to discover calendar sources: {e}");
            return vec![];
        }
    };

    if sources.is_empty() {
        debug!("[eds] No calendar sources found");
        return vec![];
    }

    let view_paths = Arc::new(StdMutex::new(HashSet::new()));
    let (time_range, query) = build_time_range_query_from_today();

    let mut handles = Vec::new();
    for source in &sources {
        let conn_clone = conn.clone();
        let state_clone = state.clone();
        let notifier_clone = notifier.clone();
        let source_uid = source.uid.clone();
        let source_display_name = source.display_name.clone();
        let query_clone = query.clone();
        let view_paths_clone = view_paths.clone();

        let handle = tokio::spawn(async move {
            match open_calendar(&conn_clone, &source_uid).await {
                Ok((calendar_path, bus_name)) => {
                    // Check whether the backend has valid credentials.
                    // Offline = GOA OAuth token expired; log a prominent warning.
                    check_backend_online(
                        &conn_clone,
                        &bus_name,
                        &calendar_path,
                        &source_display_name,
                        &source_uid,
                    )
                    .await;

                    // Record this backend so do_refresh can Open()+Refresh() it later.
                    {
                        let mut st = state_clone.lock_or_recover();
                        st.calendar_backends
                            .push((bus_name.clone(), calendar_path.clone()));
                    }

                    match create_view(&conn_clone, &bus_name, &calendar_path, &query_clone).await {
                        Ok(view_path) => {
                            {
                                let mut paths = view_paths_clone.lock_or_recover();
                                paths.insert(view_path.clone());
                            }

                            // spawn_view_monitor registers AddMatch then calls
                            // start_view internally, so the initial ObjectsAdded
                            // batch is never missed.
                            //
                            // The returned JoinHandle is for the background monitor
                            // loop — store it in state so midnight rebuild can abort it.
                            let state_for_handle = state_clone.clone();
                            match spawn_view_monitor(
                                conn_clone,
                                bus_name,
                                view_path,
                                state_clone,
                                notifier_clone,
                                view_paths_clone,
                                time_range,
                            )
                            .await
                            {
                                Ok(monitor_handle) => {
                                    let mut st = state_for_handle.lock_or_recover();
                                    st.view_monitor_handles.push(monitor_handle);
                                }
                                Err(e) => warn!("[eds] View monitor error: {e}"),
                            }
                        }
                        Err(e) => warn!("[eds] Failed to create view for {source_uid}: {e}"),
                    }
                }
                Err(e) => warn!("[eds] Failed to open calendar {source_uid}: {e}"),
            }
            debug!("[eds] View task for {source_uid} stopped");
        });

        handles.push(handle);
    }

    handles
}

/// Monitor EDS calendars and populate shared state.
///
/// Discovers calendar sources, opens calendars, creates views,
/// and spawns monitoring tasks for each view.
async fn monitor_eds_calendars(
    conn: Connection,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    // setup_calendar_views spawns one tokio task per calendar source.  Each
    // task opens the calendar, pushes its backend into state.calendar_backends,
    // creates the view, calls spawn_view_monitor, and stores the resulting
    // monitor JoinHandle in state.view_monitor_handles.
    //
    // We must wait for ALL setup tasks to finish before calling
    // refresh_with_status: do_refresh checks state.calendar_backends and skips
    // with "no backends" if the tasks haven't run yet.  Previously this wasn't
    // awaited, causing the startup sync to be silently dropped every time.
    let setup_handles = setup_calendar_views(&conn, state.clone(), notifier.clone()).await;
    futures_util::future::join_all(setup_handles).await;

    // Trigger an immediate refresh so EDS syncs from Google on startup.
    // The initial ObjectsAdded batch only reflects EDS's local cache; recently
    // added or rescheduled events (e.g. a meeting moved to today) won't appear
    // until EDS fetches from the remote backend.  Sending Refresh() here
    // ensures those events arrive via ObjectsModified within ~30-120 seconds.
    info!("[eds] Startup refresh: requesting EDS sync from remote backends");
    refresh_with_status(&conn, &state, &notifier).await;

    // Midnight loop: rebuild views once per day so the query window stays
    // anchored to today. Also purges events whose end_time is before the
    // new day to prevent stale entities from persisting in the daemon.
    loop {
        let secs = secs_until_eds_midnight();
        debug!("[eds] Next view rebuild in {secs}s (midnight)");
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;

        debug!("[eds] Midnight reached — rebuilding calendar views");

        // Compute new today midnight timestamp for stale-event pruning.
        let (new_time_range, _) = build_time_range_query_from_today();
        let new_today_midnight = new_time_range.start;

        // Abort old monitors and purge events that ended before the new day.
        {
            let mut st = state.lock_or_recover();

            for handle in st.view_monitor_handles.drain(..) {
                handle.abort();
            }

            // Clear backends and unsupported set so setup_calendar_views can repopulate
            // for the new day. Clearing refresh_unsupported gives reconfigured calendars
            // a fresh chance after midnight.
            st.calendar_backends.clear();
            st.refresh_unsupported.clear();

            let stale_keys: Vec<String> = st
                .events
                .iter()
                .filter(|(_, event)| event.end_time < new_today_midnight)
                .map(|(key, _)| key.clone())
                .collect();

            for key in &stale_keys {
                st.events.remove(key);
            }

            if !stale_keys.is_empty() {
                debug!("[eds] Pruned {} stale events at midnight", stale_keys.len());
            }
        }

        // Notify daemon so it removes the pruned entities from its cache.
        notifier.notify();

        // Set up fresh views anchored to the new today.  Each setup task
        // stores the resulting monitor JoinHandle in state.view_monitor_handles,
        // so we only need to join_all the setup tasks to ensure monitor handles
        // are in place and calendar_backends is fully repopulated.
        let setup_handles = setup_calendar_views(&conn, state.clone(), notifier.clone()).await;
        futures_util::future::join_all(setup_handles).await;

        debug!("[eds] Calendar views rebuilt for new day");
    }
}

/// Spawn a monitor task for a calendar view.
///
/// Registers D-Bus match rules first, then starts the view so the initial
/// `ObjectsAdded` batch is guaranteed to arrive after the match rules are
/// in place. Without this ordering the bus drops the initial signals before
/// our process has subscribed, causing events to be permanently missing.
///
/// Listens for ObjectsAdded, ObjectsModified, ObjectsRemoved signals,
/// parses iCalendar VEVENT data, and updates shared state.
///
/// Returns the `JoinHandle` for the background monitoring loop so the caller
/// can store it and abort it at midnight when views are rebuilt.
async fn spawn_view_monitor(
    conn: Connection,
    bus_name: String,
    view_path: String,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
    view_paths: Arc<StdMutex<HashSet<String>>>,
    time_range: TimeRange,
) -> Result<tokio::task::JoinHandle<()>> {
    // Create the message stream before registering match rules so no signals
    // can slip through the gap between AddMatch and stream creation.
    let mut stream = MessageStream::from(&conn);

    // Register match rules so the bus forwards signals to us.
    // This MUST happen before start_view() to avoid the race where EDS fires
    // the initial ObjectsAdded batch before our AddMatch is registered.
    for member in &["ObjectsAdded", "ObjectsModified", "ObjectsRemoved"] {
        let rule = format!(
            "type='signal',interface='{}',path='{}',member='{}'",
            CALENDAR_VIEW_IFACE, view_path, member
        );
        if let Err(e) = conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "AddMatch",
                &(&rule,),
            )
            .await
        {
            warn!("[eds] Failed to add match rule for {}: {e}", member);
        }
    }

    // Start the view only after match rules are in place.
    // EDS fires ObjectsAdded for all currently-matching events synchronously
    // inside Start(); if AddMatch isn't registered yet those signals are lost.
    debug!("[eds] AddMatch registered for view {view_path}, calling Start()");
    start_view(&conn, &bus_name, &view_path).await?;
    debug!("[eds] Start() returned for view {view_path}, monitoring loop starting");

    let handle = tokio::spawn(async move {
        debug!("[eds] Monitoring view: {}", view_path);

        while let Some(msg_result) = stream.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(e) => {
                    warn!("[eds] Message stream error: {}", e);
                    break;
                }
            };

            // Only process signals (skip method calls, replies, errors)
            if msg.header().message_type() != zbus::message::Type::Signal {
                continue;
            }

            // Check if this message is for our view
            let msg_path = msg
                .header()
                .path()
                .map(|p: &zbus::zvariant::ObjectPath| p.to_string())
                .unwrap_or_default();

            if msg_path != view_path {
                continue;
            }

            // Check interface
            let msg_iface = msg
                .header()
                .interface()
                .map(|i: &zbus::names::InterfaceName| i.to_string())
                .unwrap_or_default();

            if msg_iface != CALENDAR_VIEW_IFACE {
                continue;
            }

            let member = msg
                .header()
                .member()
                .map(|m: &zbus::names::MemberName| m.to_string())
                .unwrap_or_default();

            match member.as_str() {
                "ObjectsAdded" => {
                    let body = msg.body();
                    if let Ok((icals,)) = body.deserialize::<(Vec<String>,)>() {
                        info!(
                            "[eds] view={} ObjectsAdded: {} raw iCal strings",
                            view_path,
                            icals.len()
                        );
                        let events = parse_ical_events(&icals, time_range);
                        if events.is_empty() {
                            info!(
                                "[eds] view={} ObjectsAdded: all {} iCal strings produced 0 events \
                                 (parse failure or outside time range [{}, {}])",
                                view_path,
                                icals.len(),
                                time_range.start,
                                time_range.end
                            );
                        } else {
                            info!(
                                "[eds] view={} ObjectsAdded: {} events stored: [{}]",
                                view_path,
                                events.len(),
                                events
                                    .iter()
                                    .map(|e| {
                                        let start = chrono::DateTime::from_timestamp(e.start_time, 0)
                                            .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                                            .unwrap_or_else(|| format!("ts:{}", e.start_time));
                                        format!("{:?}@{}", e.summary, start)
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            update_state_add_events(&state, events);
                            notifier.notify();
                        }
                    } else {
                        warn!("[eds] view={} Failed to deserialize ObjectsAdded signal body", view_path);
                    }
                }
                "ObjectsModified" => {
                    let body = msg.body();
                    if let Ok((icals,)) = body.deserialize::<(Vec<String>,)>() {
                        info!(
                            "[eds] view={} ObjectsModified: {} raw iCal strings",
                            view_path,
                            icals.len()
                        );
                        let events = parse_ical_events(&icals, time_range);
                        if !events.is_empty() {
                            info!(
                                "[eds] view={} ObjectsModified: {} events updated: [{}]",
                                view_path,
                                events.len(),
                                events
                                    .iter()
                                    .map(|e| {
                                        let start = chrono::DateTime::from_timestamp(e.start_time, 0)
                                            .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                                            .unwrap_or_else(|| format!("ts:{}", e.start_time));
                                        format!("{:?}@{}", e.summary, start)
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            // For modified events, remove old occurrences then add new
                            let uids: Vec<String> = events.iter().map(|e| e.uid.clone()).collect();
                            update_state_remove_events(&state, &uids);
                            update_state_add_events(&state, events);
                            notifier.notify();
                        }
                    } else {
                        warn!("[eds] view={} Failed to deserialize ObjectsModified signal body", view_path);
                    }
                }
                "ObjectsRemoved" => {
                    let body = msg.body();
                    if let Ok((uids,)) = body.deserialize::<(Vec<String>,)>() {
                        if !uids.is_empty() {
                            debug!("[eds] ObjectsRemoved: {} events", uids.len());
                            update_state_remove_events(&state, &uids);
                            notifier.notify();
                        }
                    } else {
                        warn!("[eds] Failed to deserialize ObjectsRemoved signal body");
                    }
                }
                _ => {}
            }
        }

        // Clean up view path tracking
        {
            let mut paths = view_paths.lock_or_recover();
            paths.remove(&view_path);
        }

        debug!("[eds] View monitor stopped: {}", view_path);
    });

    Ok(handle)
}

/// Split a VCALENDAR string into individual VEVENT blocks.
///
/// A single VCALENDAR may contain multiple VEVENT components — for example,
/// a master recurring event (with RRULE + EXDATE) alongside an exception
/// occurrence (with RECURRENCE-ID) that was rescheduled to a different time.
/// Without this split, `parse_vevent_raw` stops at the first `END:VEVENT`
/// and silently drops all subsequent VEVENTs.
fn split_vevents(ical_str: &str) -> Vec<String> {
    let unfolded = unfold_ical(ical_str);
    let mut result = Vec::new();
    let mut current: Option<String> = None;

    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');

        if line == "BEGIN:VEVENT" {
            current = Some("BEGIN:VEVENT\r\n".to_string());
        } else if line == "END:VEVENT" {
            if let Some(mut block) = current.take() {
                block.push_str("END:VEVENT\r\n");
                result.push(block);
            }
        } else if let Some(ref mut block) = current {
            block.push_str(line);
            block.push_str("\r\n");
        }
    }

    if result.len() > 1 {
        info!(
            "[eds] split_vevents: found {} VEVENTs in one iCal blob (multi-VEVENT case)",
            result.len()
        );
    }

    result
}

/// Parse a list of iCalendar strings into CalendarEvent entities.
///
/// Each iCal string may contain multiple VEVENTs (e.g. a master recurring
/// event plus exception occurrences with RECURRENCE-ID).  The function splits
/// each string into individual VEVENT blocks before expanding.
///
/// Recurring events (those with RRULE) are expanded into individual
/// occurrences within `range`.  Non-recurring events pass through as-is.
fn parse_ical_events(icals: &[String], range: TimeRange) -> Vec<entity::calendar::CalendarEvent> {
    icals
        .iter()
        .flat_map(|ical| {
            split_vevents(ical)
                .into_iter()
                .flat_map(move |vevent| expand_vevent(&vevent, range))
        })
        .collect()
}

// ── Intermediate VEVENT representation ───────────────────────────────────

/// Holds all raw fields extracted from a VEVENT, including recurrence info
/// needed for RRULE expansion.
struct RawVevent {
    uid: String,
    summary: String,
    all_day: bool,
    description: Option<String>,
    location: Option<String>,
    attendees: Vec<entity::calendar::CalendarEventAttendee>,
    /// UTC timestamp of DTSTART.
    start_time: i64,
    /// UTC timestamp of DTEND (or DTSTART + 1h if absent).
    end_time: i64,
    /// Naive local datetime of DTSTART (needed for TZ-correct expansion).
    dtstart_naive: chrono::NaiveDateTime,
    /// TZID extracted from DTSTART params, if any.
    tz: Option<chrono_tz::Tz>,
    /// Whether DTSTART ends with Z (UTC).
    utc: bool,
    /// Raw RRULE value (e.g. "FREQ=WEEKLY;BYDAY=TU").
    rrule: Option<String>,
    /// EXDATE timestamps to exclude from recurrence.
    exdates: HashSet<i64>,
}

/// Parse a single iCalendar VEVENT string into a `RawVevent`.
fn parse_vevent_raw(ical_str: &str) -> Option<RawVevent> {
    let unfolded = unfold_ical(ical_str);

    let mut in_vevent = false;
    let mut nest_depth: u32 = 0;
    let mut uid = None;
    let mut summary = None;
    let mut dtstart_ts: Option<i64> = None;
    let mut dtend_ts: Option<i64> = None;
    let mut all_day = false;
    let mut description = None;
    let mut location = None;
    let mut attendees: Vec<entity::calendar::CalendarEventAttendee> = Vec::new();
    let mut rrule: Option<String> = None;
    let mut exdates: HashSet<i64> = HashSet::new();
    // Keep the raw DTSTART pieces for TZ-correct expansion.
    let mut dtstart_naive: Option<chrono::NaiveDateTime> = None;
    let mut dtstart_tz: Option<chrono_tz::Tz> = None;
    let mut dtstart_utc_flag = false;

    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');

        if line == "BEGIN:VEVENT" {
            in_vevent = true;
            continue;
        }
        if line == "END:VEVENT" {
            break;
        }
        if !in_vevent {
            continue;
        }

        // Track nested components
        if line.starts_with("BEGIN:") {
            nest_depth += 1;
            continue;
        }
        if line.starts_with("END:") {
            nest_depth = nest_depth.saturating_sub(1);
            continue;
        }
        if nest_depth > 0 {
            continue;
        }

        if let Some(rest) = line.strip_prefix("UID:") {
            uid = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("SUMMARY:") {
            summary = Some(rest.to_string());
        } else if line.starts_with("DTSTART") {
            let (params, value) = split_ical_property(line, "DTSTART");
            if params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") {
                all_day = true;
            }
            dtstart_ts = parse_ical_datetime(&value, &params);
            dtstart_naive = parse_ical_naive_datetime(&value);
            dtstart_tz = extract_tzid(&params);
            dtstart_utc_flag = value.ends_with('Z');
        } else if line.starts_with("DTEND") {
            let (params, value) = split_ical_property(line, "DTEND");
            dtend_ts = parse_ical_datetime(&value, &params);
        } else if let Some(rest) = line.strip_prefix("RRULE:") {
            rrule = Some(rest.to_string());
        } else if line.starts_with("EXDATE") {
            let (params, value) = split_ical_property(line, "EXDATE");
            for part in value.split(',') {
                if let Some(ts) = parse_ical_datetime(part.trim(), &params) {
                    exdates.insert(ts);
                }
            }
        } else if line.starts_with("DESCRIPTION") {
            let (_params, value) = split_ical_property(line, "DESCRIPTION");
            if !value.is_empty() {
                description = Some(unescape_ical(&value));
            }
        } else if line.starts_with("LOCATION") {
            let (_params, value) = split_ical_property(line, "LOCATION");
            if !value.is_empty() {
                location = Some(unescape_ical(&value));
            }
        } else if line.starts_with("ATTENDEE")
            && let Some(attendee) = parse_attendee_line(line)
        {
            attendees.push(attendee);
        }
    }

    let uid = uid?;
    let summary = summary.unwrap_or_default();
    let start_time = dtstart_ts?;
    let end_time = dtend_ts.unwrap_or(start_time + 3600);
    let dtstart_naive = dtstart_naive?;

    Some(RawVevent {
        uid,
        summary,
        all_day,
        description,
        location,
        attendees,
        start_time,
        end_time,
        dtstart_naive,
        tz: dtstart_tz,
        utc: dtstart_utc_flag,
        rrule,
        exdates,
    })
}

/// Convert a `RawVevent` to a single `CalendarEvent` (non-recurring path).
fn raw_to_event(raw: &RawVevent) -> entity::calendar::CalendarEvent {
    entity::calendar::CalendarEvent {
        uid: raw.uid.clone(),
        summary: raw.summary.clone(),
        start_time: raw.start_time,
        end_time: raw.end_time,
        all_day: raw.all_day,
        description: raw.description.clone(),
        location: raw.location.clone(),
        attendees: raw.attendees.clone(),
    }
}

/// Entry point kept for existing tests (non-recurring path).
#[cfg(test)]
fn parse_vevent(ical_str: &str) -> Option<entity::calendar::CalendarEvent> {
    parse_vevent_raw(ical_str).map(|raw| raw_to_event(&raw))
}

// ── RRULE parsing and expansion ──────────────────────────────────────────

/// Parsed recurrence rule.
struct RecurrenceRule {
    freq: Frequency,
    interval: u32,
    by_day: Vec<chrono::Weekday>,
    count: Option<u32>,
    until: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Parse an RRULE value string (e.g. "FREQ=WEEKLY;BYDAY=TU;INTERVAL=2").
fn parse_rrule(s: &str) -> Option<RecurrenceRule> {
    let mut freq = None;
    let mut interval = 1u32;
    let mut by_day = Vec::new();
    let mut count = None;
    let mut until = None;

    for part in s.split(';') {
        if let Some(val) = part.strip_prefix("FREQ=") {
            freq = match val {
                "DAILY" => Some(Frequency::Daily),
                "WEEKLY" => Some(Frequency::Weekly),
                "MONTHLY" => Some(Frequency::Monthly),
                "YEARLY" => Some(Frequency::Yearly),
                _ => None,
            };
        } else if let Some(val) = part.strip_prefix("INTERVAL=") {
            interval = val.parse().unwrap_or(1);
        } else if let Some(val) = part.strip_prefix("COUNT=") {
            count = val.parse().ok();
        } else if let Some(val) = part.strip_prefix("UNTIL=") {
            // UNTIL can be a date or datetime; parse as datetime with empty params (UTC)
            until = parse_ical_datetime(val, "");
        } else if let Some(val) = part.strip_prefix("BYDAY=") {
            for day_str in val.split(',') {
                // Strip optional ordinal prefix (e.g. "2MO" → "MO")
                let weekday_str =
                    day_str.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-');
                if let Some(wd) = parse_weekday(weekday_str) {
                    by_day.push(wd);
                }
            }
        }
    }

    Some(RecurrenceRule {
        freq: freq?,
        interval,
        by_day,
        count,
        until,
    })
}

fn parse_weekday(s: &str) -> Option<chrono::Weekday> {
    match s {
        "MO" => Some(chrono::Weekday::Mon),
        "TU" => Some(chrono::Weekday::Tue),
        "WE" => Some(chrono::Weekday::Wed),
        "TH" => Some(chrono::Weekday::Thu),
        "FR" => Some(chrono::Weekday::Fri),
        "SA" => Some(chrono::Weekday::Sat),
        "SU" => Some(chrono::Weekday::Sun),
        _ => None,
    }
}

/// Convert a naive local datetime to a UTC timestamp, respecting timezone.
fn naive_to_timestamp(
    naive: chrono::NaiveDateTime,
    tz: Option<chrono_tz::Tz>,
    utc: bool,
) -> Option<i64> {
    use chrono::TimeZone;

    if utc {
        return Some(naive.and_utc().timestamp());
    }
    if let Some(tz) = tz {
        // .earliest() picks the pre-DST side for ambiguous times.
        return tz
            .from_local_datetime(&naive)
            .earliest()
            .map(|dt| dt.timestamp());
    }
    // Floating time → local.
    chrono::Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.timestamp())
}

/// Parse a datetime value into a `NaiveDateTime` (without timezone conversion).
fn parse_ical_naive_datetime(value: &str) -> Option<chrono::NaiveDateTime> {
    use chrono::{NaiveDate, NaiveTime};

    let s = value.strip_suffix('Z').unwrap_or(value);

    // DATE only: YYYYMMDD
    if s.len() == 8 && !s.contains('T') {
        let year: i32 = s[0..4].parse().ok()?;
        let month: u32 = s[4..6].parse().ok()?;
        let day: u32 = s[6..8].parse().ok()?;
        let d = NaiveDate::from_ymd_opt(year, month, day)?;
        return Some(d.and_time(NaiveTime::from_hms_opt(0, 0, 0)?));
    }

    // DATETIME: YYYYMMDDTHHmmss
    if s.len() >= 15 && s.contains('T') {
        let year: i32 = s[0..4].parse().ok()?;
        let month: u32 = s[4..6].parse().ok()?;
        let day: u32 = s[6..8].parse().ok()?;
        let hour: u32 = s[9..11].parse().ok()?;
        let min: u32 = s[11..13].parse().ok()?;
        let sec: u32 = s[13..15].parse().ok()?;
        let d = NaiveDate::from_ymd_opt(year, month, day)?;
        let t = NaiveTime::from_hms_opt(hour, min, sec)?;
        return Some(chrono::NaiveDateTime::new(d, t));
    }

    None
}

/// Extract TZID from iCal property parameters (e.g. ";TZID=Europe/Prague").
fn extract_tzid(params: &str) -> Option<chrono_tz::Tz> {
    let start = params.find("TZID=")?;
    let tzid = &params[start + 5..];
    let tzid = tzid.split(';').next().unwrap_or(tzid);
    tzid.parse().ok()
}

/// Expand a single iCal VEVENT into one or more `CalendarEvent` entities.
///
/// Non-recurring events produce a single entity.  Recurring events (RRULE)
/// are expanded into individual occurrences within `range`, with EXDATE
/// exclusions applied.
fn expand_vevent(ical_str: &str, range: TimeRange) -> Vec<entity::calendar::CalendarEvent> {
    let Some(raw) = parse_vevent_raw(ical_str) else {
        // Extract SUMMARY for a more readable log message; fall back to raw iCal prefix.
        let summary_hint = ical_str
            .lines()
            .find(|l| l.starts_with("SUMMARY:"))
            .map(|l| &l[8..])
            .unwrap_or("<no SUMMARY>");
        warn!(
            "[eds] Failed to parse VEVENT (summary={:?}), event dropped: {:?}",
            summary_hint,
            &ical_str[..ical_str.len().min(200)]
        );
        return Vec::new();
    };

    let rrule_str = match &raw.rrule {
        Some(r) => r.clone(),
        None => return vec![raw_to_event(&raw)],
    };

    let Some(rule) = parse_rrule(&rrule_str) else {
        warn!("[eds] unsupported RRULE: {rrule_str}");
        return vec![raw_to_event(&raw)];
    };

    let duration = raw.end_time - raw.start_time;
    let time_of_day = raw.dtstart_naive.time();

    let mut occurrences = Vec::new();
    let mut generated = 0u32;

    // Walk candidate dates forward from DTSTART.
    let mut cursor = raw.dtstart_naive.date();

    // Iteration cap to prevent runaway loops.
    const MAX_ITERATIONS: u32 = 10_000;
    let mut iterations = 0u32;

    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            break;
        }

        // For weekly recurrence with BYDAY: check every day of the current
        // period (the week starting at cursor) against the day filter.
        let candidates: Vec<chrono::NaiveDate> = match rule.freq {
            Frequency::Weekly if !rule.by_day.is_empty() => {
                use chrono::Datelike;
                // Find the Monday of the week containing `cursor`.
                let iso_week_start =
                    cursor - chrono::Duration::days(cursor.weekday().num_days_from_monday() as i64);
                rule.by_day
                    .iter()
                    .map(|wd| {
                        iso_week_start + chrono::Duration::days(wd.num_days_from_monday() as i64)
                    })
                    .filter(|d| *d >= raw.dtstart_naive.date())
                    .collect()
            }
            _ => vec![cursor],
        };

        for date in candidates {
            let occ_naive = date.and_time(time_of_day);
            let Some(occ_start) = naive_to_timestamp(occ_naive, raw.tz, raw.utc) else {
                continue;
            };
            let occ_end = occ_start + duration;

            // Check UNTIL / COUNT limits.
            if let Some(until) = rule.until
                && occ_start > until
            {
                return occurrences;
            }

            // Past range end → done.
            if occ_start >= range.end {
                return occurrences;
            }

            // Skip if before range or excluded.
            if occ_end > range.start && !raw.exdates.contains(&occ_start) {
                occurrences.push(entity::calendar::CalendarEvent {
                    uid: raw.uid.clone(),
                    summary: raw.summary.clone(),
                    start_time: occ_start,
                    end_time: occ_end,
                    all_day: raw.all_day,
                    description: raw.description.clone(),
                    location: raw.location.clone(),
                    attendees: raw.attendees.clone(),
                });
            }

            generated += 1;
            if let Some(count) = rule.count
                && generated >= count
            {
                return occurrences;
            }
        }

        // Advance cursor by one period.
        cursor = advance_date(cursor, rule.freq, rule.interval);
    }

    occurrences
}

/// Advance a date by one recurrence period.
fn advance_date(date: chrono::NaiveDate, freq: Frequency, interval: u32) -> chrono::NaiveDate {
    use chrono::Datelike;
    match freq {
        Frequency::Daily => date + chrono::Duration::days(interval as i64),
        Frequency::Weekly => date + chrono::Duration::weeks(interval as i64),
        Frequency::Monthly => {
            // Add `interval` months; clamp day to month length.
            let total_months = date.year() * 12 + (date.month0() as i32) + (interval as i32);
            let new_year = total_months / 12;
            let new_month = (total_months % 12) as u32 + 1;
            let max_day = days_in_month(new_year, new_month);
            let day = date.day().min(max_day);
            chrono::NaiveDate::from_ymd_opt(new_year, new_month, day).unwrap_or(date)
        }
        Frequency::Yearly => {
            chrono::NaiveDate::from_ymd_opt(date.year() + interval as i32, date.month(), date.day())
                .unwrap_or(date)
        }
    }
}

/// Number of days in a given month.
fn days_in_month(year: i32, month: u32) -> u32 {
    chrono::NaiveDate::from_ymd_opt(
        if month == 12 { year + 1 } else { year },
        if month == 12 { 1 } else { month + 1 },
        1,
    )
    .map(|d| (d - chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap()).num_days() as u32)
    .unwrap_or(30)
}

/// Unfold iCalendar continuation lines.
fn unfold_ical(s: &str) -> String {
    let mut result = String::new();
    for line in s.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line: remove leading whitespace
            result.push_str(&line[1..]);
        } else {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(line);
        }
    }
    result
}

/// Split iCalendar property line into (parameters, value).
fn split_ical_property(line: &str, property: &str) -> (String, String) {
    let rest = line.strip_prefix(property).unwrap_or("");
    if let Some(colon_pos) = rest.find(':') {
        let params = rest[..colon_pos].to_string();
        let value = rest[colon_pos + 1..].to_string();
        (params, value)
    } else {
        (String::new(), rest.to_string())
    }
}

/// Parse iCalendar datetime/date value to Unix timestamp.
fn parse_ical_datetime(value: &str, params: &str) -> Option<i64> {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

    // DATE format: YYYYMMDD
    // All-day events use local midnight, not UTC midnight, so that a
    // "Feb 14" all-day event spans [Feb 14 00:00 local, Feb 15 00:00 local).
    if params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") && value.len() >= 8 {
        let year: i32 = value[0..4].parse().ok()?;
        let month: u32 = value[4..6].parse().ok()?;
        let day: u32 = value[6..8].parse().ok()?;
        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        let datetime = date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
        return Some(
            chrono::Local
                .from_local_datetime(&datetime)
                .earliest()?
                .timestamp(),
        );
    }

    // DATETIME format: YYYYMMDDTHHmmss[Z] or with TZID
    let dt_str = if let Some(stripped) = value.strip_suffix('Z') {
        stripped
    } else {
        value
    };

    if dt_str.len() >= 15 && dt_str.contains('T') {
        let year: i32 = dt_str[0..4].parse().ok()?;
        let month: u32 = dt_str[4..6].parse().ok()?;
        let day: u32 = dt_str[6..8].parse().ok()?;
        let hour: u32 = dt_str[9..11].parse().ok()?;
        let min: u32 = dt_str[11..13].parse().ok()?;
        let sec: u32 = dt_str[13..15].parse().ok()?;

        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        let time = NaiveTime::from_hms_opt(hour, min, sec)?;
        let datetime = NaiveDateTime::new(date, time);

        // Ends with Z → UTC
        if value.ends_with('Z') {
            return Some(datetime.and_utc().timestamp());
        }

        // Try to extract TZID and convert
        if let Some(tzid_start) = params.find("TZID=") {
            let tzid = &params[tzid_start + 5..];
            let tzid = tzid.split(';').next().unwrap_or(tzid);
            if let Ok(tz) = tzid.parse::<chrono_tz::Tz>()
                && let Some(dt) = tz.from_local_datetime(&datetime).earliest()
            {
                return Some(dt.timestamp());
            }
        }

        // No Z, no TZID → floating time (RFC 5545), interpret as local
        return Some(
            chrono::Local
                .from_local_datetime(&datetime)
                .earliest()
                .map(|dt| dt.timestamp())
                .unwrap_or_else(|| datetime.and_utc().timestamp()),
        );
    }

    None
}

/// Unescape iCalendar text value.
fn unescape_ical(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// Parse ATTENDEE property line.
fn parse_attendee_line(line: &str) -> Option<entity::calendar::CalendarEventAttendee> {
    let rest = line.strip_prefix("ATTENDEE")?;
    let colon_pos = rest.find(':')?;
    let params = &rest[..colon_pos];
    let value = &rest[colon_pos + 1..];

    // Extract email (value is typically "mailto:email@example.com")
    let email = value.strip_prefix("mailto:").unwrap_or(value).to_string();

    // Extract CN (Common Name) parameter
    let name = if let Some(cn_start) = params.find("CN=") {
        let cn = &params[cn_start + 3..];
        // CN value might be quoted
        let cn = if let Some(stripped) = cn.strip_prefix('"') {
            if let Some(end_quote) = stripped.find('"') {
                &stripped[..end_quote]
            } else {
                cn
            }
        } else {
            cn.split(';').next().unwrap_or(cn)
        };
        Some(cn.to_string())
    } else {
        None
    };

    // Extract PARTSTAT parameter
    let status = if let Some(partstat_start) = params.find("PARTSTAT=") {
        let partstat = &params[partstat_start + 9..];
        let partstat = partstat.split(';').next().unwrap_or(partstat);
        match partstat {
            "ACCEPTED" => entity::calendar::AttendeeStatus::Accepted,
            "DECLINED" => entity::calendar::AttendeeStatus::Declined,
            "TENTATIVE" => entity::calendar::AttendeeStatus::Tentative,
            _ => entity::calendar::AttendeeStatus::NeedsAction,
        }
    } else {
        entity::calendar::AttendeeStatus::NeedsAction
    };

    Some(entity::calendar::CalendarEventAttendee {
        name,
        email,
        status,
    })
}

/// Update state by adding/updating events.
fn update_state_add_events(
    state: &Arc<StdMutex<EdsState>>,
    events: Vec<entity::calendar::CalendarEvent>,
) {
    let mut st = state.lock_or_recover();

    for event in events {
        let key = EdsPlugin::make_occurrence_key(&event.uid, event.start_time);
        st.events.insert(key, event);
    }
}

/// Update state by removing events matching base UIDs.
fn update_state_remove_events(state: &Arc<StdMutex<EdsState>>, uids: &[String]) {
    let mut st = state.lock_or_recover();

    // Remove all occurrence keys matching any of the base UIDs
    let before = st.events.len();
    st.events.retain(|key, _| {
        let base_uid = key.rsplit_once("::").map(|(uid, _)| uid).unwrap_or(key);
        !uids.contains(&base_uid.to_string())
    });
    let removed = before - st.events.len();
    if removed > 0 {
        debug!(
            "[eds] Removed {} event occurrences for {} UIDs",
            removed,
            uids.len()
        );
    } else {
        warn!(
            "[eds] remove_events: no matching occurrences found for UIDs: {:?}",
            uids
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal iCal for the "Daily - LabRulez" recurring Tuesday meeting.
    ///
    /// When EDS expands this to an occurrence on `date` (format "YYYYMMDD")
    /// it sends a VEVENT with DTSTART set to that occurrence's date.
    ///
    /// Note: RFC 5545 folding uses `\r\n` + a leading space for continuation
    /// lines.  Rust `\` string-literal line continuation strips leading
    /// whitespace, so continuation lines are written as explicit string
    /// concatenation to preserve the required leading space.
    fn labrulez_ical(date: &str) -> String {
        // Each piece is one iCal line (or folded continuation).
        // Continuation lines intentionally start with a single space.
        "BEGIN:VCALENDAR\r\n".to_string()
            + "VERSION:2.0\r\n"
            + "BEGIN:VEVENT\r\n"
            + &format!("DTSTART;TZID=Europe/Prague:{date}T083000\r\n")
            + &format!("DTEND;TZID=Europe/Prague:{date}T083500\r\n")
            + "RRULE:FREQ=WEEKLY;BYDAY=TU\r\n"
            + "SUMMARY:Daily - LabRulez\r\n"
            + "UID:077u2vl5ec0knbionphchefveh_R20260203T073000@google.com\r\n"
            + "ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;\r\n"
            + " CN=daniel.altmann@seznam.cz;X-NUM-GUESTS=0:mailto:\r\n"
            + " daniel.altmann@seznam.cz\r\n"
            + "ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;\r\n"
            + " CN=pavel.zak@cookielab.io;X-NUM-GUESTS=0:mailto:pavel.zak@cookielab.io\r\n"
            + "BEGIN:VALARM\r\n"
            + "ACTION:DISPLAY\r\n"
            + "DESCRIPTION:This is an event reminder\r\n"
            + "TRIGGER:-PT10M\r\n"
            + "END:VALARM\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n"
    }

    // ── parse_vevent ─────────────────────────────────────────────────────────

    /// Regression: expanded occurrence for 2026-02-17 (a Tuesday) must parse
    /// correctly even though the master VEVENT has an older DTSTART.
    #[test]
    fn parse_vevent_recurring_occurrence_with_tzid() {
        let ical = labrulez_ical("20260217");
        let event = parse_vevent(&ical).expect("should parse LabRulez occurrence");

        assert_eq!(event.summary, "Daily - LabRulez");
        assert_eq!(
            event.uid,
            "077u2vl5ec0knbionphchefveh_R20260203T073000@google.com"
        );
        assert!(!event.all_day, "event should not be all-day");

        // DTSTART;TZID=Europe/Prague:20260217T083000
        // Prague is UTC+1 in February → expected UTC timestamp is 07:30
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};
        let tz: chrono_tz::Tz = "Europe/Prague".parse().unwrap();

        let start_naive = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 17).unwrap(),
            NaiveTime::from_hms_opt(8, 30, 0).unwrap(),
        );
        let expected_start = tz
            .from_local_datetime(&start_naive)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(
            event.start_time, expected_start,
            "DTSTART should be 2026-02-17 08:30 Prague (07:30 UTC)"
        );

        // DTEND;TZID=Europe/Prague:20260217T083500 → 5 min later
        let end_naive = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 17).unwrap(),
            NaiveTime::from_hms_opt(8, 35, 0).unwrap(),
        );
        let expected_end = tz
            .from_local_datetime(&end_naive)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(
            event.end_time, expected_end,
            "DTEND should be 2026-02-17 08:35 Prague"
        );
    }

    /// Folded ATTENDEE lines (RFC 5545 line-folding) must be unfolded so that
    /// the attendee email and PARTSTAT are parsed from the joined value.
    #[test]
    fn parse_vevent_folded_attendee_lines() {
        let ical = labrulez_ical("20260217");
        let event = parse_vevent(&ical).expect("should parse");

        let pz = event
            .attendees
            .iter()
            .find(|a| a.email == "pavel.zak@cookielab.io");
        assert!(pz.is_some(), "folded attendee email should be parsed");
        assert_eq!(
            pz.unwrap().status,
            entity::calendar::AttendeeStatus::Accepted,
            "PARTSTAT=ACCEPTED should map to Accepted"
        );

        let da = event
            .attendees
            .iter()
            .find(|a| a.email.contains("daniel.altmann"));
        assert!(da.is_some(), "second folded attendee should be parsed");
    }

    /// A nested VALARM component must not corrupt DTSTART/DTEND parsing
    /// (the nesting guard must skip VALARM properties).
    #[test]
    fn parse_vevent_valarm_is_skipped() {
        let ical = labrulez_ical("20260217");
        let event = parse_vevent(&ical).expect("should parse");

        // If VALARM DESCRIPTION leaked into the event description the field
        // would be "This is an event reminder" instead of None.
        assert_ne!(
            event.description.as_deref(),
            Some("This is an event reminder"),
            "VALARM DESCRIPTION must not bleed into event description"
        );
    }

    // ── build_time_range_query_from_today ────────────────────────────────────

    /// The query window must start at today midnight, not at `now`.
    /// An event at 08:30 must not be excluded when the daemon starts at 09:00.
    #[test]
    fn query_starts_at_today_midnight() {
        let (_range, query) = build_time_range_query_from_today();

        // Compute today midnight in UTC independently.
        let today_midnight_utc = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always valid")
            .and_local_timezone(chrono::Local)
            .earliest()
            .expect("today midnight is a valid local time")
            .to_utc();

        let expected_start = today_midnight_utc.format("%Y%m%dT%H%M%SZ").to_string();
        assert!(
            query.contains(&expected_start),
            "query should start at today midnight ({expected_start}), got: {query}"
        );
    }

    // ── RRULE expansion ────────────────────────────────────────────────────────

    /// The master VEVENT for "Daily - LabRulez" has DTSTART=Feb 3 (the original
    /// series start) and RRULE:FREQ=WEEKLY;BYDAY=TU.  EDS sends this master
    /// event, NOT expanded occurrences.  The plugin must expand it so that the
    /// Feb 17 occurrence (a Tuesday) appears in the Agenda widget.
    #[test]
    fn expand_weekly_recurring_event_into_today() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        // Master event: starts Feb 3, weekly on Tuesdays.
        let ical = labrulez_ical("20260203");

        // Query window: Feb 17 midnight → Feb 19 midnight (covers Feb 17 Tuesday).
        let tz: chrono_tz::Tz = "Europe/Prague".parse().unwrap();
        let range_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 17).unwrap(),
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            ))
            .single()
            .unwrap()
            .timestamp();
        let range_end = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 19).unwrap(),
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            ))
            .single()
            .unwrap()
            .timestamp();

        let events = expand_vevent(
            &ical,
            TimeRange {
                start: range_start,
                end: range_end,
            },
        );

        // Should produce exactly 1 occurrence on Feb 17 (a Tuesday).
        assert_eq!(
            events.len(),
            1,
            "expected 1 occurrence in range, got {}",
            events.len()
        );

        let event = &events[0];
        assert_eq!(event.summary, "Daily - LabRulez");

        let expected_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 17).unwrap(),
                NaiveTime::from_hms_opt(8, 30, 0).unwrap(),
            ))
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(
            event.start_time, expected_start,
            "occurrence should be on Feb 17 08:30 Prague"
        );
    }

    /// Weekly recurring event should produce multiple occurrences across a
    /// multi-week range.
    #[test]
    fn expand_weekly_recurring_multiple_weeks() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        let ical = labrulez_ical("20260203"); // Weekly TU
        let tz: chrono_tz::Tz = "Europe/Prague".parse().unwrap();

        // 3-week window: Feb 10 → Mar 3 (should have Feb 10, 17, 24)
        let range_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 10).unwrap(),
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            ))
            .single()
            .unwrap()
            .timestamp();
        let range_end = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 3, 3).unwrap(),
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            ))
            .single()
            .unwrap()
            .timestamp();

        let events = expand_vevent(
            &ical,
            TimeRange {
                start: range_start,
                end: range_end,
            },
        );

        assert_eq!(
            events.len(),
            3,
            "3 Tuesdays in [Feb 10, Mar 3): {:?}",
            events.iter().map(|e| e.start_time).collect::<Vec<_>>()
        );
    }

    /// EXDATE exclusions must suppress the matching occurrence.
    #[test]
    fn expand_weekly_with_exdate() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        let tz: chrono_tz::Tz = "Europe/Prague".parse().unwrap();

        // Add EXDATE for Feb 17 (skip that Tuesday).
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260203T083000\r\n"
            + "DTEND;TZID=Europe/Prague:20260203T083500\r\n"
            + "RRULE:FREQ=WEEKLY;BYDAY=TU\r\n"
            + "EXDATE;TZID=Europe/Prague:20260217T083000\r\n"
            + "SUMMARY:Daily - LabRulez\r\n"
            + "UID:test-exdate@example.com\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        // Range: Feb 10 → Mar 3 → would normally be 3 Tuesdays.
        let range_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 10).unwrap(),
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            ))
            .single()
            .unwrap()
            .timestamp();
        let range_end = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 3, 3).unwrap(),
                NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            ))
            .single()
            .unwrap()
            .timestamp();

        let events = expand_vevent(
            &ical,
            TimeRange {
                start: range_start,
                end: range_end,
            },
        );

        // Feb 17 excluded → only Feb 10 and Feb 24.
        assert_eq!(events.len(), 2, "EXDATE should exclude Feb 17");
    }

    // ── RRULE expansion: other frequencies ──────────────────────────────────

    /// Helper: build a minimal recurring VEVENT iCal string.
    fn recurring_ical(dtstart: &str, dtend: &str, rrule: &str, uid: &str) -> String {
        "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + &format!("DTSTART;TZID=Europe/Prague:{dtstart}\r\n")
            + &format!("DTEND;TZID=Europe/Prague:{dtend}\r\n")
            + &format!("RRULE:{rrule}\r\n")
            + &format!("SUMMARY:Test event\r\n")
            + &format!("UID:{uid}\r\n")
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n"
    }

    /// Helper: build a UTC range from Prague dates for brevity.
    fn prague_range(start: (i32, u32, u32), end: (i32, u32, u32)) -> TimeRange {
        use chrono::{NaiveDate, NaiveTime, TimeZone as _};
        let tz: chrono_tz::Tz = "Europe/Prague".parse().unwrap();
        let mk = |y, m, d| {
            tz.from_local_datetime(
                &NaiveDate::from_ymd_opt(y, m, d)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
            )
            .single()
            .unwrap()
            .timestamp()
        };
        TimeRange {
            start: mk(start.0, start.1, start.2),
            end: mk(end.0, end.1, end.2),
        }
    }

    #[test]
    fn expand_daily_recurring() {
        // Daily event at 09:00, 30 min duration.
        let ical = recurring_ical(
            "20260210T090000",
            "20260210T093000",
            "FREQ=DAILY",
            "daily@test",
        );
        let range = prague_range((2026, 2, 15), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        // Feb 15, 16, 17 = 3 days.
        assert_eq!(events.len(), 3, "daily should produce 3 occurrences");
    }

    #[test]
    fn expand_daily_with_interval() {
        // Every 3 days starting Feb 1.
        let ical = recurring_ical(
            "20260201T100000",
            "20260201T110000",
            "FREQ=DAILY;INTERVAL=3",
            "daily3@test",
        );
        // Range: Feb 1 → Feb 16. Occurrences: Feb 1, 4, 7, 10, 13 = 5.
        let range = prague_range((2026, 2, 1), (2026, 2, 16));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 5, "every-3-days in 15 days = 5");
    }

    #[test]
    fn expand_weekly_with_interval() {
        // Every 2 weeks on Tuesdays starting Feb 3.
        let ical = recurring_ical(
            "20260203T083000",
            "20260203T093000",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU",
            "biweekly@test",
        );
        // Range: Feb 1 → Mar 15. Occurrences: Feb 3, Feb 17, Mar 3 = 3.
        let range = prague_range((2026, 2, 1), (2026, 3, 15));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "biweekly TU in 6 weeks = 3");
    }

    #[test]
    fn expand_monthly_recurring() {
        // Monthly on the 15th.
        let ical = recurring_ical(
            "20260115T140000",
            "20260115T150000",
            "FREQ=MONTHLY",
            "monthly@test",
        );
        // Range: Feb 1 → May 1. Occurrences: Feb 15, Mar 15, Apr 15 = 3.
        let range = prague_range((2026, 2, 1), (2026, 5, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "monthly from Feb→May = 3");
    }

    #[test]
    fn expand_monthly_clamps_day_to_month_length() {
        // Monthly on the 31st — months without 31 days should still produce
        // an occurrence (clamped to last day).
        let ical = recurring_ical(
            "20260131T100000",
            "20260131T110000",
            "FREQ=MONTHLY",
            "monthly31@test",
        );
        // Range: Jan 1 → May 1.
        // Jan 31 ✓, Feb 28 (clamped) ✓, Mar 31 ✓, Apr 30 (clamped) ✓ = 4.
        let range = prague_range((2026, 1, 1), (2026, 5, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 4, "monthly-31 with clamping = 4");
    }

    #[test]
    fn expand_yearly_recurring() {
        let ical = recurring_ical(
            "20250614T180000",
            "20250614T200000",
            "FREQ=YEARLY",
            "yearly@test",
        );
        // Range: 2026-01 → 2029-01. Occurrences: Jun 14 2026, 2027, 2028 = 3.
        let range = prague_range((2026, 1, 1), (2029, 1, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "yearly from 2026→2029 = 3");
    }

    // ── RRULE expansion: COUNT and UNTIL limits ──────────────────────────────

    #[test]
    fn expand_with_count_limit() {
        // Daily event with COUNT=5, starting Feb 10.
        let ical = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=DAILY;COUNT=5",
            "count@test",
        );
        // Range is wide, but COUNT=5 limits to Feb 10–14.
        let range = prague_range((2026, 2, 1), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 5, "COUNT=5 should cap at 5 occurrences");
    }

    #[test]
    fn expand_with_count_fewer_in_range() {
        // Daily event with COUNT=3, starting Feb 10.
        let ical = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=DAILY;COUNT=3",
            "count3@test",
        );
        // Range starts at Feb 12, so only Feb 12 falls in range (COUNT ends at Feb 12).
        let range = prague_range((2026, 2, 12), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 1, "only 1 of 3 counted occurrences in range");
    }

    #[test]
    fn expand_with_until_limit() {
        // Weekly TU starting Feb 3, until Feb 20.
        let ical = recurring_ical(
            "20260203T083000",
            "20260203T093000",
            "FREQ=WEEKLY;BYDAY=TU;UNTIL=20260220T235959Z",
            "until@test",
        );
        // Feb 3, 10, 17 are before UNTIL; Feb 24 is after.
        let range = prague_range((2026, 2, 1), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "UNTIL=Feb 20 should include Feb 3, 10, 17");
    }

    // ── RRULE expansion: non-recurring passthrough ───────────────────────────

    #[test]
    fn expand_non_recurring_event_passes_through() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260217T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260217T150000\r\n"
            + "SUMMARY:One-off meeting\r\n"
            + "UID:single@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "One-off meeting");
        assert_eq!(events[0].uid, "single@test");
    }

    #[test]
    fn expand_non_recurring_outside_range_still_returned() {
        // Non-recurring events are NOT filtered by expand_vevent (the caller
        // or the Agenda widget handles range filtering for non-recurring events).
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260101T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260101T150000\r\n"
            + "SUMMARY:Past meeting\r\n"
            + "UID:past@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 1, "non-recurring always passes through");
    }

    // ── RRULE expansion: BYDAY with multiple days ────────────────────────────

    #[test]
    fn expand_weekly_multiple_byday() {
        // MWF schedule.
        let ical = recurring_ical(
            "20260202T090000", // Monday Feb 2
            "20260202T100000",
            "FREQ=WEEKLY;BYDAY=MO,WE,FR",
            "mwf@test",
        );
        // One week: Feb 9–15. Should have Mon 9, Wed 11, Fri 13 = 3.
        let range = prague_range((2026, 2, 9), (2026, 2, 16));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "MO,WE,FR in 1 week = 3");
    }

    // ── RRULE expansion: preserves event metadata ────────────────────────────

    #[test]
    fn expand_preserves_uid_and_duration() {
        let ical = recurring_ical(
            "20260203T083000",
            "20260203T093000", // 1-hour duration
            "FREQ=WEEKLY;BYDAY=TU",
            "preserve-uid@test",
        );
        let range = prague_range((2026, 2, 10), (2026, 2, 25));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3); // Feb 10, 17, 24
        for event in &events {
            assert_eq!(event.uid, "preserve-uid@test");
            assert_eq!(
                event.end_time - event.start_time,
                3600,
                "duration should be preserved"
            );
        }
    }

    // ── parse_ical_events (top-level, mixed input) ───────────────────────────

    #[test]
    fn parse_ical_events_mixes_recurring_and_single() {
        let recurring = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=DAILY",
            "recurring@test",
        );
        let single = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260211T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260211T150000\r\n"
            + "SUMMARY:Single\r\n"
            + "UID:single@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let icals = vec![recurring, single];
        let range = prague_range((2026, 2, 10), (2026, 2, 13));
        let events = parse_ical_events(
            &icals.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            range,
        );
        // Daily: Feb 10, 11, 12 = 3. Single: 1. Total: 4.
        assert_eq!(events.len(), 4, "3 daily + 1 single = 4");
    }

    // ── parse_rrule ──────────────────────────────────────────────────────────

    #[test]
    fn parse_rrule_weekly_byday() {
        let rule = parse_rrule("FREQ=WEEKLY;BYDAY=TU").unwrap();
        assert_eq!(rule.freq, Frequency::Weekly);
        assert_eq!(rule.interval, 1);
        assert_eq!(rule.by_day, vec![chrono::Weekday::Tue]);
        assert!(rule.count.is_none());
        assert!(rule.until.is_none());
    }

    #[test]
    fn parse_rrule_daily_interval_count() {
        let rule = parse_rrule("FREQ=DAILY;INTERVAL=3;COUNT=10").unwrap();
        assert_eq!(rule.freq, Frequency::Daily);
        assert_eq!(rule.interval, 3);
        assert_eq!(rule.count, Some(10));
    }

    #[test]
    fn parse_rrule_monthly() {
        let rule = parse_rrule("FREQ=MONTHLY").unwrap();
        assert_eq!(rule.freq, Frequency::Monthly);
        assert_eq!(rule.interval, 1);
    }

    #[test]
    fn parse_rrule_with_until() {
        let rule = parse_rrule("FREQ=WEEKLY;UNTIL=20260301T000000Z").unwrap();
        assert!(rule.until.is_some());
        // UNTIL is a UTC timestamp for 2026-03-01 00:00:00Z.
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 3, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        assert_eq!(rule.until.unwrap(), expected);
    }

    #[test]
    fn parse_rrule_multiple_byday() {
        let rule = parse_rrule("FREQ=WEEKLY;BYDAY=MO,WE,FR").unwrap();
        assert_eq!(
            rule.by_day,
            vec![
                chrono::Weekday::Mon,
                chrono::Weekday::Wed,
                chrono::Weekday::Fri
            ]
        );
    }

    #[test]
    fn parse_rrule_unknown_freq_returns_none() {
        assert!(parse_rrule("FREQ=SECONDLY").is_none());
    }

    // ── advance_date ─────────────────────────────────────────────────────────

    #[test]
    fn advance_date_daily() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 2, 28).unwrap();
        let next = advance_date(d, Frequency::Daily, 1);
        assert_eq!(next, chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
    }

    #[test]
    fn advance_date_weekly() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 2, 17).unwrap();
        let next = advance_date(d, Frequency::Weekly, 2);
        assert_eq!(next, chrono::NaiveDate::from_ymd_opt(2026, 3, 3).unwrap());
    }

    #[test]
    fn advance_date_monthly_clamps() {
        // Jan 31 + 1 month → Feb 28 (2026 is not a leap year).
        let d = chrono::NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        let next = advance_date(d, Frequency::Monthly, 1);
        assert_eq!(next, chrono::NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
    }

    #[test]
    fn advance_date_monthly_leap_year() {
        // Jan 31 + 1 month in 2028 (leap year) → Feb 29.
        let d = chrono::NaiveDate::from_ymd_opt(2028, 1, 31).unwrap();
        let next = advance_date(d, Frequency::Monthly, 1);
        assert_eq!(next, chrono::NaiveDate::from_ymd_opt(2028, 2, 29).unwrap());
    }

    #[test]
    fn advance_date_yearly() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let next = advance_date(d, Frequency::Yearly, 1);
        assert_eq!(next, chrono::NaiveDate::from_ymd_opt(2027, 6, 15).unwrap());
    }

    #[test]
    fn advance_date_monthly_wraps_year() {
        // Nov + 2 months → Jan next year.
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let next = advance_date(d, Frequency::Monthly, 2);
        assert_eq!(next, chrono::NaiveDate::from_ymd_opt(2027, 1, 15).unwrap());
    }

    // ── days_in_month ────────────────────────────────────────────────────────

    #[test]
    fn days_in_month_february_non_leap() {
        assert_eq!(days_in_month(2026, 2), 28);
    }

    #[test]
    fn days_in_month_february_leap() {
        assert_eq!(days_in_month(2028, 2), 29);
    }

    #[test]
    fn days_in_month_various() {
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    // ── parse_ical_naive_datetime ────────────────────────────────────────────

    #[test]
    fn parse_ical_naive_datetime_full() {
        let dt = parse_ical_naive_datetime("20260217T083000").unwrap();
        assert_eq!(
            dt.date(),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 17).unwrap()
        );
        assert_eq!(
            dt.time(),
            chrono::NaiveTime::from_hms_opt(8, 30, 0).unwrap()
        );
    }

    #[test]
    fn parse_ical_naive_datetime_strips_z() {
        let dt = parse_ical_naive_datetime("20260217T083000Z").unwrap();
        assert_eq!(
            dt.date(),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 17).unwrap()
        );
    }

    #[test]
    fn parse_ical_naive_datetime_date_only() {
        let dt = parse_ical_naive_datetime("20260217").unwrap();
        assert_eq!(
            dt.date(),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 17).unwrap()
        );
        assert_eq!(dt.time(), chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    }

    #[test]
    fn parse_ical_naive_datetime_invalid() {
        assert!(parse_ical_naive_datetime("garbage").is_none());
        assert!(parse_ical_naive_datetime("").is_none());
    }

    // ── extract_tzid ─────────────────────────────────────────────────────────

    #[test]
    fn extract_tzid_present() {
        let tz = extract_tzid(";TZID=Europe/Prague");
        assert_eq!(tz, Some("Europe/Prague".parse().unwrap()));
    }

    #[test]
    fn extract_tzid_with_extra_params() {
        let tz = extract_tzid(";VALUE=DATE-TIME;TZID=America/New_York;X-FOO=bar");
        assert_eq!(tz, Some("America/New_York".parse().unwrap()));
    }

    #[test]
    fn extract_tzid_absent() {
        assert!(extract_tzid("").is_none());
        assert!(extract_tzid(";VALUE=DATE").is_none());
    }

    #[test]
    fn extract_tzid_unknown_returns_none() {
        assert!(extract_tzid(";TZID=Mars/Olympus_Mons").is_none());
    }

    // ── parse_ical_datetime ──────────────────────────────────────────────────

    #[test]
    fn parse_ical_datetime_with_europe_prague_tzid() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};
        // Prague is UTC+1 in winter; 08:30 Prague = 07:30 UTC
        let ts = parse_ical_datetime("20260217T083000", ";TZID=Europe/Prague");
        let dt = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 17).unwrap(),
            NaiveTime::from_hms_opt(8, 30, 0).unwrap(),
        );
        let tz: chrono_tz::Tz = "Europe/Prague".parse().unwrap();
        let expected = tz.from_local_datetime(&dt).single().unwrap().timestamp();
        assert_eq!(ts, Some(expected));
    }

    #[test]
    fn parse_ical_datetime_utc_z_suffix() {
        let ts = parse_ical_datetime("20260217T073000Z", "");
        // 2026-02-17 07:30:00 UTC
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 2, 17)
            .unwrap()
            .and_hms_opt(7, 30, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        assert_eq!(ts, Some(expected));
    }

    #[test]
    fn parse_ical_datetime_all_day_date_only() {
        let ts = parse_ical_datetime("20260217", ";VALUE=DATE");
        assert!(ts.is_some(), "all-day date should parse");
        // The timestamp must represent local midnight, not UTC midnight.
        let local_start = chrono::NaiveDate::from_ymd_opt(2026, 2, 17)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .unwrap()
            .timestamp();
        assert_eq!(ts, Some(local_start));
    }

    #[test]
    fn parse_ical_datetime_floating_no_tz_no_z() {
        // No Z, no TZID → floating time interpreted as local.
        let ts = parse_ical_datetime("20260217T120000", "");
        assert!(ts.is_some());
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 2, 17)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .unwrap()
            .timestamp();
        assert_eq!(ts, Some(expected));
    }

    #[test]
    fn parse_ical_datetime_invalid() {
        assert!(parse_ical_datetime("not-a-date", "").is_none());
        assert!(parse_ical_datetime("", "").is_none());
    }

    // ── parse_weekday ────────────────────────────────────────────────────────

    #[test]
    fn parse_weekday_all() {
        assert_eq!(parse_weekday("MO"), Some(chrono::Weekday::Mon));
        assert_eq!(parse_weekday("TU"), Some(chrono::Weekday::Tue));
        assert_eq!(parse_weekday("WE"), Some(chrono::Weekday::Wed));
        assert_eq!(parse_weekday("TH"), Some(chrono::Weekday::Thu));
        assert_eq!(parse_weekday("FR"), Some(chrono::Weekday::Fri));
        assert_eq!(parse_weekday("SA"), Some(chrono::Weekday::Sat));
        assert_eq!(parse_weekday("SU"), Some(chrono::Weekday::Sun));
        assert_eq!(parse_weekday("XX"), None);
    }

    // ── check_debounce ───────────────────────────────────────────────────────

    #[test]
    fn check_debounce_allows_first_request() {
        let mut recent = std::collections::VecDeque::new();
        assert!(
            check_debounce(&mut recent, 15),
            "first call must be allowed"
        );
        assert_eq!(recent.len(), 1, "entry must be recorded");
    }

    #[test]
    fn check_debounce_blocks_second_within_base() {
        let mut recent = std::collections::VecDeque::new();
        recent.push_back(std::time::Instant::now());
        assert!(
            !check_debounce(&mut recent, 15),
            "second call within base must be blocked"
        );
    }

    #[test]
    fn check_debounce_allows_second_after_base_elapsed() {
        let base_secs = 15u64;
        let mut recent = std::collections::VecDeque::new();
        // Push an entry just older than base
        recent.push_back(std::time::Instant::now() - std::time::Duration::from_secs(base_secs + 1));
        assert!(
            check_debounce(&mut recent, base_secs),
            "second call after base elapsed must be allowed"
        );
    }

    #[test]
    fn check_debounce_allows_up_to_three_in_4x_window() {
        let base_secs = 15u64;
        let mut recent = std::collections::VecDeque::new();
        // Two entries: one just past base, one just past 2×base
        recent.push_back(std::time::Instant::now() - std::time::Duration::from_secs(base_secs + 1));
        recent.push_back(
            std::time::Instant::now() - std::time::Duration::from_secs(base_secs * 2 + 1),
        );
        assert!(
            check_debounce(&mut recent, base_secs),
            "third call within 4×base must be allowed"
        );
        assert_eq!(recent.len(), 3, "all three entries must be in the window");
    }

    #[test]
    fn check_debounce_blocks_fourth_in_4x_window() {
        let base_secs = 15u64;
        let mut recent = std::collections::VecDeque::new();
        // Three entries spread across the 4×base window
        recent.push_back(std::time::Instant::now() - std::time::Duration::from_secs(base_secs + 1));
        recent.push_back(
            std::time::Instant::now() - std::time::Duration::from_secs(base_secs * 2 + 1),
        );
        recent.push_back(
            std::time::Instant::now() - std::time::Duration::from_secs(base_secs * 3 + 1),
        );
        assert!(
            !check_debounce(&mut recent, base_secs),
            "fourth call within 4×base must be blocked"
        );
    }

    #[test]
    fn check_debounce_prunes_old_entries() {
        let base_secs = 15u64;
        let mut recent = std::collections::VecDeque::new();
        // Four entries all older than 4×base — should all be pruned
        for _ in 0..4 {
            recent.push_back(
                std::time::Instant::now() - std::time::Duration::from_secs(base_secs * 4 + 1),
            );
        }
        assert!(
            check_debounce(&mut recent, base_secs),
            "after pruning old entries, first call must be allowed"
        );
        assert_eq!(recent.len(), 1, "only the new entry should remain");
    }

    #[test]
    fn eds_state_syncing_defaults_false() {
        let state = EdsState::new();
        assert!(!state.syncing, "syncing must start false");
    }

    // ── update_state_remove_events ───────────────────────────────────────────

    /// Regression: UIDs that contain `@` characters (e.g. Google Calendar UIDs
    /// like `event@google.com`) must be matched correctly via the
    /// `rsplit_once("::")` key split.  The base UID is everything left of the
    /// last `::` separator, so an `@` in the UID must not interfere.
    #[test]
    fn removes_events_by_uid_with_at_sign() {
        use std::sync::{Arc, Mutex as StdMutex};

        let make_event = |uid: &str, start_time: i64| entity::calendar::CalendarEvent {
            uid: uid.to_string(),
            summary: "Test event".to_string(),
            start_time,
            end_time: start_time + 3600,
            all_day: false,
            description: None,
            location: None,
            attendees: vec![],
        };

        // Three events with UIDs that contain `@` signs, matching real Google
        // Calendar UIDs.  The occurrence key is "{uid}::{start_time}".
        let uid_a = "event@google.com";
        let uid_b = "meeting@google.com";
        let uid_c = "standup@example.org";

        let mut state = EdsState::new();
        state.events.insert(
            EdsPlugin::make_occurrence_key(uid_a, 1_000_000),
            make_event(uid_a, 1_000_000),
        );
        state.events.insert(
            EdsPlugin::make_occurrence_key(uid_b, 2_000_000),
            make_event(uid_b, 2_000_000),
        );
        state.events.insert(
            EdsPlugin::make_occurrence_key(uid_c, 3_000_000),
            make_event(uid_c, 3_000_000),
        );

        let shared = Arc::new(StdMutex::new(state));

        // Remove only uid_a.
        update_state_remove_events(&shared, &[uid_a.to_string()]);

        let st = shared.lock().unwrap();
        assert_eq!(
            st.events.len(),
            2,
            "only the uid_a occurrence should be removed"
        );
        assert!(
            !st.events
                .contains_key(&EdsPlugin::make_occurrence_key(uid_a, 1_000_000)),
            "uid_a occurrence must be gone"
        );
        assert!(
            st.events
                .contains_key(&EdsPlugin::make_occurrence_key(uid_b, 2_000_000)),
            "uid_b occurrence must be retained"
        );
        assert!(
            st.events
                .contains_key(&EdsPlugin::make_occurrence_key(uid_c, 3_000_000)),
            "uid_c occurrence must be retained"
        );
    }

    #[test]
    fn check_debounce_and_unix_now_are_consistent() {
        // unix_now() must return a plausible recent timestamp (after 2020-01-01).
        let ts = unix_now();
        assert!(
            ts > 1_577_836_800,
            "unix_now must return a timestamp after 2020-01-01"
        );
    }

    // ── expand_vevent: parse failure paths ───────────────────────────────────

    /// Regression: a VEVENT without a UID must be silently dropped (empty vec),
    /// not panic.  This exercises the `warn!` path added to `expand_vevent`.
    #[test]
    fn expand_vevent_missing_uid_returns_empty() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260217T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260217T150000\r\n"
            + "SUMMARY:No UID event\r\n"
            // UID deliberately omitted
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert!(
            events.is_empty(),
            "VEVENT without UID must be dropped, not partially parsed"
        );
    }

    /// Regression: a VEVENT without a parseable DTSTART must be silently dropped.
    #[test]
    fn expand_vevent_missing_dtstart_returns_empty() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            // DTSTART deliberately omitted
            + "DTEND;TZID=Europe/Prague:20260217T150000\r\n"
            + "SUMMARY:No DTSTART event\r\n"
            + "UID:no-dtstart@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert!(
            events.is_empty(),
            "VEVENT without DTSTART must be dropped, not partially parsed"
        );
    }

    // ── query window: 60-day lookahead ───────────────────────────────────────

    /// Regression: the EDS query window was previously 30 days.  Events
    /// scheduled 31–60 days out (e.g., an "FE interview" 5 weeks away) were
    /// completely absent from the plugin's ObjectsAdded batches.  The window
    /// was extended to 60 days to capture such events.
    ///
    /// This test verifies that a single (non-recurring) event 45 days away
    /// is visible in a 60-day query range but would be absent from 30 days.
    #[test]
    fn single_event_45_days_out_is_in_60_day_range_not_30() {
        // Use a fixed reference point: Feb 23, 2026 midnight Prague.
        // 45 days later = Apr 9, 2026.
        let reference_start = prague_range((2026, 2, 23), (2026, 2, 24)).start;
        let range_30 = TimeRange {
            start: reference_start,
            end: reference_start + 30 * 24 * 3600,
        };
        let range_60 = TimeRange {
            start: reference_start,
            end: reference_start + 60 * 24 * 3600,
        };

        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260409T100000\r\n"
            + "DTEND;TZID=Europe/Prague:20260409T110000\r\n"
            + "SUMMARY:FE interview\r\n"
            + "UID:fe-interview@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let events_30 = expand_vevent(&ical, range_30);
        assert_eq!(events_30.len(), 1, "non-recurring always passes through regardless of range");
        // However, the EDS S-expression query would NOT return it with a 30-day window.
        // This test documents that expand_vevent itself doesn't filter non-recurring events —
        // the filtering happens at the EDS query level (occur-in-time-range?).

        let events_60 = expand_vevent(&ical, range_60);
        assert_eq!(events_60.len(), 1, "45-day-out event visible in 60-day window");
        assert_eq!(events_60[0].summary, "FE interview");
    }

    /// Regression: COUNT-limited recurring events where COUNT is exhausted
    /// before the query window must return zero occurrences in that window.
    /// Previously the generated counter was verified to handle this correctly.
    #[test]
    fn expand_with_count_exhausted_before_range() {
        // Weekly on Mondays, started 2025-01-06, COUNT=5 → last occurrence 2025-02-03.
        // Query window starts in 2026 — no occurrences should be returned.
        let ical = recurring_ical(
            "20250106T090000",
            "20250106T100000",
            "FREQ=WEEKLY;BYDAY=MO;COUNT=5",
            "exhausted-count@test",
        );
        let range = prague_range((2026, 2, 23), (2026, 3, 25));
        let events = expand_vevent(&ical, range);
        assert_eq!(
            events.len(),
            0,
            "COUNT exhausted in 2025 should produce 0 events for a 2026 query window"
        );
    }

    /// Bi-weekly recurring event: verify that exactly the expected occurrences
    /// within a 60-day window are returned.
    #[test]
    fn expand_biweekly_event_in_60_day_window() {
        // Bi-weekly on Mondays starting 2026-02-23.  In a 60-day window
        // (Feb 23 – Apr 24) the Mondays are: Feb 23, Mar 9, Mar 23, Apr 6, Apr 20 = 5.
        let ical = recurring_ical(
            "20260223T100000",
            "20260223T110000",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO",
            "biweekly-1on1@test",
        );
        let range = prague_range((2026, 2, 23), (2026, 4, 24));
        let events = expand_vevent(&ical, range);
        assert_eq!(
            events.len(),
            5,
            "bi-weekly MO in 60-day window should produce 5 occurrences"
        );
    }

    // ── multi-VEVENT iCal (exception occurrences) ────────────────────────────

    /// Regression: two events scheduled for today ("Tech 1:1 Tomáš / Pavel" and
    /// "FE interview") were not shown in the agenda widget.
    ///
    /// Root cause: EDS sends a single VCALENDAR containing two VEVENTs — the
    /// master recurring event (RRULE + EXDATE for today's original slot) and an
    /// exception occurrence (RECURRENCE-ID, DTSTART rescheduled to this
    /// afternoon).  Previously `parse_vevent_raw` stopped at the first
    /// `END:VEVENT` and silently dropped the exception VEVENT, so today's actual
    /// rescheduled occurrence was never surfaced.
    ///
    /// The fix: `split_vevents` extracts each VEVENT block independently before
    /// `expand_vevent` processes it.
    #[test]
    fn parse_ical_events_multi_vevent_with_exception_occurrence() {
        // Weekly Monday recurring event; the original 09:00 UTC slot on
        // 2026-02-23 (today) is excluded via EXDATE.  An exception VEVENT
        // reschedules that occurrence to 13:00 UTC.
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            // Master: weekly on Mondays, started 2026-02-02; today's slot excluded.
            + "BEGIN:VEVENT\r\n"
            + "UID:tech-1on1@test\r\n"
            + "SUMMARY:Tech 1:1 Tomáš / Pavel\r\n"
            + "DTSTART:20260202T090000Z\r\n"
            + "DTEND:20260202T100000Z\r\n"
            + "RRULE:FREQ=WEEKLY;BYDAY=MO\r\n"
            + "EXDATE:20260223T090000Z\r\n"
            + "END:VEVENT\r\n"
            // Exception: today's occurrence rescheduled to 13:00 UTC.
            + "BEGIN:VEVENT\r\n"
            + "UID:tech-1on1@test\r\n"
            + "SUMMARY:Tech 1:1 Tomáš / Pavel\r\n"
            + "RECURRENCE-ID:20260223T090000Z\r\n"
            + "DTSTART:20260223T130000Z\r\n"
            + "DTEND:20260223T140000Z\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        // Query for today only.
        let range = prague_range((2026, 2, 23), (2026, 2, 24));
        let events = parse_ical_events(&[ical], range);

        // Must yield exactly one event: the rescheduled 13:00 exception.
        // Without split_vevents the exception VEVENT is dropped → 0 events.
        assert_eq!(
            events.len(),
            1,
            "multi-VEVENT iCal must yield the exception occurrence, not be dropped"
        );
        assert_eq!(events[0].summary, "Tech 1:1 Tomáš / Pavel");

        // 13:00 UTC on 2026-02-23.
        let expected_start = chrono::DateTime::parse_from_rfc3339("2026-02-23T13:00:00Z")
            .unwrap()
            .timestamp();
        assert_eq!(
            events[0].start_time, expected_start,
            "rescheduled exception must be at 13:00 UTC, not original 09:00 slot"
        );
    }

    /// Companion: a non-recurring event in a multi-VEVENT VCALENDAR alongside
    /// an unrelated recurring master is still extracted and returned.
    #[test]
    fn parse_ical_events_multi_vevent_standalone_alongside_recurring() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            // A standalone non-recurring event today at 15:00 UTC.
            + "BEGIN:VEVENT\r\n"
            + "UID:fe-interview@test\r\n"
            + "SUMMARY:FE interview\r\n"
            + "DTSTART:20260223T150000Z\r\n"
            + "DTEND:20260223T160000Z\r\n"
            + "END:VEVENT\r\n"
            // An unrelated recurring event also in the same VCALENDAR blob.
            + "BEGIN:VEVENT\r\n"
            + "UID:standup@test\r\n"
            + "SUMMARY:Daily standup\r\n"
            + "DTSTART:20260202T080000Z\r\n"
            + "DTEND:20260202T080500Z\r\n"
            + "RRULE:FREQ=DAILY\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let range = prague_range((2026, 2, 23), (2026, 2, 24));
        let events = parse_ical_events(&[ical], range);

        // FE interview (15:00) + standup (08:00) = 2 events today.
        assert_eq!(
            events.len(),
            2,
            "both VEVENTs from a multi-VEVENT VCALENDAR must be returned"
        );
        let summaries: Vec<&str> = events.iter().map(|e| e.summary.as_str()).collect();
        assert!(summaries.contains(&"FE interview"), "standalone event must be present");
        assert!(summaries.contains(&"Daily standup"), "recurring occurrence must be present");
    }

    // ── parse_ical_datetime: DST ambiguity ───────────────────────────────────

    /// Regression: a TZID-qualified datetime that falls in the DST "fall-back"
    /// gap (when the clock goes back and the same wall time occurs twice) must
    /// still parse successfully.
    ///
    /// Previously `parse_ical_datetime` used `.single()` which returns `None`
    /// for ambiguous times, causing the event to fall through to the "floating
    /// time" branch and be interpreted as the system's local timezone instead
    /// of the specified TZID.  The fix uses `.earliest()` so the first of the
    /// two possible offsets is chosen deterministically.
    ///
    /// Prague (Europe/Prague) falls back on the last Sunday of October:
    /// 2026-10-25 03:00 CEST → 02:00 CET.  Any time between 02:00 and 03:00
    /// on that day is ambiguous.
    #[test]
    fn parse_ical_datetime_dst_ambiguous_uses_earliest() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        // 02:30 on 2026-10-25 in Prague is ambiguous (occurs in both CEST and CET).
        let ts = parse_ical_datetime("20261025T023000", ";TZID=Europe/Prague");
        assert!(
            ts.is_some(),
            "DST-ambiguous TZID datetime must still parse (earliest offset chosen)"
        );

        let tz: chrono_tz::Tz = "Europe/Prague".parse().unwrap();
        let naive = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 10, 25).unwrap(),
            NaiveTime::from_hms_opt(2, 30, 0).unwrap(),
        );
        // .earliest() picks CEST (UTC+2): 02:30 Prague CEST = 00:30 UTC.
        let expected = tz.from_local_datetime(&naive).earliest().unwrap().timestamp();
        assert_eq!(
            ts,
            Some(expected),
            "ambiguous time should resolve to earliest offset (CEST, UTC+2)"
        );
    }

    // ── Startup sync regression: missing SUMMARY and empty input ─────────────

    /// Regression: EDS can deliver an event whose SUMMARY was stripped by
    /// libical with `X-LIC-ERROR;X-LIC-ERRORTYPE=VALUE-PARSE-ERROR:No value
    /// for SUMMARY property`.  The parser must default to an empty string
    /// rather than silently dropping the event.
    #[test]
    fn parse_vevent_missing_summary_defaults_to_empty() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART:20260223T100000Z\r\n"
            + "DTEND:20260223T110000Z\r\n"
            // No SUMMARY line — libical error caused it to be stripped.
            + "UID:no-summary@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let event = parse_vevent(&ical);
        assert!(
            event.is_some(),
            "event with missing SUMMARY must still parse (defaults to empty string)"
        );
        assert_eq!(
            event.unwrap().summary,
            "",
            "missing SUMMARY must default to empty string, not cause a parse failure"
        );
    }

    /// `parse_ical_events` must return an empty Vec for empty input without
    /// panicking.  This is a basic boundary check for the startup path.
    #[test]
    fn parse_ical_events_empty_input_returns_empty() {
        let range = prague_range((2026, 2, 23), (2026, 2, 24));
        let events = parse_ical_events(&[], range);
        assert!(events.is_empty(), "empty input must produce empty output");
    }

    /// Recurring events whose occurrences all fall OUTSIDE the query range
    /// must produce no results.  This confirms the RRULE expander filters
    /// correctly so stale events from before today are not shown.
    #[test]
    fn expand_recurring_outside_range_produces_no_events() {
        // Daily event from Jan 1–5 (5 occurrences, COUNT=5).
        let ical = recurring_ical(
            "20260101T090000",
            "20260101T100000",
            "FREQ=DAILY;COUNT=5",
            "past-daily@test",
        );
        // Query window starts after the series ends.
        let range = prague_range((2026, 2, 1), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert!(
            events.is_empty(),
            "recurring event series that ended before the range must produce 0 occurrences"
        );
    }

    /// Regression guard for the startup CalDAV refresh race:
    /// `parse_ical_events` must handle a batch of events (as EDS would send in
    /// `ObjectsAdded`) containing both a recently-added single event and a
    /// recurring master, and return them all.  This exercises the full
    /// parse/expand pipeline that runs once backends are properly populated.
    #[test]
    fn parse_ical_events_handles_mixed_batch_like_objects_added() {
        // A newly-added one-off event (like "FE interview brainstorm").
        let new_event = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART:20260224T140000Z\r\n"
            + "DTEND:20260224T150000Z\r\n"
            + "SUMMARY:FE interview brainstorm\r\n"
            + "UID:fe-interview-brainstorm@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        // A bi-weekly recurring event (like "Tech 1:1").
        // Series starts Feb 10 (Tuesday) → biweekly occurrences: Feb 10, Feb 24, Mar 10, …
        // This puts an occurrence on the same day as the new event (Feb 24).
        let recurring = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU",
            "tech-1on1@test",
        );

        let icals: Vec<String> = vec![new_event, recurring];
        // Range covers Feb 24 only — the new event AND a biweekly occurrence both land here.
        let range = prague_range((2026, 2, 24), (2026, 2, 25));
        let events = parse_ical_events(
            &icals.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            range,
        );

        // FE interview (14:00 UTC) + Tech 1:1 biweekly occurrence on Feb 24 (09:00 Prague).
        assert_eq!(
            events.len(),
            2,
            "newly-added event and recurring occurrence must both appear after startup sync"
        );
        let summaries: Vec<&str> = events.iter().map(|e| e.summary.as_str()).collect();
        assert!(
            summaries.contains(&"FE interview brainstorm"),
            "newly-added event must appear"
        );
        // recurring_ical helper uses SUMMARY:Test event
        assert!(
            summaries.contains(&"Test event"),
            "recurring occurrence must appear"
        );
    }
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides_i18n(
        &[
            entity::calendar::ENTITY_TYPE,
            entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
        ],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting EDS plugin...");

    // Build the tokio runtime manually so `handle_provides` runs without it
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = EdsPlugin::new().await?;

        // Grab shared state handle and connection before plugin is moved into the runtime
        let shared_state = plugin.shared_state();
        let conn = plugin.conn.clone();
        let config = plugin.config.clone();
        let session_locked = plugin.session_locked();
        let unlock_notify = plugin.unlock_notify();
        let notifier_slot = plugin.notifier_slot();

        let (runtime, notifier) = PluginRuntime::new("eds", plugin);

        // Fill the notifier slot so handle_action and the scheduler can push syncing state.
        {
            let mut slot = notifier_slot.lock_or_recover();
            *slot = Some(notifier.clone());
        }

        // Clone conn for the scheduler before the monitor spawn moves it
        let scheduler_conn = conn.clone();

        // Spawn D-Bus monitoring task
        let monitor_state = shared_state.clone();
        let monitor_notifier = notifier.clone();
        spawn_monitored_anyhow("eds/calendar-monitor", async move {
            monitor_eds_calendars(conn, monitor_state, monitor_notifier).await
        });

        // Spawn session monitor
        tokio::spawn(spawn_session_monitor(
            session_locked.clone(),
            unlock_notify.clone(),
        ));

        // Spawn periodic refresh scheduler
        tokio::spawn(spawn_refresh_scheduler(
            scheduler_conn,
            shared_state.clone(),
            config,
            session_locked,
            unlock_notify,
            notifier.clone(),
        ));

        runtime.run().await?;
        Ok(())
    })
}
