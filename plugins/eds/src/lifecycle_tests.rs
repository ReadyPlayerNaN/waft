//! Wire-level fixtures on disposable buses, using production supervisors and views.
use crate::{EdsConfig, EdsPlugin, source_registry};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, Notify},
};
use waft_plugin::{EntityNotifier, Plugin, StateLocker, Urn};
use waft_protocol::entity::{
    accounts::Availability,
    calendar::{self, CalendarSourceState, RefreshOutcome},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
struct Bus {
    child: Child,
    address: String,
    _dir: tempfile::TempDir,
}
impl Bus {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("private bus directory");
        let address = format!("unix:path={}", dir.path().join("bus").display());
        let mut child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address=1",
                "--address",
                &address,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("dbus-daemon required");
        let mut lines = BufReader::new(child.stdout.take().expect("bus stdout")).lines();
        tokio::time::timeout(Duration::from_secs(2), lines.next_line())
            .await
            .expect("startup deadline")
            .expect("stdout")
            .expect("bus address");
        Self {
            child,
            address,
            _dir: dir,
        }
    }
    async fn restart(&mut self) {
        self.stop().await;
        let mut child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address=1",
                "--address",
                &self.address,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("replacement bus");
        let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
        lines
            .next_line()
            .await
            .expect("address")
            .expect("bus listening");
        self.child = child;
    }
    async fn stop(&mut self) {
        self.child.kill().await.expect("kill bus");
        self.child.wait().await.expect("reap bus");
    }
}
#[derive(Default)]
struct Fixture {
    objects: Mutex<Objects>,
    items: Mutex<Vec<String>>,
    views: Mutex<HashMap<String, ()>>,
    view_count: AtomicUsize,
    max_views: AtomicUsize,
    close_count: AtomicUsize,
    refresh_count: AtomicUsize,
    failed_open: AtomicBool,
    open_count: AtomicUsize,
    failed_complete: AtomicBool,
    failed_refresh: AtomicBool,
    offline: AtomicBool,
    connection_status: std::sync::Mutex<String>,
    hold_complete: AtomicBool,
    completed: Notify,
    hold_refresh: AtomicBool,
    refreshed: Notify,
    held_dispose: std::sync::Mutex<Option<String>>,
    dispose_started: Notify,
    dispose_released: Notify,
}
#[derive(Clone)]
struct Registry(Arc<Fixture>);
#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl Registry {
    async fn get_managed_objects(&self) -> Objects {
        self.0.objects.lock().await.clone()
    }
}
struct Source {
    fixture: Arc<Fixture>,
    uid: String,
}
#[zbus::interface(name = "org.gnome.evolution.dataserver.Source")]
impl Source {
    #[zbus(property, name = "UID")]
    fn uid(&self) -> &str {
        &self.uid
    }
    #[zbus(property)]
    async fn data(&self) -> String {
        self.fixture
            .objects
            .lock()
            .await
            .values()
            .filter_map(|i| i.get(source_registry::SOURCE_IFACE))
            .find(|p| {
                p.get("UID")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .as_deref()
                    == Some(self.uid.as_str())
            })
            .and_then(|p| p.get("Data"))
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default()
    }
    #[zbus(property)]
    fn connection_status(&self) -> String {
        let value = self.fixture.connection_status.lock().expect("status");
        if value.is_empty() {
            "connected".into()
        } else {
            value.clone()
        }
    }
    #[zbus(property)]
    fn set_connection_status(&self, value: &str) {
        *self.fixture.connection_status.lock().expect("status") = value.into();
    }
    #[zbus(signal)]
    async fn credentials_required(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        reason: &str,
        certificate_pem: &str,
        certificate_errors: &str,
        dbus_error_name: &str,
        dbus_error_message: &str,
    ) -> zbus::Result<()>;
}
struct Factory(Arc<Fixture>);
#[zbus::interface(name = "org.gnome.evolution.dataserver.CalendarFactory")]
impl Factory {
    fn open_calendar(
        &self,
        _uid: &str,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> zbus::fdo::Result<(String, String)> {
        self.0.open_count.fetch_add(1, Ordering::SeqCst);
        if self.0.failed_open.swap(false, Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed("fixture setup failure".into()));
        }
        // Deliberately share a backend: production must reference-count Close.
        Ok((
            "/org/gnome/evolution/dataserver/Calendar/fixture".into(),
            conn.unique_name().expect("owner").to_string(),
        ))
    }
}
struct Calendar(Arc<Fixture>);
#[zbus::interface(name = "org.gnome.evolution.dataserver.Calendar")]
impl Calendar {
    fn open(&self) -> Vec<String> {
        vec![]
    }
    fn close(&self) {
        self.0.close_count.fetch_add(1, Ordering::SeqCst);
    }
    #[zbus(property)]
    fn online(&self) -> bool {
        !self.0.offline.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn capabilities(&self) -> Vec<String> {
        vec!["refresh-supported".into()]
    }
    async fn refresh(&self) -> zbus::fdo::Result<()> {
        self.0.refresh_count.fetch_add(1, Ordering::SeqCst);
        if self.0.hold_refresh.load(Ordering::SeqCst) {
            self.0.refreshed.notified().await;
        }
        if self.0.failed_refresh.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed(
                "fixture private error marker".into(),
            ));
        }
        Ok(())
    }
    #[zbus(signal)]
    async fn error(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        error_message: &str,
    ) -> zbus::Result<()>;
    async fn get_view(
        &self,
        _query: &str,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let sequence = self.0.view_count.fetch_add(1, Ordering::SeqCst);
        let path = format!("/org/gnome/evolution/dataserver/CalendarView/view_{sequence}");
        conn.object_server()
            .at(
                path.as_str(),
                View {
                    fixture: self.0.clone(),
                    path: path.clone(),
                },
            )
            .await
            .map_err(|_| zbus::fdo::Error::Failed("fixture view registration".into()))?;
        let mut views = self.0.views.lock().await;
        views.insert(path.clone(), ());
        self.0.max_views.fetch_max(views.len(), Ordering::SeqCst);
        Ok(OwnedObjectPath::try_from(path).expect("view path"))
    }
}
struct View {
    fixture: Arc<Fixture>,
    path: String,
}
#[zbus::interface(name = "org.gnome.evolution.dataserver.CalendarView")]
impl View {
    #[zbus(signal)]
    async fn objects_added(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        objects: Vec<String>,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn objects_modified(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        objects: Vec<String>,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn objects_removed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        uids: Vec<String>,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn complete(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        error_name: &str,
        error_message: &str,
    ) -> zbus::Result<()>;
    async fn start(&self, #[zbus(connection)] conn: &zbus::Connection) {
        let fixture = self.fixture.clone();
        let conn = conn.clone();
        let path = self.path.clone();
        tokio::spawn(async move {
            let items = fixture.items.lock().await.clone();
            conn.emit_signal(
                None::<&str>,
                path.as_str(),
                "org.gnome.evolution.dataserver.CalendarView",
                "ObjectsAdded",
                &(items,),
            )
            .await
            .expect("initial batch");
            if fixture.hold_complete.load(Ordering::SeqCst) {
                fixture.completed.notified().await;
            }
            let error = if fixture.failed_complete.swap(false, Ordering::SeqCst) {
                "fixture.incomplete"
            } else {
                ""
            };
            conn.emit_signal(
                None::<&str>,
                path.as_str(),
                "org.gnome.evolution.dataserver.CalendarView",
                "Complete",
                &(error, ""),
            )
            .await
            .expect("completion");
        });
    }
    fn stop(&self) {}
    async fn dispose(&self, #[zbus(connection)] conn: &zbus::Connection) {
        let held = self
            .fixture
            .held_dispose
            .lock()
            .expect("held Dispose")
            .as_deref()
            == Some(self.path.as_str());
        if held {
            let released = self.fixture.dispose_released.notified();
            self.fixture.dispose_started.notify_one();
            released.await;
        }
        self.fixture.views.lock().await.remove(&self.path);
        let conn = conn.clone();
        let path = self.path.clone();
        tokio::spawn(async move {
            conn.object_server()
                .remove::<View, _>(path.as_str())
                .await
                .expect("remove retired fixture view");
        });
    }
}
fn sources(ids: &[&str], enabled: bool) -> Objects {
    ids.iter().map(|uid| {
        let value = |text: String| OwnedValue::try_from(zbus::zvariant::Value::from(text)).expect("fixture value");
        (OwnedObjectPath::try_from(format!("/org/gnome/evolution/dataserver/Source/{uid}")).expect("source path"), HashMap::from([(source_registry::SOURCE_IFACE.into(), HashMap::from([
            ("UID".into(), value((*uid).into())), ("Data".into(), value(format!("[Data Source]\nDisplayName=Fixture\nEnabled={enabled}\n[Calendar]\n"))),
        ]))]))
    }).collect()
}
fn item(summary: &str) -> String {
    format!(
        "BEGIN:VEVENT\nUID:shared@host::é\nSUMMARY:{summary}\nDTSTART:20260216T100000Z\nDTEND:20260216T110000Z\nEND:VEVENT"
    )
}
async fn serve(bus: &Bus, fixture: Arc<Fixture>) -> zbus::Connection {
    let source_nodes: Vec<_> = fixture
        .objects
        .lock()
        .await
        .iter()
        .filter_map(|(path, i)| {
            i.get(source_registry::SOURCE_IFACE)
                .and_then(|p| p.get("UID"))
                .and_then(|v| String::try_from(v.clone()).ok())
                .map(|uid| (path.clone(), uid))
        })
        .collect();
    let mut builder = zbus::connection::Builder::address(bus.address.as_str())
        .expect("address")
        .name(source_registry::SOURCES_DEST)
        .expect("registry name")
        .name(source_registry::FACTORY_DEST)
        .expect("factory name")
        .serve_at(source_registry::SOURCES_PATH, Registry(fixture.clone()))
        .expect("registry")
        .serve_at(source_registry::FACTORY_PATH, Factory(fixture.clone()))
        .expect("factory")
        .serve_at(
            "/org/gnome/evolution/dataserver/Calendar/fixture",
            Calendar(fixture.clone()),
        )
        .expect("calendar");
    for (path, uid) in source_nodes {
        builder = builder
            .serve_at(
                path,
                Source {
                    fixture: fixture.clone(),
                    uid,
                },
            )
            .expect("source");
    }
    builder.build().await.expect("service")
}
fn plugin() -> EdsPlugin {
    let (notifier, _receiver) = EntityNotifier::new_pair();
    EdsPlugin::new(notifier, EdsConfig::default())
}
async fn until(predicate: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if predicate() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("convergence deadline");
}
async fn invalidate(conn: &zbus::Connection) {
    conn.emit_signal(
        None::<&str>,
        source_registry::SOURCES_PATH,
        "org.freedesktop.DBus.ObjectManager",
        "InterfacesAdded",
        &(
            OwnedObjectPath::try_from("/org/gnome/evolution/dataserver/Source/fixture")
                .expect("path"),
            HashMap::<String, HashMap<String, OwnedValue>>::new(),
        ),
    )
    .await
    .expect("registry invalidation");
}
async fn stop_task(task: tokio::task::JoinHandle<()>) {
    task.abort();
    task.await.expect_err("task aborted");
}

#[tokio::test]
async fn late_registry_and_failed_open_recover_without_restart() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut bus = Bus::new().await;
        let plugin = plugin();
        let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
        until(|| plugin.state.lock_or_recover().sync.availability == Availability::Unavailable)
            .await;
        let fixture = Arc::new(Fixture::default());
        *fixture.objects.lock().await = sources(&["fixture"], true);
        *fixture.items.lock().await = vec![item("initial")];
        fixture.failed_open.store(true, Ordering::SeqCst);
        let _service = serve(&bus, fixture.clone()).await;
        until(|| fixture.open_count.load(Ordering::SeqCst) >= 2).await;
        until(|| {
            plugin
                .state
                .lock_or_recover()
                .sources
                .get("fixture")
                .is_some_and(|s| s.status.state == CalendarSourceState::Watching)
        })
        .await;
        assert_eq!(
            plugin.state.lock_or_recover().sources["fixture"]
                .events
                .len(),
            1
        );
        stop_task(supervisor).await;
        bus.stop().await;
    })
    .await
    .expect("outer deadline");
}
#[tokio::test]
async fn complete_is_required_and_refresh_returns_before_remote_work() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut bus = Bus::new().await;
        let fixture = Arc::new(Fixture::default());
        *fixture.objects.lock().await = sources(&["fixture"], true);
        *fixture.items.lock().await = vec![item("initial")];
        fixture.hold_complete.store(true, Ordering::SeqCst);
        fixture.hold_refresh.store(true, Ordering::SeqCst);
        let _service = serve(&bus, fixture.clone()).await;
        let plugin = plugin();
        let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
        let refresh = tokio::spawn(plugin.clone().refresh_loop());
        until(|| {
            plugin
                .state
                .lock_or_recover()
                .sources
                .get("fixture")
                .is_some_and(|s| s.status.state == CalendarSourceState::Loading)
        })
        .await;
        assert!(
            plugin
                .get_entities()
                .iter()
                .all(|e| e.entity_type != calendar::ENTITY_TYPE)
        );
        fixture.hold_complete.store(false, Ordering::SeqCst);
        fixture.completed.notify_waiters();
        until(|| fixture.refresh_count.load(Ordering::SeqCst) == 1).await;
        let started = std::time::Instant::now();
        let result = plugin
            .handle_action(
                Urn::new("eds", calendar::CALENDAR_SYNC_ENTITY_TYPE, "singleton"),
                "refresh".into(),
                waft_plugin::serde_json::Value::Null,
            )
            .await
            .expect("enqueue");
        assert_eq!(result["outcome"], "queued");
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(plugin.state.lock_or_recover().sync.syncing);
        assert!(!plugin.can_stop());
        fixture.hold_refresh.store(false, Ordering::SeqCst);
        fixture.refreshed.notify_waiters();
        until(|| !plugin.state.lock_or_recover().sync.syncing).await;
        assert_eq!(
            plugin.state.lock_or_recover().sync.refresh_outcome,
            RefreshOutcome::Accepted
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            fixture.refresh_count.load(Ordering::SeqCst),
            1,
            "in-flight requests merge"
        );
        stop_task(refresh).await;
        stop_task(supervisor).await;
        bus.stop().await;
    })
    .await
    .expect("outer deadline");
}
#[tokio::test]
async fn duplicate_uids_and_shared_backend_cleanup_are_source_local() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut bus = Bus::new().await;
        let fixture = Arc::new(Fixture::default());
        *fixture.objects.lock().await = sources(&["a", "b"], true);
        *fixture.items.lock().await = vec![item("shared")];
        let service = serve(&bus, fixture.clone()).await;
        let plugin = plugin();
        let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
        until(|| {
            let st = plugin.state.lock_or_recover();
            st.sources.len() == 2
                && st
                    .sources
                    .values()
                    .all(|s| s.status.state == CalendarSourceState::Watching)
        })
        .await;
        let events: Vec<_> = plugin
            .get_entities()
            .into_iter()
            .filter(|e| e.entity_type == calendar::ENTITY_TYPE)
            .collect();
        assert_eq!(events.len(), 2);
        assert_ne!(events[0].urn, events[1].urn);
        *fixture.objects.lock().await = sources(&["b"], true);
        invalidate(&service).await;
        until(|| plugin.state.lock_or_recover().sources.len() == 1).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            fixture.close_count.load(Ordering::SeqCst),
            0,
            "remaining source owns shared backend"
        );
        fixture.objects.lock().await.clear();
        invalidate(&service).await;
        until(|| plugin.state.lock_or_recover().sources.is_empty()).await;
        until(|| fixture.close_count.load(Ordering::SeqCst) == 1).await;
        assert!(fixture.views.lock().await.is_empty());
        stop_task(supervisor).await;
        bus.stop().await;
    })
    .await
    .expect("outer deadline");
}
async fn connection_status_signal(service: &zbus::Connection) {
    service
        .emit_signal(
            None::<&str>,
            "/org/gnome/evolution/dataserver/Source/fixture",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                source_registry::SOURCE_IFACE,
                HashMap::<String, OwnedValue>::new(),
                vec!["ConnectionStatus"],
            ),
        )
        .await
        .expect("status invalidation");
}
#[tokio::test]
async fn initial_attention_invalidated_status_and_offline_signals_remain_sanitized() {
    let mut bus = Bus::new().await;
    let fixture = Arc::new(Fixture::default());
    *fixture.objects.lock().await = sources(&["fixture"], true);
    *fixture.items.lock().await = vec![item("cached")];
    *fixture.connection_status.lock().expect("status") = "awaiting-credentials".into();
    let service = serve(&bus, fixture.clone()).await;
    let plugin = plugin();
    let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
    until(|| {
        plugin
            .state
            .lock_or_recover()
            .sources
            .get("fixture")
            .is_some_and(|s| {
                s.status.state == CalendarSourceState::Watching
                    && s.status
                        .error
                        .as_ref()
                        .is_some_and(|e| e.code == "eds.credentials-required")
            })
    })
    .await;
    service
        .emit_signal(
            None::<&str>,
            "/org/gnome/evolution/dataserver/Calendar/fixture",
            "org.gnome.evolution.dataserver.Calendar",
            "Error",
            &("fixture private error marker",),
        )
        .await
        .expect("backend error");
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        plugin.state.lock_or_recover().sources["fixture"]
            .status
            .error
            .as_ref()
            .expect("attention")
            .code,
        "eds.credentials-required"
    );
    *fixture.connection_status.lock().expect("status") = "ssl-failed".into();
    connection_status_signal(&service).await;
    until(|| {
        plugin.state.lock_or_recover().sources["fixture"]
            .status
            .error
            .as_ref()
            .is_some_and(|e| e.code == "eds.certificate-required")
    })
    .await;
    *fixture.connection_status.lock().expect("status") = "connected".into();
    connection_status_signal(&service).await;
    until(|| {
        plugin.state.lock_or_recover().sources["fixture"]
            .status
            .error
            .is_none()
    })
    .await;
    fixture.offline.store(true, Ordering::SeqCst);
    service
        .emit_signal(
            None::<&str>,
            "/org/gnome/evolution/dataserver/Calendar/fixture",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                "org.gnome.evolution.dataserver.Calendar",
                HashMap::from([("Online", OwnedValue::from(false))]),
                Vec::<String>::new(),
            ),
        )
        .await
        .expect("offline");
    until(|| {
        plugin.state.lock_or_recover().sources["fixture"]
            .status
            .online
            == Some(false)
    })
    .await;
    service
        .emit_signal(
            None::<&str>,
            "/org/gnome/evolution/dataserver/Source/fixture",
            source_registry::SOURCE_IFACE,
            "CredentialsRequired",
            &(
                "unknown",
                "fixture certificate marker",
                "fixture errors",
                "fixture error name",
                "fixture private error marker",
            ),
        )
        .await
        .expect("unknown reason");
    until(|| {
        plugin.state.lock_or_recover().sources["fixture"]
            .status
            .error
            .as_ref()
            .is_some_and(|e| e.code == "eds.refresh-failed")
    })
    .await;
    let error = plugin.state.lock_or_recover().sources["fixture"]
        .status
        .error
        .clone()
        .expect("diagnostic");
    assert!(!error.message.contains("marker"));
    stop_task(supervisor).await;
    bus.stop().await;
}
#[tokio::test]
async fn failed_candidate_and_malformed_configuration_keep_the_previous_snapshot() {
    let mut bus = Bus::new().await;
    let fixture = Arc::new(Fixture::default());
    *fixture.objects.lock().await = sources(&["fixture"], true);
    *fixture.items.lock().await = vec![item("old")];
    let service = serve(&bus, fixture.clone()).await;
    let plugin = plugin();
    let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
    until(|| {
        plugin
            .state
            .lock_or_recover()
            .sources
            .get("fixture")
            .is_some_and(|s| s.status.state == CalendarSourceState::Watching)
    })
    .await;
    fixture.failed_complete.store(true, Ordering::SeqCst);
    *fixture.items.lock().await = vec![item("replacement")];
    let path = fixture
        .views
        .lock()
        .await
        .keys()
        .next()
        .expect("view")
        .clone();
    service
        .emit_signal(
            None::<&str>,
            path.as_str(),
            "org.gnome.evolution.dataserver.CalendarView",
            "ObjectsModified",
            &(vec![item("replacement")],),
        )
        .await
        .expect("modified");
    until(|| {
        plugin.state.lock_or_recover().sources["fixture"]
            .status
            .state
            == CalendarSourceState::RetryWait
    })
    .await;
    assert!(
        plugin.state.lock_or_recover().sources["fixture"]
            .events
            .values()
            .all(|e| e.summary == "old")
    );
    for interfaces in fixture.objects.lock().await.values_mut() {
        interfaces
            .get_mut(source_registry::SOURCE_IFACE)
            .expect("source")
            .insert(
                "Data".into(),
                OwnedValue::from(zbus::zvariant::Str::from(
                    "[Data Source]\nEnabled=malformed\n",
                )),
            );
    }
    invalidate(&service).await;
    until(|| {
        plugin.state.lock_or_recover().sources["fixture"]
            .status
            .state
            == CalendarSourceState::Unsupported
    })
    .await;
    assert_eq!(
        plugin.state.lock_or_recover().sources["fixture"]
            .events
            .len(),
        1
    );
    *fixture.objects.lock().await = sources(&["fixture"], true);
    invalidate(&service).await;
    until(|| {
        let st = plugin.state.lock_or_recover();
        st.sources["fixture"].status.state == CalendarSourceState::Watching
            && st.sources["fixture"]
                .events
                .values()
                .any(|e| e.summary == "replacement")
    })
    .await;
    assert!(fixture.max_views.load(Ordering::SeqCst) <= 2);
    stop_task(supervisor).await;
    bus.stop().await;
}
#[tokio::test]
async fn bus_restart_cancels_refresh_and_cannot_reuse_an_old_bus_backend_lease() {
    let mut bus = Bus::new().await;
    let fixture = Arc::new(Fixture::default());
    *fixture.objects.lock().await = sources(&["fixture"], true);
    *fixture.items.lock().await = vec![item("old bus")];
    fixture.hold_refresh.store(true, Ordering::SeqCst);
    let old_service = serve(&bus, fixture.clone()).await;
    let plugin = plugin();
    let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
    let refresh = tokio::spawn(plugin.clone().refresh_loop());
    until(|| {
        plugin.state.lock_or_recover().sync.syncing
            && fixture.refresh_count.load(Ordering::SeqCst) == 1
    })
    .await;
    let old_backend = plugin.state.lock_or_recover().sources["fixture"]
        .backend
        .clone()
        .expect("old lease");
    // Deterministic cancellation: retire the owner while its RPC is still held,
    // rather than racing a socket error against observation of bus shutdown.
    old_service
        .release_name(source_registry::SOURCES_DEST)
        .await
        .expect("retire registry owner");
    until(|| plugin.state.lock_or_recover().sync.refresh_outcome == RefreshOutcome::Cancelled)
        .await;
    bus.restart().await;
    until(|| plugin.state.lock_or_recover().sync.refresh_outcome == RefreshOutcome::Cancelled)
        .await;
    assert!(
        plugin.state.lock_or_recover().sources["fixture"]
            .events
            .values()
            .any(|e| e.summary == "old bus")
    );
    let replacement = Arc::new(Fixture::default());
    *replacement.objects.lock().await = sources(&["fixture"], true);
    *replacement.items.lock().await = vec![item("new bus")];
    let service = serve(&bus, replacement.clone()).await;
    until(|| {
        let st = plugin.state.lock_or_recover();
        st.sources["fixture"].status.state == CalendarSourceState::Watching
            && st.sources["fixture"]
                .events
                .values()
                .any(|e| e.summary == "new bus")
    })
    .await;
    assert_ne!(
        old_backend.conn.server_guid(),
        plugin.state.lock_or_recover().sources["fixture"]
            .backend
            .as_ref()
            .expect("new lease")
            .conn
            .server_guid()
    );
    fixture.hold_refresh.store(false, Ordering::SeqCst);
    fixture.refreshed.notify_waiters();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        plugin.state.lock_or_recover().sources["fixture"]
            .events
            .values()
            .all(|e| e.summary == "new bus")
    );
    drop(old_backend);
    drop(old_service);
    stop_task(refresh).await;
    stop_task(supervisor).await;
    drop(service);
    bus.stop().await;
}
async fn check_wire_contract(service: &zbus::Connection, path: &str, interface: &str) {
    let proxy = zbus::Proxy::new(
        service,
        service.unique_name().expect("owner").as_str(),
        path,
        "org.freedesktop.DBus.Introspectable",
    )
    .await
    .expect("introspection proxy");
    let xml: String = proxy.call("Introspect", &()).await.expect("live XML");
    let live = roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .expect("live XML parse");
    let pinned = roxmltree::Document::parse(include_str!("../tests/fixtures/contracts.xml"))
        .expect("pinned XML");
    let expected = pinned
        .descendants()
        .find(|n| n.has_tag_name("interface") && n.attribute("name") == Some(interface))
        .expect("pinned interface");
    let actual = live
        .descendants()
        .find(|n| n.has_tag_name("interface") && n.attribute("name") == Some(interface))
        .expect("live interface");
    for member in expected.children().filter(roxmltree::Node::is_element) {
        let observed = actual
            .children()
            .find(|n| {
                n.tag_name().name() == member.tag_name().name()
                    && n.attribute("name") == member.attribute("name")
            })
            .expect("consumed wire member");
        if member.has_tag_name("property") {
            assert_eq!(observed.attribute("type"), member.attribute("type"));
        } else {
            let args = |node: roxmltree::Node<'_, '_>| {
                node.children()
                    .filter(|n| n.has_tag_name("arg"))
                    .map(|n| {
                        (
                            n.attribute("type").unwrap_or("").to_owned(),
                            n.attribute("direction").unwrap_or("in").to_owned(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                args(observed),
                args(member),
                "{}",
                member.attribute("name").expect("member")
            );
        }
    }
}
#[tokio::test]
async fn private_bus_mocks_match_pinned_consumed_wire_contracts() {
    let mut bus = Bus::new().await;
    let fixture = Arc::new(Fixture::default());
    *fixture.objects.lock().await = sources(&["fixture"], true);
    let service = serve(&bus, fixture).await;
    check_wire_contract(
        &service,
        "/org/gnome/evolution/dataserver/Source/fixture",
        source_registry::SOURCE_IFACE,
    )
    .await;
    check_wire_contract(
        &service,
        source_registry::FACTORY_PATH,
        "org.gnome.evolution.dataserver.CalendarFactory",
    )
    .await;
    check_wire_contract(
        &service,
        "/org/gnome/evolution/dataserver/Calendar/fixture",
        "org.gnome.evolution.dataserver.Calendar",
    )
    .await;
    let calendar = zbus::Proxy::new(
        &service,
        service.unique_name().expect("owner").as_str(),
        "/org/gnome/evolution/dataserver/Calendar/fixture",
        "org.gnome.evolution.dataserver.Calendar",
    )
    .await
    .expect("calendar");
    let view: OwnedObjectPath = calendar.call("GetView", &("true",)).await.expect("view");
    check_wire_contract(
        &service,
        view.as_str(),
        "org.gnome.evolution.dataserver.CalendarView",
    )
    .await;
    calendar
        .call::<_, _, ()>("Close", &())
        .await
        .expect("close");
    drop(service);
    bus.stop().await;
}
#[tokio::test]
async fn live_invalidation_during_old_view_disposal_is_not_swallowed() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut bus = Bus::new().await;
        let fixture = Arc::new(Fixture::default());
        *fixture.objects.lock().await = sources(&["fixture"], true);
        *fixture.items.lock().await = vec![item("initial")];
        let service = serve(&bus, fixture.clone()).await;
        let plugin = plugin();
        let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
        until(|| plugin.state.lock_or_recover().sources.get("fixture").is_some_and(|s| s.status.state == CalendarSourceState::Watching)).await;
        let old_path = fixture.views.lock().await.keys().next().expect("initial view").clone();
        *fixture.held_dispose.lock().expect("hold old Dispose") = Some(old_path.clone());
        *fixture.items.lock().await = vec![item("replacement")];
        service.emit_signal(None::<&str>, old_path.as_str(), "org.gnome.evolution.dataserver.CalendarView", "ObjectsModified", &(vec![item("replacement")],)).await.expect("trigger replacement");
        tokio::time::timeout(Duration::from_secs(4), fixture.dispose_started.notified()).await.expect("old view is being disposed");
        assert!(plugin.state.lock_or_recover().sources["fixture"].events.values().any(|e| e.summary == "replacement"), "replacement published before retirement finishes");
        let new_path = fixture.views.lock().await.keys().find(|p| *p != &old_path).expect("completed replacement view").clone();
        // The old reader is aborted; only the new steady reader can observe this
        // delivery marker. This proves invalidation happened inside the await.
        plugin.state.lock_or_recover().sources.get_mut("fixture").expect("source").status.last_event_delivery = None;
        *fixture.items.lock().await = vec![item("changed during disposal")];
        service.emit_signal(None::<&str>, new_path.as_str(), "org.gnome.evolution.dataserver.CalendarView", "ObjectsModified", &(vec![item("changed during disposal")],)).await.expect("modify new steady view");
        until(|| plugin.state.lock_or_recover().sources["fixture"].status.last_event_delivery.is_some()).await;
        fixture.dispose_released.notify_one();
        let converged = tokio::time::timeout(Duration::from_secs(2), async {
            until(|| plugin.state.lock_or_recover().sources["fixture"].events.values().any(|e| e.summary == "changed during disposal")).await;
            until(|| fixture.views.try_lock().is_ok_and(|views| views.len() == 1)).await;
        }).await.is_ok();
        let count = fixture.view_count.load(Ordering::SeqCst);
        let max_views = fixture.max_views.load(Ordering::SeqCst);
        fixture.objects.lock().await.clear();
        invalidate(&service).await;
        until(|| fixture.close_count.load(Ordering::SeqCst) == 1).await;
        assert!(fixture.views.lock().await.is_empty());
        stop_task(supervisor).await;
        bus.stop().await;
        assert!(converged, "live invalidation during old-view Dispose must converge without another signal or midnight");
        assert_eq!(count, 3, "one subsequent snapshot consumes the invalidation");
        assert!(max_views <= 2, "active plus candidate remains bounded");
    }).await.expect("retirement race deadline");
}
#[tokio::test]
async fn view_churn_replaces_membership_and_retires_server_resources() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut bus = Bus::new().await;
        let fixture = Arc::new(Fixture::default());
        *fixture.objects.lock().await = sources(&["fixture"], true);
        *fixture.items.lock().await = vec![item("initial")];
        let service = serve(&bus, fixture.clone()).await;
        let plugin = plugin();
        let supervisor = tokio::spawn(plugin.clone().supervise(Some(bus.address.clone())));
        until(|| {
            plugin
                .state
                .lock_or_recover()
                .sources
                .get("fixture")
                .is_some_and(|s| s.status.state == CalendarSourceState::Watching)
        })
        .await;
        for i in 0..100 {
            let expected = format!("revision-{i}");
            *fixture.items.lock().await = vec![item(&expected)];
            let path = fixture
                .views
                .lock()
                .await
                .keys()
                .next()
                .expect("active view")
                .clone();
            service
                .emit_signal(
                    None::<&str>,
                    path.as_str(),
                    "org.gnome.evolution.dataserver.CalendarView",
                    "ObjectsModified",
                    &(vec![item(&expected)],),
                )
                .await
                .expect("modify");
            until(|| {
                plugin.state.lock_or_recover().sources["fixture"]
                    .events
                    .values()
                    .any(|e| e.summary == expected)
            })
            .await;
            until(|| fixture.views.try_lock().is_ok_and(|views| views.len() == 1)).await;
        }
        assert!(
            fixture.max_views.load(Ordering::SeqCst) <= 2,
            "only active plus candidate"
        );
        assert_eq!(
            plugin.state.lock_or_recover().sources["fixture"]
                .events
                .len(),
            1
        );
        fixture.objects.lock().await.clear();
        invalidate(&service).await;
        until(|| fixture.close_count.load(Ordering::SeqCst) == 1).await;
        assert!(fixture.views.lock().await.is_empty());
        stop_task(supervisor).await;
        bus.stop().await;
    })
    .await
    .expect("churn deadline");
}
