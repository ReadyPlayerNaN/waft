//! EDS (Evolution Data Server) plugin — calendar event integration.
//!
//! Provides calendar events from EDS via the entity-based protocol.
//! Events are exposed as `calendar-event` entities with URN format:
//! `eds/calendar-event/{uid}@{start_time}`
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "eds"
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::stream::StreamExt;
use log::{debug, warn};
use serde::Deserialize;
use waft_plugin::*;
use zbus::{Connection, MessageStream};
use zvariant::OwnedValue;

/// EDS configuration from config file.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct EdsConfig {}

/// Shared daemon state containing calendar events.
struct EdsState {
    /// Map of occurrence keys to calendar events.
    /// Key format: "{uid}@{start_time}"
    events: HashMap<String, entity::calendar::CalendarEvent>,
}

impl EdsState {
    fn new() -> Self {
        Self {
            events: HashMap::new(),
        }
    }
}

/// EDS plugin implementation.
struct EdsPlugin {
    #[allow(dead_code)]
    config: EdsConfig,
    state: Arc<StdMutex<EdsState>>,
    #[allow(dead_code)]
    conn: Connection,
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
        })
    }

    fn shared_state(&self) -> Arc<StdMutex<EdsState>> {
        self.state.clone()
    }

    /// Create occurrence key from UID and start time.
    fn make_occurrence_key(uid: &str, start_time: i64) -> String {
        format!("{}@{}", uid, start_time)
    }
}

#[async_trait::async_trait]
impl Plugin for EdsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("Mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };

        state
            .events
            .iter()
            .map(|(key, event)| {
                let urn = Urn::new("eds", entity::calendar::ENTITY_TYPE, key);
                Entity::new(urn, entity::calendar::ENTITY_TYPE, event)
            })
            .collect()
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::debug!("Received action '{}' for URN: {}", action, urn);
        log::warn!("EDS plugin is read-only; action '{}' not supported", action);
        Ok(())
    }

    fn can_stop(&self) -> bool {
        true
    }
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
    #[allow(dead_code)]
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

    debug!("[eds] Discovered {} calendar sources", sources.len());
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

