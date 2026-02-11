//! EDS (Evolution Data Server) D-Bus integration for daemon.
//!
//! This module discovers calendar sources, opens calendars, creates views,
//! and listens for event signals via the session bus.
//!
//! Unlike the cdylib version, this uses direct tokio spawns instead of spawn_on_tokio.

use anyhow::{Context, Result};
use log::{debug, warn};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::broadcast;

use zvariant::OwnedValue;

use waft_core::dbus::DbusHandle;
use waft_plugin_agenda::values::{AgendaEvent, CalendarSource, parse_vevent};

/// Type alias for D-Bus ObjectManager's GetManagedObjects() return value.
type ManagedObjects = HashMap<zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

const SOURCES_DEST: &str = "org.gnome.evolution.dataserver.Sources5";
const SOURCES_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
const CALENDAR_FACTORY_DEST: &str = "org.gnome.evolution.dataserver.Calendar8";
const CALENDAR_FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
pub const CALENDAR_VIEW_IFACE: &str = "org.gnome.evolution.dataserver.CalendarView";

/// Discover calendar sources from EDS source registry.
pub async fn discover_calendar_sources(dbus: &Arc<DbusHandle>) -> Result<Vec<CalendarSource>> {
    let conn = dbus.connection();

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

            if let (Some(uid), Some(data)) = (uid, data) {
                if data.contains("[Calendar]") {
                    let display_name = extract_display_name(&data).unwrap_or_else(|| uid.clone());
                    sources.push(CalendarSource { uid, display_name });
                }
            }
        }
    }

    debug!(
        "[agenda/dbus] Discovered {} calendar sources",
        sources.len()
    );
    Ok(sources)
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
pub async fn open_calendar(
    dbus: &Arc<DbusHandle>,
    source_uid: &str,
) -> Result<(String, String)> {
    let conn = dbus.connection();

    let proxy = zbus::Proxy::new(
        &conn,
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
        "[agenda/dbus] Opened calendar '{}': path={}, bus={}",
        source_uid, object_path, bus_name
    );
    Ok((object_path, bus_name))
}

/// Create a calendar view with the given query.
pub async fn create_view(
    dbus: &Arc<DbusHandle>,
    bus_name: &str,
    calendar_path: &str,
    query: &str,
) -> Result<String> {
    let conn = dbus.connection();

    let proxy = zbus::Proxy::new(
        &conn,
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
    debug!("[agenda/dbus] Created view: {}", view_path);
    Ok(view_path)
}

/// Start a calendar view (begins delivering signals).
pub async fn start_view(dbus: &Arc<DbusHandle>, bus_name: &str, view_path: &str) -> Result<()> {
    let conn = dbus.connection();

    let proxy = zbus::Proxy::new(&conn, bus_name, view_path, CALENDAR_VIEW_IFACE)
        .await
        .context("Failed to create CalendarView proxy")?;

    let _: () = proxy
        .call("Start", &())
        .await
        .context("Failed to start calendar view")?;

    debug!("[agenda/dbus] Started view: {}", view_path);
    Ok(())
}

/// Stop and dispose a calendar view.
pub async fn stop_and_dispose_view(
    dbus: &Arc<DbusHandle>,
    bus_name: &str,
    view_path: &str,
) -> Result<()> {
    let conn = dbus.connection();

    let proxy = zbus::Proxy::new(&conn, bus_name, view_path, CALENDAR_VIEW_IFACE)
        .await
        .context("Failed to create CalendarView proxy for cleanup")?;

    // Best-effort stop + dispose
    let _stop: std::result::Result<(), _> = proxy.call("Stop", &()).await;
    let _dispose: std::result::Result<(), _> = proxy.call("Dispose", &()).await;

    debug!("[agenda/dbus] Stopped and disposed view: {}", view_path);
    Ok(())
}

/// Message type sent from D-Bus signal listener to the daemon.
#[derive(Clone, Debug)]
pub enum ViewSignal {
    Added(Vec<AgendaEvent>),
    Modified(Vec<AgendaEvent>),
    Removed(Vec<String>),
}

/// Listen for CalendarView signals and forward parsed events via a flume channel.
pub async fn listen_view_signals(
    dbus: &Arc<DbusHandle>,
    tx: flume::Sender<ViewSignal>,
    view_paths: Arc<StdMutex<HashSet<String>>>,
) -> Result<()> {
    let rule = format!("type='signal',interface='{}'", CALENDAR_VIEW_IFACE);

    let mut rx = dbus.listen_signals(&rule).await?;

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
                    "ObjectsAdded" => {
                        if let Ok((icals,)) = msg.body().deserialize::<(Vec<String>,)>() {
                            let events: Vec<AgendaEvent> = icals
                                .iter()
                                .filter_map(|ical| parse_vevent(ical))
                                .collect();
                            if !events.is_empty() {
                                debug!("[agenda/dbus] ObjectsAdded: {} events", events.len());
                                if tx.send(ViewSignal::Added(events)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    "ObjectsModified" => {
                        if let Ok((icals,)) = msg.body().deserialize::<(Vec<String>,)>() {
                            let events: Vec<AgendaEvent> = icals
                                .iter()
                                .filter_map(|ical| parse_vevent(ical))
                                .collect();
                            if !events.is_empty() {
                                debug!("[agenda/dbus] ObjectsModified: {} events", events.len());
                                if tx.send(ViewSignal::Modified(events)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    "ObjectsRemoved" => {
                        if let Ok((uids,)) = msg.body().deserialize::<(Vec<String>,)>() {
                            if !uids.is_empty() {
                                debug!("[agenda/dbus] ObjectsRemoved: {} events", uids.len());
                                if tx.send(ViewSignal::Removed(uids)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                debug!("[agenda/dbus] signal receiver lagged by {n}, continuing");
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
    debug!("[agenda/dbus] listener stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_display_name_basic() {
        let data = "[Data Source]\nDisplayName=Personal\nEnabled=true\n[Calendar]\nBackendName=local";
        assert_eq!(extract_display_name(data), Some("Personal".to_string()));
    }

    #[test]
    fn extract_display_name_with_spaces() {
        let data = "[Data Source]\nDisplayName=My Work Calendar\nEnabled=true";
        assert_eq!(
            extract_display_name(data),
            Some("My Work Calendar".to_string())
        );
    }

    #[test]
    fn extract_display_name_missing() {
        let data = "[Data Source]\nEnabled=true\n[Calendar]\nBackendName=local";
        assert_eq!(extract_display_name(data), None);
    }

    #[test]
    fn extract_display_name_empty_value() {
        let data = "DisplayName=\nEnabled=true";
        assert_eq!(extract_display_name(data), Some("".to_string()));
    }

    #[test]
    fn extract_display_name_with_leading_whitespace() {
        let data = "  DisplayName=Trimmed\nEnabled=true";
        assert_eq!(extract_display_name(data), Some("Trimmed".to_string()));
    }

    #[test]
    fn extract_display_name_empty_data() {
        assert_eq!(extract_display_name(""), None);
    }

    #[test]
    fn extract_display_name_takes_first_match() {
        let data = "DisplayName=First\n[Section]\nDisplayName=Second";
        assert_eq!(extract_display_name(data), Some("First".to_string()));
    }

    #[test]
    fn extract_display_name_unicode() {
        let data = "DisplayName=Kalendář\nEnabled=true";
        assert_eq!(extract_display_name(data), Some("Kalendář".to_string()));
    }
}
