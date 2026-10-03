//! Source-local authoritative view replacement. Live deltas invalidate, not delete by UID.
use crate::{
    EdsPlugin, details, ical,
    lifecycle::{bounded, resolve_owner, unknown_method},
    source_registry::{self, CalendarSource},
    unix_now,
};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use waft_plugin::StateLocker;
use waft_protocol::entity::calendar::{CalendarEvent, CalendarSourceState};
use zbus::{Connection, MatchRule, MessageStream, Proxy};
const CALENDAR: &str = "org.gnome.evolution.dataserver.Calendar";
const VIEW: &str = "org.gnome.evolution.dataserver.CalendarView";

#[derive(Clone)]
pub(crate) struct Backend {
    pub conn: Connection,
    pub owner: String,
    pub path: String,
    pub capabilities: Option<Vec<String>>,
    _lease: Arc<BackendLease>,
    _observer: Arc<Observer>,
}
pub(crate) struct BackendLease {
    conn: Connection,
    owner: String,
    path: String,
}
impl Drop for BackendLease {
    fn drop(&mut self) {
        let conn = self.conn.clone();
        let owner = self.owner.clone();
        let path = self.path.clone();
        spawn_cleanup(async move {
            let close = async {
                Proxy::new(&conn, owner.as_str(), path.as_str(), CALENDAR)
                    .await?
                    .call::<_, _, ()>("Close", &())
                    .await
            };
            if !matches!(
                tokio::time::timeout(Duration::from_millis(500), close).await,
                Ok(Ok(()))
            ) {
                log::debug!("[eds] backend cleanup unavailable; local lease released");
            }
        });
    }
}
struct View {
    backend: Backend,
    path: String,
    reader: Option<tokio::task::JoinHandle<()>>,
    events: Arc<Mutex<HashMap<String, CalendarEvent>>>,
    complete: Arc<AtomicU8>,
    completed: Arc<Notify>,
    disposed: bool,
    parse_failed: Arc<AtomicBool>,
}
impl View {
    async fn retire(mut self) {
        if let Some(reader) = self.reader.take() {
            reader.abort();
            if reader.await.is_err_and(|e| !e.is_cancelled()) {
                log::warn!("[eds] view reader failed during retirement");
            }
        }
        self.disposed = true;
        dispose_view(&self.backend, &self.path).await;
    }
}
impl Drop for View {
    fn drop(&mut self) {
        if self.disposed {
            return;
        }
        let reader = self.reader.take();
        if let Some(reader) = &reader {
            reader.abort();
        }
        let backend = self.backend.clone();
        let path = self.path.clone();
        spawn_cleanup(async move {
            if let Some(reader) = reader
                && reader.await.is_err_and(|e| !e.is_cancelled())
            {
                log::warn!("[eds] cancelled view reader failed");
            }
            dispose_view(&backend, &path).await;
        });
    }
}
async fn dispose_view(backend: &Backend, path: &str) {
    let dispose = async {
        let proxy = Proxy::new(&backend.conn, backend.owner.as_str(), path, VIEW).await?;
        if proxy.call::<_, _, ()>("Stop", &()).await.is_err() {
            log::debug!("[eds] view Stop unavailable");
        }
        proxy.call::<_, _, ()>("Dispose", &()).await
    };
    if !matches!(
        tokio::time::timeout(Duration::from_millis(500), dispose).await,
        Ok(Ok(()))
    ) {
        log::debug!("[eds] view cleanup unavailable; local monitor released");
    }
}
struct Observer(Option<tokio::task::JoinHandle<()>>);
impl Drop for Observer {
    fn drop(&mut self) {
        if let Some(reader) = self.0.take() {
            reader.abort();
            spawn_cleanup(async move {
                if reader.await.is_err_and(|e| !e.is_cancelled()) {
                    log::warn!("[eds] health monitor failed during retirement");
                }
            });
        }
    }
}
fn spawn_cleanup(future: impl std::future::Future<Output = ()> + Send + 'static) {
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(future);
    } else {
        log::debug!("[eds] runtime ended; releasing local resources through connection teardown");
    }
}
impl EdsPlugin {
    pub(crate) async fn watch_source(
        self,
        conn: Connection,
        source_owner: String,
        source: CalendarSource,
        generation: u64,
        epoch: u64,
        wake: Arc<Notify>,
    ) {
        let health = self.clone();
        let health_source = source.clone();
        let health_conn = conn.clone();
        let health_wake = wake.clone();
        let _observer = Observer(Some(tokio::spawn(async move {
            health
                .source_health(
                    health_conn,
                    source_owner,
                    health_source,
                    generation,
                    epoch,
                    health_wake,
                )
                .await;
        })));
        let revisions = Arc::new(AtomicU64::new(0));
        let mut active: Option<View> = None;
        let mut backend: Option<Backend> = None;
        let mut retry = 1;
        while self.current(&source.uid, generation, epoch) {
            if active.is_none() {
                self.source_state(
                    &source.uid,
                    generation,
                    epoch,
                    CalendarSourceState::Pending,
                    None,
                );
            }
            let Ok(permit) = self.setup_permits.acquire().await else {
                log::warn!("[eds] setup scheduler closed");
                return;
            };
            if !self.current(&source.uid, generation, epoch) {
                return;
            }
            self.source_state(
                &source.uid,
                generation,
                epoch,
                CalendarSourceState::Opening,
                None,
            );
            let revision = revisions.load(Ordering::SeqCst);
            let setup = async {
                let current_backend = match &backend {
                    Some(backend) => backend.clone(),
                    None => self.open_backend(&conn, &source, generation, epoch).await?,
                };
                let view = create_view(
                    current_backend.clone(),
                    &source.uid,
                    revisions.clone(),
                    wake.clone(),
                    (&self, generation, epoch),
                )
                .await?;
                Ok::<_, anyhow::Error>((current_backend, view))
            };
            let candidate = tokio::time::timeout(Duration::from_secs(10), setup)
                .await
                .context("source setup timeout")
                .and_then(|r| r);
            let candidate = match candidate {
                Ok((opened, view)) => {
                    backend = Some(opened);
                    view
                }
                Err(error) => {
                    drop(permit);
                    let unsupported = unknown_method(&error);
                    self.source_state(
                        &source.uid,
                        generation,
                        epoch,
                        if unsupported {
                            CalendarSourceState::Unsupported
                        } else {
                            CalendarSourceState::RetryWait
                        },
                        Some(details(
                            if unsupported {
                                "eds.unsupported-api"
                            } else {
                                "eds.view-failed"
                            },
                            "Calendar view setup is unavailable",
                        )),
                    );
                    if unsupported {
                        wake.notified().await;
                    } else {
                        tokio::select! { _ = tokio::time::sleep(Duration::from_secs(retry)) => {}, _ = wake.notified() => {} }
                    }
                    retry = (retry * 2).min(30);
                    if backend.is_none() {
                        let mut st = self.state.lock_or_recover();
                        if let Some(entry) = st
                            .sources
                            .get_mut(&source.uid)
                            .filter(|s| s.generation == epoch)
                        {
                            entry.backend = None;
                        }
                    }
                    continue;
                }
            };
            self.source_state(
                &source.uid,
                generation,
                epoch,
                CalendarSourceState::Loading,
                None,
            );
            let completed = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let notified = candidate.completed.notified();
                    let state = candidate.complete.load(Ordering::SeqCst);
                    if state != 0 {
                        return state == 1;
                    }
                    notified.await;
                }
            })
            .await
            .unwrap_or(false);
            if !completed
                || revisions.load(Ordering::SeqCst) != revision
                || !self.current(&source.uid, generation, epoch)
            {
                candidate.retire().await;
                drop(permit);
                self.source_state(
                    &source.uid,
                    generation,
                    epoch,
                    CalendarSourceState::RetryWait,
                    Some(details(
                        "eds.view-failed",
                        "Calendar snapshot incomplete or invalidated",
                    )),
                );
                // A dirty candidate waits its backoff even if further live deltas arrive.
                // Owner/source replacement cancels this generation's task instead.
                tokio::time::sleep(Duration::from_secs(retry)).await;
                retry = (retry * 2).min(30);
                continue;
            }
            {
                let events = candidate.events.lock_or_recover().clone();
                let mut st = self.state.lock_or_recover();
                if st.generation == generation
                    && let Some(entry) = st
                        .sources
                        .get_mut(&source.uid)
                        .filter(|s| s.generation == epoch)
                {
                    entry.events = events;
                    entry.setup = false;
                    entry.status.state = CalendarSourceState::Watching;
                    entry.status.last_view_snapshot = Some(unix_now());
                    if entry
                        .status
                        .error
                        .as_ref()
                        .is_some_and(|e| e.code == "eds.view-failed")
                    {
                        entry.status.error = None;
                    }
                    if candidate.parse_failed.load(Ordering::SeqCst) && entry.status.error.is_none()
                    {
                        entry.status.error = Some(details("eds.view-failed", "An invalid calendar item was excluded").with_details(waft_plugin::serde_json::json!({"category":"invalid-calendar-item"})));
                    }
                    entry.backend = backend.clone();
                }
            }
            drop(permit);
            if let Some(retired) = active.replace(candidate) {
                retired.retire().await;
            }
            retry = 1;
            self.notifier.notify();
            self.refresh.notify_one();
            // Keep the candidate's snapshot revision across retirement. Its new
            // steady reader can invalidate while old-view Stop/Dispose awaits;
            // adopting a fresh baseline here would swallow that queued change.
            if revisions.load(Ordering::SeqCst) != revision {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
            let midnight = tokio::time::sleep(Duration::from_secs(ical::secs_until_eds_midnight()));
            tokio::pin!(midnight);
            loop {
                tokio::select! {
                    _ = &mut midnight => { break; }
                    _ = wake.notified() => {
                        if !self.current(&source.uid, generation, epoch) { return; }
                        if revisions.load(Ordering::SeqCst) != revision { tokio::time::sleep(Duration::from_millis(50)).await; break; }
                    }
                }
            }
        }
        drop(active);
    }
    fn current(&self, uid: &str, generation: u64, epoch: u64) -> bool {
        let st = self.state.lock_or_recover();
        st.generation == generation && st.sources.get(uid).is_some_and(|s| s.generation == epoch)
    }
    fn source_state(
        &self,
        uid: &str,
        generation: u64,
        epoch: u64,
        state: CalendarSourceState,
        error: Option<waft_protocol::error::ProtocolError>,
    ) {
        let mut st = self.state.lock_or_recover();
        if st.generation == generation
            && let Some(source) = st.sources.get_mut(uid).filter(|s| s.generation == epoch)
        {
            source.status.state = state;
            source.setup = matches!(
                state,
                CalendarSourceState::Opening | CalendarSourceState::Loading
            );
            if error.is_some() {
                source.status.error = error;
            }
        }
        drop(st);
        self.notifier.notify();
    }
    async fn open_backend(
        &self,
        conn: &Connection,
        source: &CalendarSource,
        generation: u64,
        epoch: u64,
    ) -> Result<Backend> {
        let factory_owner = resolve_owner(conn, source_registry::FACTORY_DEST).await?;
        let factory = Proxy::new(
            conn,
            factory_owner.as_str(),
            source_registry::FACTORY_PATH,
            "org.gnome.evolution.dataserver.CalendarFactory",
        )
        .await?;
        let (path, destination): (String, String) = bounded(async {
            Ok(factory
                .call("OpenCalendar", &(source.uid.as_str(),))
                .await?)
        })
        .await?;
        let owner = resolve_owner(conn, &destination).await?;
        let lease = {
            let mut st = self.state.lock_or_recover();
            st.backend_leases
                .retain(|_, lease| lease.strong_count() > 0);
            // Unique names can repeat after a bus restart; Close is also client-scoped.
            let key = (
                conn.server_guid().to_string(),
                conn.unique_name()
                    .map_or_else(String::new, ToString::to_string),
                owner.clone(),
                path.clone(),
            );
            if let Some(lease) = st
                .backend_leases
                .get(&key)
                .and_then(std::sync::Weak::upgrade)
            {
                lease
            } else {
                let lease = Arc::new(BackendLease {
                    conn: conn.clone(),
                    owner: owner.clone(),
                    path: path.clone(),
                });
                st.backend_leases.insert(key, Arc::downgrade(&lease));
                lease
            }
        };
        let observer_plugin = self.clone();
        let observer_conn = conn.clone();
        let observer_owner = owner.clone();
        let observer_path = path.clone();
        let observer = Arc::new(Observer(Some(tokio::spawn(async move {
            observer_plugin
                .backend_health(observer_conn, observer_owner, observer_path, generation)
                .await;
        }))));
        let mut backend = Backend {
            conn: conn.clone(),
            owner,
            path,
            capabilities: None,
            _lease: lease,
            _observer: observer,
        };
        // Track the owner even when Open/GetView fails so a backend replacement wakes recovery.
        {
            let mut st = self.state.lock_or_recover();
            if st.generation == generation
                && let Some(entry) = st
                    .sources
                    .get_mut(&source.uid)
                    .filter(|s| s.generation == epoch)
            {
                entry.backend_owner = Some(backend.owner.clone());
                entry.backend = Some(backend.clone());
            }
        }
        let proxy = Proxy::new(
            conn,
            backend.owner.as_str(),
            backend.path.as_str(),
            CALENDAR,
        )
        .await?;
        bounded(async {
            proxy.call_method("Open", &()).await?;
            Ok(())
        })
        .await?;
        backend.capabilities = proxy.get_property::<Vec<String>>("Capabilities").await.ok();
        let online = proxy.get_property::<bool>("Online").await.ok();
        {
            let mut st = self.state.lock_or_recover();
            if st.generation == generation
                && let Some(entry) = st
                    .sources
                    .get_mut(&source.uid)
                    .filter(|s| s.generation == epoch)
            {
                entry.status.online = online;
            }
        }
        Ok(backend)
    }
    async fn backend_health(&self, conn: Connection, owner: String, path: String, generation: u64) {
        let result = async {
            let rule = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(owner.as_str())?
                .path(path.as_str())?
                .build();
            let mut stream = MessageStream::for_match_rule(rule, &conn, Some(256)).await?;
            while let Some(result) = stream.next().await {
                let message = result?;
                let header = message.header();
                if !header.sender().is_some_and(|s| s.as_str() == owner) {
                    continue;
                }
                let mut online = None;
                let mut invalidated = false;
                let backend_error = header.interface().is_some_and(|i| i.as_str() == CALENDAR)
                    && header.member().is_some_and(|m| m.as_str() == "Error");
                if header
                    .member()
                    .is_some_and(|m| m.as_str() == "PropertiesChanged")
                    && let Ok((iface, values, missing)) = message.body().deserialize::<(
                        String,
                        HashMap<String, zvariant::OwnedValue>,
                        Vec<String>,
                    )>()
                {
                    if iface != CALENDAR {
                        continue;
                    }
                    online = values
                        .get("Online")
                        .and_then(|v| bool::try_from(v.clone()).ok());
                    invalidated = missing.iter().any(|p| p == "Online");
                }
                if invalidated {
                    online = bounded(async {
                        Ok(Proxy::new(&conn, owner.as_str(), path.as_str(), CALENDAR)
                            .await?
                            .get_property::<bool>("Online")
                            .await?)
                    })
                    .await
                    .ok();
                }
                let mut st = self.state.lock_or_recover();
                if st.generation != generation {
                    break;
                }
                for entry in st.sources.values_mut().filter(|s| {
                    s.backend
                        .as_ref()
                        .is_some_and(|b| b.owner == owner && b.path == path)
                }) {
                    if backend_error
                        && entry.status.error.as_ref().is_none_or(|e| {
                            matches!(e.code.as_str(), "eds.refresh-failed" | "eds.view-failed")
                        })
                    {
                        entry.status.error = Some(details(
                            "eds.refresh-failed",
                            "Calendar backend reported an error",
                        ));
                    }
                    if online.is_some() || invalidated {
                        let previous = entry.status.online;
                        entry.status.online = online;
                        if online == Some(false) && entry.status.error.is_none() {
                            entry.status.error =
                                Some(details("eds.offline", "Calendar backend is offline"));
                        }
                        if online == Some(true) && previous == Some(false) {
                            if entry
                                .status
                                .error
                                .as_ref()
                                .is_some_and(|e| e.code == "eds.offline")
                            {
                                entry.status.error = None;
                            }
                            if !entry.refresh_active {
                                entry.refresh_pending = true;
                            }
                            self.refresh.notify_one();
                        }
                    }
                }
                drop(st);
                self.notifier.notify();
            }
            Ok::<_, zbus::Error>(())
        }
        .await;
        if result.is_err() {
            log::debug!("[eds] backend health monitor disconnected");
        }
    }
    async fn source_health(
        &self,
        conn: Connection,
        owner: String,
        source: CalendarSource,
        generation: u64,
        epoch: u64,
        wake: Arc<Notify>,
    ) {
        let streams = bounded(async {
            let properties = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(owner.as_str())?
                .path(source.path.as_str())?
                .interface("org.freedesktop.DBus.Properties")?
                .member("PropertiesChanged")?
                .add_arg(source_registry::SOURCE_IFACE)?
                .build();
            let attention = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(owner.as_str())?
                .path(source.path.as_str())?
                .interface(source_registry::SOURCE_IFACE)?
                .member("CredentialsRequired")?
                .build();
            Ok(vec![
                MessageStream::for_match_rule(properties, &conn, Some(256)).await?,
                MessageStream::for_match_rule(attention, &conn, Some(256)).await?,
            ])
        })
        .await;
        let Ok(streams) = streams else {
            log::warn!("[eds] source health subscription failed");
            wake.notify_one();
            return;
        };
        let read_status = || {
            bounded(async {
                Ok(Proxy::new(
                    &conn,
                    owner.as_str(),
                    source.path.as_str(),
                    source_registry::SOURCE_IFACE,
                )
                .await?
                .get_property::<String>("ConnectionStatus")
                .await?)
            })
        };
        if let Ok(status) = read_status().await {
            self.source_connection_status(&source.uid, generation, epoch, &status, &wake);
        }
        let mut stream: futures_util::stream::SelectAll<_> = streams.into_iter().collect();
        while let Some(result) = stream.next().await {
            let Ok(message) = result else {
                break;
            };
            if !self.current(&source.uid, generation, epoch) {
                break;
            }
            let header = message.header();
            if !header.sender().is_some_and(|s| s.as_str() == owner) {
                continue;
            }
            if header
                .member()
                .is_some_and(|m| m.as_str() == "CredentialsRequired")
            {
                if let Ok((reason, _, _, _, _)) =
                    message
                        .body()
                        .deserialize::<(String, String, String, String, String)>()
                {
                    let error = details(
                        match reason.as_str() {
                            "ssl-failed" => "eds.certificate-required",
                            "required" | "rejected" => "eds.credentials-required",
                            _ => "eds.refresh-failed",
                        },
                        "Calendar backend requires attention",
                    );
                    let mut st = self.state.lock_or_recover();
                    if st.generation == generation
                        && let Some(entry) = st
                            .sources
                            .get_mut(&source.uid)
                            .filter(|s| s.generation == epoch)
                    {
                        entry.status.error = Some(error);
                    }
                    drop(st);
                    self.notifier.notify();
                }
            } else if header
                .member()
                .is_some_and(|m| m.as_str() == "PropertiesChanged")
                && let Ok((iface, values, missing)) = message.body().deserialize::<(
                    String,
                    HashMap<String, zvariant::OwnedValue>,
                    Vec<String>,
                )>()
            {
                if iface != source_registry::SOURCE_IFACE {
                    continue;
                }
                let status = if missing.iter().any(|p| p == "ConnectionStatus") {
                    read_status().await.ok()
                } else {
                    values
                        .get("ConnectionStatus")
                        .and_then(|v| String::try_from(v.clone()).ok())
                };
                if let Some(status) = status {
                    self.source_connection_status(&source.uid, generation, epoch, &status, &wake);
                }
            }
        }
        log::debug!("[eds] source health monitor stopped");
    }
    fn source_connection_status(
        &self,
        uid: &str,
        generation: u64,
        epoch: u64,
        status: &str,
        wake: &Notify,
    ) {
        let mut st = self.state.lock_or_recover();
        if st.generation == generation
            && let Some(entry) = st.sources.get_mut(uid).filter(|s| s.generation == epoch)
        {
            match status {
                "awaiting-credentials" | "ssl-failed" => {
                    entry.status.error = Some(details(
                        if status == "ssl-failed" {
                            "eds.certificate-required"
                        } else {
                            "eds.credentials-required"
                        },
                        "Calendar backend requires attention",
                    ));
                }
                "connected" => {
                    let attention = entry.status.error.as_ref().is_some_and(|e| {
                        matches!(
                            e.code.as_str(),
                            "eds.credentials-required" | "eds.certificate-required" | "eds.offline"
                        )
                    });
                    if attention {
                        entry.status.error = None;
                        if !entry.refresh_active {
                            entry.refresh_pending = true;
                        }
                        self.refresh.notify_one();
                    }
                    // Connection repair is not proof that an invalid/incomplete view recovered.
                    if attention
                        || matches!(
                            entry.status.state,
                            CalendarSourceState::RetryWait | CalendarSourceState::Unavailable
                        )
                    {
                        wake.notify_one();
                    }
                }
                _ => {}
            }
        }
        drop(st);
        self.notifier.notify();
    }
}
async fn create_view(
    backend: Backend,
    uid: &str,
    revision: Arc<AtomicU64>,
    wake: Arc<Notify>,
    context: (&EdsPlugin, u64, u64),
) -> Result<View> {
    let (range, query) = ical::build_time_range_query_from_today();
    let proxy = Proxy::new(
        &backend.conn,
        backend.owner.as_str(),
        backend.path.as_str(),
        CALENDAR,
    )
    .await?;
    let path: zvariant::OwnedObjectPath = proxy.call("GetView", &(query.as_str(),)).await?;
    let path = path.to_string();
    let events = Arc::new(Mutex::new(HashMap::new()));
    let complete = Arc::new(AtomicU8::new(0));
    let completed = Arc::new(Notify::new());
    let parse_failed = Arc::new(AtomicBool::new(false));
    let invalid = parse_failed.clone();
    let data = events.clone();
    let completion = complete.clone();
    let done = completed.clone();
    let owner = backend.owner.clone();
    let source_uid = uid.to_string();
    let mut view = View {
        backend,
        path,
        reader: None,
        events,
        complete,
        completed,
        disposed: false,
        parse_failed,
    };
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(view.backend.owner.as_str())?
        .path(view.path.as_str())?
        .interface(VIEW)?
        .build();
    let mut stream = MessageStream::for_match_rule(rule, &view.backend.conn, Some(256)).await?;
    let plugin = context.0.clone();
    let generation = context.1;
    let epoch = context.2;
    let steady = Arc::new(AtomicBool::new(false));
    let reader = tokio::spawn(async move {
        while let Some(result) = stream.next().await {
            let Ok(message) = result else {
                break;
            };
            let header = message.header();
            if !header.sender().is_some_and(|s| s.as_str() == owner) {
                continue;
            }
            if header.member().is_some_and(|m| {
                matches!(
                    m.as_str(),
                    "ObjectsAdded" | "ObjectsModified" | "ObjectsRemoved"
                )
            }) {
                let mut st = plugin.state.lock_or_recover();
                if st.generation == generation
                    && let Some(entry) = st
                        .sources
                        .get_mut(&source_uid)
                        .filter(|s| s.generation == epoch)
                {
                    entry.status.last_event_delivery = Some(unix_now());
                }
            }
            match header.member().map(zbus::names::MemberName::as_str) {
                Some("ObjectsAdded") if !steady.load(Ordering::SeqCst) => {
                    if let Ok((items,)) = message.body().deserialize::<(Vec<String>,)>() {
                        let (parsed, malformed) =
                            ical::parse_ical_events_with_diagnostics(&items, range);
                        if malformed {
                            invalid.store(true, Ordering::SeqCst);
                        }
                        let mut data = data.lock_or_recover();
                        for mut event in parsed {
                            event.source_uid = source_uid.clone();
                            let id = format!(
                                "v2::{}::{}::{}",
                                source_registry::encode_identifier(&source_uid),
                                source_registry::encode_identifier(&event.uid),
                                event.start_time
                            );
                            data.insert(id, event);
                        }
                    } else {
                        revision.fetch_add(1, Ordering::SeqCst);
                    }
                }
                Some("Complete") => {
                    let success = message
                        .body()
                        .deserialize::<(String, String)>()
                        .is_ok_and(|(name, message)| name.is_empty() && message.is_empty());
                    steady.store(success, Ordering::SeqCst);
                    completion.store(if success { 1 } else { 2 }, Ordering::SeqCst);
                    done.notify_one();
                }
                Some("ObjectsAdded" | "ObjectsModified" | "ObjectsRemoved") => {
                    revision.fetch_add(1, Ordering::SeqCst);
                    wake.notify_one();
                }
                _ => {}
            }
        }
        completion.store(3, Ordering::SeqCst);
        done.notify_one();
        revision.fetch_add(1, Ordering::SeqCst);
        wake.notify_one();
        log::debug!("[eds] view reader stopped");
    });
    view.reader = Some(reader);
    Proxy::new(
        &view.backend.conn,
        view.backend.owner.as_str(),
        view.path.as_str(),
        VIEW,
    )
    .await?
    .call::<_, _, ()>("Start", &())
    .await?;
    Ok(view)
}
