//! Production supervisor/action paths on isolated Unix buses; no environment mutation.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::AsyncBufReadExt,
    process::{Child, Command},
    sync::{Mutex, Notify},
};
use waft_plugin::{EntityNotifier, Plugin, PluginActionError};
use waft_plugin_gnome_online_accounts::{
    actions::GoaPlugin,
    dbus::{self, ManagedObjects},
    lifecycle::GoaLifecycle,
};
use waft_protocol::{
    Urn,
    entity::accounts::{Availability, OnlineAccountsStatus},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
const ACCOUNT_PATH: &str = "/org/gnome/OnlineAccounts/Accounts/fixture";
struct Bus {
    child: Child,
    address: String,
    _dir: tempfile::TempDir,
}
impl Bus {
    async fn start() -> Self {
        let dir = tempfile::tempdir().expect("bus directory");
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .arg(format!(
                "--address=unix:path={}",
                dir.path().join("bus").display()
            ))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("private bus");
        let mut lines = tokio::io::BufReader::new(child.stdout.take().expect("bus stdout")).lines();
        let _announced = lines
            .next_line()
            .await
            .expect("bus address")
            .expect("address line");
        let address = format!("unix:path={}", dir.path().join("bus").display());
        Self {
            child,
            address,
            _dir: dir,
        }
    }
    async fn restart(&mut self) {
        self.child.kill().await.expect("crash private bus");
        self.child.wait().await.expect("reap old bus");
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .arg(format!("--address={}", self.address))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("replacement bus");
        let mut lines = tokio::io::BufReader::new(child.stdout.take().expect("stdout")).lines();
        lines
            .next_line()
            .await
            .expect("address")
            .expect("replacement listening");
        self.child = child;
    }
    async fn stop(mut self) {
        self.child.kill().await.expect("stop private bus");
        self.child.wait().await.expect("reap private bus");
    }
}
#[derive(Clone, Default)]
struct Objects {
    data: Arc<Mutex<ManagedObjects>>,
    reads: Arc<AtomicUsize>,
    hold: Arc<AtomicBool>,
    gate: Arc<Notify>,
    fail: Arc<AtomicBool>,
}
#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl Objects {
    async fn get_managed_objects(&self) -> zbus::fdo::Result<ManagedObjects> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.hold.load(Ordering::SeqCst) {
            self.gate.notified().await;
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed("fixture discovery failure".into()));
        }
        Ok(self.data.lock().await.clone())
    }
}
#[derive(Clone, Default)]
struct Manager {
    fail: Arc<AtomicBool>,
    unsupported: Arc<AtomicBool>,
    hold: Arc<AtomicBool>,
    gate: Arc<Notify>,
    calls: Arc<AtomicUsize>,
}
#[zbus::interface(name = "org.gnome.OnlineAccounts.Manager")]
impl Manager {
    async fn is_supported_provider(&self, provider_type: &str) -> zbus::fdo::Result<bool> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.hold.load(Ordering::SeqCst) {
            self.gate.notified().await;
        }
        if self.unsupported.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::UnknownMethod(
                "fixture unsupported probe".into(),
            ));
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed("fixture probe failure".into()));
        }
        Ok(matches!(provider_type, "ms_graph" | "google"))
    }
}
#[derive(Clone, Default)]
struct Writes {
    count: Arc<AtomicUsize>,
    hold: Arc<AtomicBool>,
    gate: Arc<Notify>,
    ignore: Arc<AtomicBool>,
    removals: Arc<AtomicUsize>,
}
struct Account {
    objects: Objects,
    writes: Writes,
}
#[zbus::interface(name = "org.gnome.OnlineAccounts.Account")]
impl Account {
    #[zbus(property)]
    async fn calendar_disabled(&self) -> bool {
        self.objects.boolean("CalendarDisabled").await
    }
    #[zbus(property)]
    async fn set_calendar_disabled(&self, value: bool) {
        self.writes.count.fetch_add(1, Ordering::SeqCst);
        if !self.writes.ignore.load(Ordering::SeqCst) {
            self.objects
                .property("CalendarDisabled", OwnedValue::from(value))
                .await;
        }
        if self.writes.hold.load(Ordering::SeqCst) {
            self.writes.gate.notified().await;
        }
    }
    async fn remove(&self) {
        self.writes.removals.fetch_add(1, Ordering::SeqCst);
        self.objects.data.lock().await.clear();
    }
}
impl Objects {
    fn account() -> Self {
        Self {
            data: Arc::new(Mutex::new(account_objects())),
            ..Self::default()
        }
    }
    async fn property(&self, name: &str, value: OwnedValue) {
        let mut objects = self.data.lock().await;
        if let Some(props) = objects
            .values_mut()
            .find_map(|i| i.get_mut(dbus::GOA_ACCOUNT_IFACE))
        {
            props.insert(name.into(), value);
        }
    }
    async fn boolean(&self, name: &str) -> bool {
        self.data
            .lock()
            .await
            .values()
            .find_map(|i| i.get(dbus::GOA_ACCOUNT_IFACE))
            .and_then(|p| p.get(name))
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false)
    }
}
fn account_objects() -> ManagedObjects {
    let props = std::collections::HashMap::from([
        (
            "Id".into(),
            OwnedValue::from(zbus::zvariant::Str::from("fixture")),
        ),
        (
            "Identity".into(),
            OwnedValue::from(zbus::zvariant::Str::from("fixture-identity")),
        ),
        (
            "ProviderType".into(),
            OwnedValue::from(zbus::zvariant::Str::from("ms_graph")),
        ),
        ("CalendarDisabled".into(), OwnedValue::from(false)),
        ("IsLocked".into(), OwnedValue::from(false)),
        ("AttentionNeeded".into(), OwnedValue::from(false)),
    ]);
    std::collections::HashMap::from([(
        OwnedObjectPath::try_from(ACCOUNT_PATH).expect("account path"),
        std::collections::HashMap::from([(dbus::GOA_ACCOUNT_IFACE.into(), props)]),
    )])
}
async fn serve(bus: &Bus, objects: Objects, manager: Manager, writes: Writes) -> zbus::Connection {
    zbus::connection::Builder::address(bus.address.as_str())
        .expect("bus address")
        .serve_at(dbus::GOA_OBJECT_PATH, objects.clone())
        .expect("objects")
        .serve_at(dbus::GOA_MANAGER_PATH, manager)
        .expect("manager")
        .serve_at(ACCOUNT_PATH, Account { objects, writes })
        .expect("account")
        .name(dbus::GOA_BUS_NAME)
        .expect("service name")
        .build()
        .await
        .expect("service")
}
async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("lifecycle convergence");
}
fn status(lifecycle: &GoaLifecycle) -> OnlineAccountsStatus {
    lifecycle.state.lock().expect("state").status.clone()
}
fn start(bus: &Bus) -> (GoaLifecycle, tokio::task::JoinHandle<()>) {
    let (notifier, _receiver) = EntityNotifier::new_pair();
    let lifecycle = GoaLifecycle::new(notifier);
    let actor = lifecycle.clone();
    let address = bus.address.clone();
    (lifecycle, tokio::spawn(actor.run(Some(address))))
}
fn target() -> Urn {
    Urn::new("gnome-online-accounts", "online-account", "fixture")
}
fn code(error: anyhow::Error) -> String {
    error
        .downcast::<PluginActionError>()
        .expect("structured action failure")
        .0
        .code
}
async fn props_signal(service: &zbus::Connection) {
    service
        .emit_signal(
            None::<&str>,
            ACCOUNT_PATH,
            dbus::IFACE_PROPERTIES,
            "PropertiesChanged",
            &(
                dbus::GOA_ACCOUNT_IFACE,
                std::collections::HashMap::<String, OwnedValue>::new(),
                vec!["CalendarDisabled"],
            ),
        )
        .await
        .expect("properties invalidation");
}
async fn stop(task: tokio::task::JoinHandle<()>, bus: Bus) {
    task.abort();
    task.await.expect_err("stop supervisor");
    bus.stop().await;
}
#[tokio::test]
async fn late_owner_and_owner_restart_resnapshot_without_restarting_waft() {
    let bus = Bus::start().await;
    let (lifecycle, task) = start(&bus);
    until(|| status(&lifecycle).accounts == Availability::Unavailable).await;
    let service = serve(
        &bus,
        Objects::account(),
        Manager::default(),
        Writes::default(),
    )
    .await;
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && lifecycle.state.lock().expect("state").accounts.len() == 1
    })
    .await;
    service
        .release_name(dbus::GOA_BUS_NAME)
        .await
        .expect("release owner");
    until(|| status(&lifecycle).accounts != Availability::Ready).await;
    assert_eq!(
        lifecycle.state.lock().expect("stale cache").accounts.len(),
        1
    );
    let replacement = serve(
        &bus,
        Objects::default(),
        Manager::default(),
        Writes::default(),
    )
    .await;
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && lifecycle.state.lock().expect("state").accounts.is_empty()
    })
    .await;
    drop(replacement);
    drop(service);
    stop(task, bus).await;
}
#[tokio::test]
async fn hundred_owner_cycles_converge_without_accumulating_connections() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let bus = Bus::start().await;
        let service = serve(
            &bus,
            Objects::account(),
            Manager::default(),
            Writes::default(),
        )
        .await;
        let (lifecycle, task) = start(&bus);
        until(|| {
            status(&lifecycle).accounts == Availability::Ready
                && status(&lifecycle).providers == Availability::Ready
        })
        .await;
        let proxy = zbus::fdo::DBusProxy::new(&service)
            .await
            .expect("bus proxy");
        let baseline = proxy.list_names().await.expect("baseline peers").len();
        for _ in 0..100 {
            service
                .release_name(dbus::GOA_BUS_NAME)
                .await
                .expect("retire owner");
            until(|| status(&lifecycle).accounts != Availability::Ready).await;
            assert_eq!(
                lifecycle.state.lock().expect("stale cache").accounts.len(),
                1
            );
            service
                .request_name(dbus::GOA_BUS_NAME)
                .await
                .expect("restore owner");
            until(|| {
                status(&lifecycle).accounts == Availability::Ready
                    && status(&lifecycle).providers == Availability::Ready
            })
            .await;
            assert_eq!(lifecycle.state.lock().expect("accounts").accounts.len(), 1);
        }
        assert_eq!(
            proxy.list_names().await.expect("final peers").len(),
            baseline,
            "owner recovery reuses its connection"
        );
        drop(service);
        stop(task, bus).await;
    })
    .await
    .expect("owner churn deadline");
}
#[tokio::test]
async fn provider_failure_retries_without_polling_healthy_accounts() {
    let bus = Bus::start().await;
    let objects = Objects::account();
    let manager = Manager::default();
    manager.fail.store(true, Ordering::SeqCst);
    let service = serve(&bus, objects.clone(), manager.clone(), Writes::default()).await;
    let (lifecycle, task) = start(&bus);
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && status(&lifecycle).providers == Availability::Unavailable
    })
    .await;
    let reads = objects.reads.load(Ordering::SeqCst);
    until(|| manager.calls.load(Ordering::SeqCst) >= 18).await;
    assert_eq!(
        objects.reads.load(Ordering::SeqCst),
        reads,
        "provider retry cannot poll accounts"
    );
    manager.fail.store(false, Ordering::SeqCst);
    until(|| status(&lifecycle).providers == Availability::Ready).await;
    assert_eq!(objects.reads.load(Ordering::SeqCst), reads);
    drop(service);
    stop(task, bus).await;
}
#[tokio::test]
async fn provider_readiness_is_independent_of_failed_account_discovery_and_missing_probe_api() {
    let bus = Bus::start().await;
    let objects = Objects::account();
    objects.fail.store(true, Ordering::SeqCst);
    let manager = Manager::default();
    let service = serve(&bus, objects.clone(), manager.clone(), Writes::default()).await;
    let (lifecycle, task) = start(&bus);
    until(|| {
        status(&lifecycle).accounts == Availability::Unavailable
            && status(&lifecycle).providers == Availability::Ready
    })
    .await;
    objects.fail.store(false, Ordering::SeqCst);
    props_signal(&service).await;
    until(|| status(&lifecycle).accounts == Availability::Ready).await;
    service
        .release_name(dbus::GOA_BUS_NAME)
        .await
        .expect("release owner");
    manager.unsupported.store(true, Ordering::SeqCst);
    let replacement = serve(&bus, Objects::account(), manager.clone(), Writes::default()).await;
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && status(&lifecycle).providers == Availability::Unsupported
    })
    .await;
    let calls = manager.calls.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        manager.calls.load(Ordering::SeqCst),
        calls,
        "unsupported API waits for ownership recovery"
    );
    drop(replacement);
    drop(service);
    stop(task, bus).await;
}
#[tokio::test]
async fn bus_restart_reconnects_and_authoritative_empty_snapshot_removes_stale_accounts() {
    let mut bus = Bus::start().await;
    let service = serve(
        &bus,
        Objects::account(),
        Manager::default(),
        Writes::default(),
    )
    .await;
    let (lifecycle, task) = start(&bus);
    until(|| status(&lifecycle).accounts == Availability::Ready).await;
    bus.restart().await;
    until(|| status(&lifecycle).accounts != Availability::Ready).await;
    assert_eq!(
        lifecycle.state.lock().expect("stale data").accounts.len(),
        1
    );
    let replacement = serve(
        &bus,
        Objects::default(),
        Manager::default(),
        Writes::default(),
    )
    .await;
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && lifecycle.state.lock().expect("state").accounts.is_empty()
    })
    .await;
    drop(replacement);
    drop(service);
    stop(task, bus).await;
}
#[tokio::test]
async fn slow_provider_probes_do_not_block_account_property_reconciliation() {
    let bus = Bus::start().await;
    let objects = Objects::account();
    let manager = Manager::default();
    manager.hold.store(true, Ordering::SeqCst);
    let service = serve(&bus, objects.clone(), manager.clone(), Writes::default()).await;
    let (lifecycle, task) = start(&bus);
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && manager.calls.load(Ordering::SeqCst) > 0
    })
    .await;
    objects
        .property("CalendarDisabled", OwnedValue::from(true))
        .await;
    props_signal(&service).await;
    until(|| {
        lifecycle
            .state
            .lock()
            .expect("state")
            .accounts
            .get("fixture")
            .is_some_and(|a| !a.services[0].enabled)
    })
    .await;
    assert_eq!(status(&lifecycle).providers, Availability::Starting);
    manager.hold.store(false, Ordering::SeqCst);
    manager.gate.notify_waiters();
    until(|| status(&lifecycle).providers == Availability::Ready).await;
    drop(service);
    stop(task, bus).await;
}
#[tokio::test]
async fn mutations_confirm_state_enforce_locks_and_release_conflicts_on_cancellation() {
    let bus = Bus::start().await;
    let objects = Objects::account();
    let writes = Writes::default();
    let service = serve(&bus, objects.clone(), Manager::default(), writes.clone()).await;
    let (lifecycle, task) = start(&bus);
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && status(&lifecycle).providers == Availability::Ready
    })
    .await;
    let plugin = Arc::new(GoaPlugin {
        lifecycle: lifecycle.clone(),
    });
    let params = waft_plugin::serde_json::json!({"service_name":"calendar"});
    let result = plugin
        .handle_action(target(), "disable-service".into(), params.clone())
        .await
        .expect("confirmed write");
    assert_eq!(result["outcome"], "applied");
    assert_eq!(writes.count.load(Ordering::SeqCst), 1);
    writes.ignore.store(true, Ordering::SeqCst);
    assert_eq!(
        code(
            plugin
                .handle_action(target(), "enable-service".into(), params.clone())
                .await
                .expect_err("rejected confirmation")
        ),
        "goa.outcome-unconfirmed"
    );
    writes.ignore.store(false, Ordering::SeqCst);
    objects.property("IsLocked", OwnedValue::from(true)).await;
    assert_eq!(
        code(
            plugin
                .handle_action(
                    target(),
                    "remove-account".into(),
                    waft_plugin::serde_json::Value::Null
                )
                .await
                .expect_err("locked")
        ),
        "goa.locked"
    );
    assert_eq!(writes.removals.load(Ordering::SeqCst), 0);
    objects.property("IsLocked", OwnedValue::from(false)).await;
    objects.hold.store(true, Ordering::SeqCst);
    let reads = objects.reads.load(Ordering::SeqCst);
    let cloned = plugin.clone();
    let p = params.clone();
    let action = tokio::spawn(async move {
        cloned
            .handle_action(target(), "enable-service".into(), p)
            .await
    });
    until(|| !plugin.can_stop() && objects.reads.load(Ordering::SeqCst) > reads).await;
    assert_eq!(
        code(
            plugin
                .handle_action(target(), "enable-service".into(), params)
                .await
                .expect_err("conflict")
        ),
        "goa.busy"
    );
    action.abort();
    action.await.expect_err("cancel action");
    assert!(plugin.can_stop(), "conflict permit released");
    objects.hold.store(false, Ordering::SeqCst);
    objects.gate.notify_waiters();
    let removed = plugin
        .handle_action(
            target(),
            "remove-account".into(),
            waft_plugin::serde_json::Value::Null,
        )
        .await
        .expect("confirmed removal");
    assert_eq!(removed["outcome"], "removed");
    assert_eq!(writes.removals.load(Ordering::SeqCst), 1);
    drop(service);
    stop(task, bus).await;
}
#[tokio::test]
async fn owner_loss_fences_an_in_flight_write_without_replay() {
    let bus = Bus::start().await;
    let objects = Objects::account();
    let writes = Writes::default();
    writes.hold.store(true, Ordering::SeqCst);
    let service = serve(&bus, objects, Manager::default(), writes.clone()).await;
    let (lifecycle, task) = start(&bus);
    until(|| status(&lifecycle).accounts == Availability::Ready).await;
    let plugin = Arc::new(GoaPlugin {
        lifecycle: lifecycle.clone(),
    });
    let cloned = plugin.clone();
    let action = tokio::spawn(async move {
        cloned
            .handle_action(
                target(),
                "disable-service".into(),
                waft_plugin::serde_json::json!({"service_name":"calendar"}),
            )
            .await
    });
    until(|| writes.count.load(Ordering::SeqCst) == 1).await;
    service
        .release_name(dbus::GOA_BUS_NAME)
        .await
        .expect("retire owner");
    let error = tokio::time::timeout(Duration::from_secs(1), action)
        .await
        .expect("generation cancellation")
        .expect("action task")
        .expect_err("unconfirmed write");
    assert_eq!(code(error), "goa.outcome-unconfirmed");
    assert!(plugin.can_stop());
    let replacement = serve(
        &bus,
        Objects::account(),
        Manager::default(),
        Writes::default(),
    )
    .await;
    until(|| status(&lifecycle).accounts == Availability::Ready).await;
    writes.hold.store(false, Ordering::SeqCst);
    writes.gate.notify_waiters();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        writes.count.load(Ordering::SeqCst),
        1,
        "never replay on replacement"
    );
    drop(replacement);
    drop(service);
    stop(task, bus).await;
}
#[tokio::test]
async fn replacement_disappearance_failed_snapshot_and_validation_do_not_authorize_writes() {
    let bus = Bus::start().await;
    let objects = Objects::account();
    let writes = Writes::default();
    let service = serve(&bus, objects.clone(), Manager::default(), writes.clone()).await;
    let (lifecycle, task) = start(&bus);
    until(|| {
        status(&lifecycle).accounts == Availability::Ready
            && status(&lifecycle).providers == Availability::Ready
    })
    .await;
    let plugin = GoaPlugin {
        lifecycle: lifecycle.clone(),
    };
    assert_eq!(
        code(
            plugin
                .handle_action(
                    Urn::new("other", "online-account", "fixture"),
                    "remove-account".into(),
                    waft_plugin::serde_json::Value::Null
                )
                .await
                .expect_err("foreign target")
        ),
        "protocol.validation"
    );
    assert_eq!(
        code(
            plugin
                .handle_action(
                    target(),
                    "unknown".into(),
                    waft_plugin::serde_json::Value::Null
                )
                .await
                .expect_err("unknown action")
        ),
        "protocol.validation"
    );
    assert_eq!(
        code(
            plugin
                .handle_action(
                    target(),
                    "disable-service".into(),
                    waft_plugin::serde_json::json!({"service_name":"files"})
                )
                .await
                .expect_err("unestablished service")
        ),
        "goa.service-unsupported"
    );
    objects
        .property(
            "Identity",
            OwnedValue::from(zbus::zvariant::Str::from("replacement-identity")),
        )
        .await;
    assert_eq!(
        code(
            plugin
                .handle_action(
                    target(),
                    "disable-service".into(),
                    waft_plugin::serde_json::json!({"service_name":"calendar"})
                )
                .await
                .expect_err("identity replacement")
        ),
        "goa.outcome-unconfirmed"
    );
    assert_eq!(writes.count.load(Ordering::SeqCst), 0);
    objects.fail.store(true, Ordering::SeqCst);
    props_signal(&service).await;
    until(|| status(&lifecycle).accounts == Availability::Recovering).await;
    assert_eq!(
        lifecycle.state.lock().expect("stale state").accounts.len(),
        1
    );
    objects.fail.store(false, Ordering::SeqCst);
    props_signal(&service).await;
    until(|| status(&lifecycle).accounts == Availability::Ready).await;
    objects.data.lock().await.clear();
    assert_eq!(
        code(
            plugin
                .handle_action(
                    target(),
                    "remove-account".into(),
                    waft_plugin::serde_json::Value::Null
                )
                .await
                .expect_err("disappeared before dispatch")
        ),
        "entity.not-found"
    );
    assert_eq!(writes.removals.load(Ordering::SeqCst), 0);
    drop(service);
    stop(task, bus).await;
}
