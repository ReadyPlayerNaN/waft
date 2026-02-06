//! EDS (Evolution Data Server) D-Bus integration.
//!
//! Discovers calendar sources, opens calendars, creates views,
//! and listens for event signals via the session bus.

use anyhow::{Context, Result};
use log::{debug, warn};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use zvariant::OwnedValue;

use waft_core::dbus::DbusHandle;

use super::values::{AgendaEvent, CalendarSource, parse_vevent};

/// Bridge tokio and glib runtimes using flume channel.
async fn spawn_on_tokio<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = flume::bounded(1);
    tokio::spawn(async move {
        let result = future.await;
        let _ = tx.send(result);
    });
    rx.recv_async().await.expect("tokio task cancelled")
}

/// Type alias for D-Bus ObjectManager's GetManagedObjects() return value.
type ManagedObjects = HashMap<zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

const SOURCES_DEST: &str = "org.gnome.evolution.dataserver.Sources5";
const SOURCES_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
const CALENDAR_FACTORY_DEST: &str = "org.gnome.evolution.dataserver.Calendar8";
const CALENDAR_FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
pub const CALENDAR_VIEW_IFACE: &str = "org.gnome.evolution.dataserver.CalendarView";

/// Discover calendar sources from EDS source registry.
///
/// Calls `GetManagedObjects()` and filters for sources with a `[Calendar]` group.
pub async fn discover_calendar_sources(dbus: &Arc<DbusHandle>) -> Result<Vec<CalendarSource>> {
    let conn = dbus.connection();
    let result: Vec<CalendarSource> = spawn_on_tokio(async move {
        let proxy = zbus::Proxy::new(
            &conn,
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
            // Look for the exact Source interface (not Source.Writable, Source.OAuth2Support, etc.)
            let source_iface = interfaces.get("org.gnome.evolution.dataserver.Source");

            if let Some(props) = source_iface {
                // Get the UID
                let uid = props.get("UID").and_then(|v| {
                    let val: zvariant::Value = v.clone().into();
                    if let zvariant::Value::Str(s) = val {
                        Some(s.to_string())
                    } else {
                        None
                    }
                });

                // Get the Data (key file content)
                let data = props.get("Data").and_then(|v| {
                    let val: zvariant::Value = v.clone().into();
                    if let zvariant::Value::Str(s) = val {
                        Some(s.to_string())
                    } else {
                        None
                    }
                });

                if let (Some(uid), Some(data)) = (uid, data) {
                    // Only include calendar sources
                    if data.contains("[Calendar]") {
                        let display_name =
                            extract_display_name(&data).unwrap_or_else(|| uid.clone());
                        sources.push(CalendarSource { uid, display_name });
                    }
                }
            }
        }

        debug!(
            "[agenda/dbus] Discovered {} calendar sources",
            sources.len()
        );
        Ok::<_, anyhow::Error>(sources)
    })
    .await?;

    Ok(result)
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

/// Open a calendar backend via the CalendarFactory.
///
/// Returns `(object_path, bus_name)` for the opened calendar.
pub async fn open_calendar(dbus: &Arc<DbusHandle>, source_uid: &str) -> Result<(String, String)> {
    let conn = dbus.connection();
    let uid = source_uid.to_string();

    let result: (String, String) = spawn_on_tokio(async move {
        let proxy = zbus::Proxy::new(
            &conn,
            CALENDAR_FACTORY_DEST,
            CALENDAR_FACTORY_PATH,
            "org.gnome.evolution.dataserver.CalendarFactory",
        )
        .await
        .context("Failed to create CalendarFactory proxy")?;

        let (object_path, bus_name): (String, String) = proxy
            .call("OpenCalendar", &(&uid,))
            .await
            .context("Failed to open calendar")?;

        debug!(
            "[agenda/dbus] Opened calendar '{}': path={}, bus={}",
            uid, object_path, bus_name
        );
        Ok::<_, anyhow::Error>((object_path, bus_name))
    })
    .await?;

    Ok(result)
}

/// Create a calendar view with the given query.
///
/// Returns the view object path.
pub async fn create_view(
    dbus: &Arc<DbusHandle>,
    bus_name: &str,
    calendar_path: &str,
    query: &str,
) -> Result<String> {
    let conn = dbus.connection();
    let bus = bus_name.to_string();
    let path = calendar_path.to_string();
    let q = query.to_string();

    let view_path: String = spawn_on_tokio(async move {
        let proxy = zbus::Proxy::new(
            &conn,
            bus.as_str(),
            path.as_str(),
            "org.gnome.evolution.dataserver.Calendar",
        )
        .await
        .context("Failed to create Calendar proxy")?;

        let view_path: zvariant::OwnedObjectPath = proxy
            .call("GetView", &(&q,))
            .await
            .context("Failed to create calendar view")?;

        let view_path = view_path.to_string();
        debug!("[agenda/dbus] Created view: {}", view_path);
        Ok::<_, anyhow::Error>(view_path)
    })
    .await?;

    Ok(view_path)
}

/// Start a calendar view (begins delivering signals).
pub async fn start_view(dbus: &Arc<DbusHandle>, bus_name: &str, view_path: &str) -> Result<()> {
    let conn = dbus.connection();
    let bus = bus_name.to_string();
    let path = view_path.to_string();

    spawn_on_tokio(async move {
        let proxy = zbus::Proxy::new(&conn, bus.as_str(), path.as_str(), CALENDAR_VIEW_IFACE)
            .await
            .context("Failed to create CalendarView proxy")?;

        let _: () = proxy
            .call("Start", &())
            .await
            .context("Failed to start calendar view")?;

        debug!("[agenda/dbus] Started view: {}", path);
        Ok::<_, anyhow::Error>(())
    })
    .await?;

    Ok(())
}

/// Stop and dispose a calendar view.
pub async fn stop_and_dispose_view(
    dbus: &Arc<DbusHandle>,
    bus_name: &str,
    view_path: &str,
) -> Result<()> {
    let conn = dbus.connection();
    let bus = bus_name.to_string();
    let path = view_path.to_string();

    spawn_on_tokio(async move {
        let proxy = zbus::Proxy::new(&conn, bus.as_str(), path.as_str(), CALENDAR_VIEW_IFACE)
            .await
            .context("Failed to create CalendarView proxy for cleanup")?;

        // Best-effort stop + dispose
        let _stop: std::result::Result<(), _> = proxy.call("Stop", &()).await;
        let _dispose: std::result::Result<(), _> = proxy.call("Dispose", &()).await;

        debug!("[agenda/dbus] Stopped and disposed view: {}", path);
        Ok::<_, anyhow::Error>(())
    })
    .await?;

    Ok(())
}

/// Message type sent from D-Bus signal listener to the main thread.
#[derive(Clone, Debug)]
pub enum ViewSignal {
    Added(Vec<AgendaEvent>),
    Modified(Vec<AgendaEvent>),
    Removed(Vec<String>),
}

/// Listen for CalendarView signals and forward parsed events via a flume channel.
///
/// Listens for `ObjectsAdded`, `ObjectsModified`, and `ObjectsRemoved` signals
/// on the CalendarView interface. iCal strings are parsed into `AgendaEvent`s.
pub async fn listen_view_signals(
    dbus: &Arc<DbusHandle>,
    tx: flume::Sender<ViewSignal>,
    view_paths: Arc<Mutex<HashSet<String>>>,
) -> Result<()> {
    let rule = format!("type='signal',interface='{}'", CALENDAR_VIEW_IFACE);

    let mut rx = dbus.listen_signals(&rule).await?;

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    // Only process signals from views we created
                    let msg_path = msg
                        .header()
                        .path()
                        .map(|p| p.to_string())
                        .unwrap_or_default();
                    {
                        let paths = match view_paths.lock() {
                            Ok(paths) => paths,
                            Err(e) => {
                                warn!("[agenda/dbus] view_paths mutex poisoned, recovering: {e}");
                                e.into_inner()
                            }
                        };
                        if !paths.is_empty() && !paths.contains(&msg_path) {
                            continue;
                        }
                    }

                    let member = msg
                        .header()
                        .member()
                        .map(|m| m.to_string())
                        .unwrap_or_default();

                    match member.as_str() {
                        "ObjectsAdded" | "ObjectsModified" => {
                            match msg.body().deserialize::<(Vec<String>,)>() {
                                Ok((ical_strings,)) => {
                                    debug!(
                                        "[agenda/dbus] {} signal: {} iCal string(s)",
                                        member,
                                        ical_strings.len()
                                    );
                                    let events: Vec<AgendaEvent> = ical_strings
                                        .iter()
                                        .filter_map(|s| {
                                            let result = parse_vevent(s);
                                            if result.is_none() {
                                                warn!(
                                                    "[agenda/dbus] Failed to parse VEVENT: {}",
                                                    &s[..s.len().min(200)]
                                                );
                                            }
                                            result
                                        })
                                        .collect();

                                    debug!(
                                        "[agenda/dbus] Parsed {} event(s): {:?}",
                                        events.len(),
                                        events.iter().map(|e| &e.summary).collect::<Vec<_>>()
                                    );

                                    if !events.is_empty() {
                                        let signal = if member == "ObjectsAdded" {
                                            ViewSignal::Added(events)
                                        } else {
                                            ViewSignal::Modified(events)
                                        };
                                        if tx.send(signal).is_err() {
                                            warn!(
                                                "[agenda/dbus] receiver dropped, stopping signal listener"
                                            );
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "[agenda/dbus] Failed to deserialize {} body: {}",
                                        member, e
                                    );
                                }
                            }
                        }
                        "ObjectsRemoved" => {
                            if let Ok((uids,)) = msg.body().deserialize::<(Vec<String>,)>() {
                                debug!("[agenda/dbus] ObjectsRemoved: {} uid(s)", uids.len());
                                if !uids.is_empty()
                                    && tx.send(ViewSignal::Removed(uids)).is_err() {
                                        warn!(
                                            "[agenda/dbus] receiver dropped, stopping signal listener"
                                        );
                                        break;
                                    }
                            }
                        }
                        _ => {}
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("[agenda/dbus] Lagged {} messages", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
        debug!("[agenda/dbus] view signal listener stopped");
    });

    Ok(())
}

// Tests removed for plugin version