/// Monitor EDS calendars and populate shared state.
///
/// Discovers calendar sources, opens calendars, creates views,
/// and spawns monitoring tasks for each view.
async fn monitor_eds_calendars(
    conn: Connection,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    // Discover calendar sources
    let sources = match discover_calendar_sources(&conn).await {
        Ok(sources) => sources,
        Err(e) => {
            warn!("[eds] Failed to discover calendar sources: {}", e);
            return Err(e);
        }
    };

    if sources.is_empty() {
        debug!("[eds] No calendar sources found");
        return Ok(());
    }

    // Track view paths for signal filtering
    let view_paths = Arc::new(StdMutex::new(HashSet::new()));

    // Query for upcoming events (next 30 days)
    let now = chrono::Utc::now();
    let end = now + chrono::Duration::days(30);
    let query = format!(
        "(occur-in-time-range? (make-time \"{}\") (make-time \"{}\"))",
        now.format("%Y%m%dT%H%M%SZ"),
        end.format("%Y%m%dT%H%M%SZ")
    );

    // Open calendars and create views
    for source in &sources {
        let conn_clone = conn.clone();
        let state_clone = state.clone();
        let notifier_clone = notifier.clone();
        let source_uid = source.uid.clone();
        let query_clone = query.clone();
        let view_paths_clone = view_paths.clone();

        tokio::spawn(async move {
            match open_calendar(&conn_clone, &source_uid).await {
                Ok((calendar_path, bus_name)) => {
                    match create_view(&conn_clone, &bus_name, &calendar_path, &query_clone).await {
                        Ok(view_path) => {
                            // Track this view path
                            {
                                let mut paths = match view_paths_clone.lock() {
                                    Ok(p) => p,
                                    Err(e) => {
                                        warn!("[eds] view_paths mutex poisoned, recovering: {e}");
                                        e.into_inner()
                                    }
                                };
                                paths.insert(view_path.clone());
                            }

                            // Start the view
                            if let Err(e) = start_view(&conn_clone, &bus_name, &view_path).await {
                                warn!("[eds] Failed to start view: {}", e);
                                return;
                            }

                            // Spawn view monitor
                            if let Err(e) = spawn_view_monitor(
                                conn_clone,
                                bus_name,
                                view_path,
                                state_clone,
                                notifier_clone,
                                view_paths_clone,
                            )
                            .await
                            {
                                warn!("[eds] View monitor error: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("[eds] Failed to create view for {}: {}", source_uid, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("[eds] Failed to open calendar {}: {}", source_uid, e);
                }
            }
        });
    }

    Ok(())
}

/// Spawn a monitor task for a calendar view.
///
/// Listens for ObjectsAdded, ObjectsModified, ObjectsRemoved signals,
/// parses iCalendar VEVENT data, and updates shared state.
async fn spawn_view_monitor(
    conn: Connection,
    _bus_name: String,
    view_path: String,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
    view_paths: Arc<StdMutex<HashSet<String>>>,
) -> Result<()> {
    // Get message stream
    let mut stream = MessageStream::from(&conn);

    // Register match rules so the bus forwards signals to us
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

    tokio::spawn(async move {
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
                        let events = parse_ical_events(&icals);
                        if !events.is_empty() {
                            debug!("[eds] ObjectsAdded: {} events", events.len());
                            update_state_add_events(&state, events);
                            notifier.notify();
                        }
                    } else {
                        warn!("[eds] Failed to deserialize ObjectsAdded signal body");
                    }
                }
                "ObjectsModified" => {
                    let body = msg.body();
                    if let Ok((icals,)) = body.deserialize::<(Vec<String>,)>() {
                        let events = parse_ical_events(&icals);
                        if !events.is_empty() {
                            debug!("[eds] ObjectsModified: {} events", events.len());
                            // For modified events, remove old occurrences then add new
                            let uids: Vec<String> = events.iter().map(|e| e.uid.clone()).collect();
                            update_state_remove_events(&state, &uids);
                            update_state_add_events(&state, events);
                            notifier.notify();
                        }
                    } else {
                        warn!("[eds] Failed to deserialize ObjectsModified signal body");
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
            let mut paths = match view_paths.lock() {
                Ok(p) => p,
                Err(e) => {
                    warn!("[eds] view_paths mutex poisoned during cleanup, recovering: {e}");
                    e.into_inner()
                }
            };
            paths.remove(&view_path);
        }

        debug!("[eds] View monitor stopped: {}", view_path);
    });

    Ok(())
}

/// Parse a list of iCalendar strings into CalendarEvent entities.
fn parse_ical_events(icals: &[String]) -> Vec<entity::calendar::CalendarEvent> {
    icals.iter().filter_map(|ical| parse_vevent(ical)).collect()
}

/// Parse a single iCalendar VEVENT string into a CalendarEvent.
///
/// This is a simplified parser that extracts the essential fields.
/// For a complete implementation, consider using the icalendar crate.
fn parse_vevent(ical_str: &str) -> Option<entity::calendar::CalendarEvent> {
    // Unfold continuation lines
    let unfolded = unfold_ical(ical_str);

    let mut in_vevent = false;
    let mut nest_depth: u32 = 0;
    let mut uid = None;
    let mut summary = None;
    let mut dtstart: Option<i64> = None;
    let mut dtend: Option<i64> = None;
    let mut all_day = false;
    let mut description = None;
    let mut location = None;
    let mut attendees: Vec<entity::calendar::CalendarEventAttendee> = Vec::new();

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
            dtstart = parse_ical_datetime(&value, &params);
        } else if line.starts_with("DTEND") {
            let (params, value) = split_ical_property(line, "DTEND");
            dtend = parse_ical_datetime(&value, &params);
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
    let start_time = dtstart?;
    let end_time = dtend.unwrap_or(start_time + 3600);

    Some(entity::calendar::CalendarEvent {
        uid,
        summary,
        start_time,
        end_time,
        all_day,
        description,
        location,
        attendees,
    })
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
                && let Some(dt) = tz.from_local_datetime(&datetime).single()
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
    let mut st = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[eds] Mutex poisoned during add_events, recovering: {e}");
            e.into_inner()
        }
    };

    for event in events {
        let key = EdsPlugin::make_occurrence_key(&event.uid, event.start_time);
        st.events.insert(key, event);
    }
}

/// Update state by removing events matching base UIDs.
fn update_state_remove_events(state: &Arc<StdMutex<EdsState>>, uids: &[String]) {
    let mut st = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[eds] Mutex poisoned during remove_events, recovering: {e}");
            e.into_inner()
        }
    };

    // Remove all occurrence keys matching any of the base UIDs
    st.events.retain(|key, _| {
        let base_uid = key.split('@').next().unwrap_or("");
        !uids.contains(&base_uid.to_string())
    });
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[entity::calendar::ENTITY_TYPE]) {
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

        let (runtime, notifier) = PluginRuntime::new("eds", plugin);

        // Spawn D-Bus monitoring task
        let monitor_state = shared_state.clone();
        let monitor_notifier = notifier.clone();
        tokio::spawn(async move {
            if let Err(e) = monitor_eds_calendars(conn, monitor_state, monitor_notifier).await {
                log::error!("[eds] Failed to start calendar monitoring: {}", e);
            }
            log::debug!("[eds] Calendar monitoring task stopped");
        });

        runtime.run().await?;
        Ok(())
    })
}
