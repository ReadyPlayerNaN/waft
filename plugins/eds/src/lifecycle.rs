//! Event-driven EDS registry/owner supervision; failed discovery never deletes cached events.
use crate::{
    EdsPlugin, SourceEntry, details,
    source_registry::{self, Eligibility},
    unix_now,
};
use anyhow::{Context, Result};
use futures_util::{StreamExt, stream::SelectAll};
use std::{collections::HashMap, time::Duration};
use waft_plugin::StateLocker;
use waft_protocol::entity::{
    accounts::Availability,
    calendar::{CalendarSourceState, CalendarSourceStatus},
};
use zbus::{Connection, MatchRule, MessageStream};
impl EdsPlugin {
    pub async fn supervise(self, address: Option<String>) {
        let mut retry = 1;
        loop {
            let connect = async {
                let builder = if let Some(address) = &address {
                    zbus::connection::Builder::address(address.as_str())?
                } else {
                    zbus::connection::Builder::session()?
                };
                Ok(builder.max_queued(256).build().await?)
            };
            match bounded(connect).await {
                Ok(conn) => {
                    if self.registry_connection(conn).await.is_err() {
                        self.registry_failure(false);
                    }
                }
                Err(_) => self.registry_failure(false),
            }
            tokio::time::sleep(Duration::from_secs(retry)).await;
            retry = (retry * 2).min(30);
        }
    }
    async fn registry_connection(&self, conn: Connection) -> Result<()> {
        let owners = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .build();
        let sources = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(source_registry::SOURCES_DEST)?
            .path_namespace("/org/gnome/evolution/dataserver")?
            .interface("org.freedesktop.DBus.ObjectManager")?
            .build();
        let properties = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(source_registry::SOURCES_DEST)?
            .path_namespace("/org/gnome/evolution/dataserver")?
            .interface("org.freedesktop.DBus.Properties")?
            .build();
        let streams = bounded(async {
            Ok(vec![
                MessageStream::for_match_rule(owners, &conn, Some(256)).await?,
                MessageStream::for_match_rule(sources, &conn, Some(256)).await?,
                MessageStream::for_match_rule(properties, &conn, Some(256)).await?,
            ])
        })
        .await?;
        let reader = self.clone();
        let reader = Reader(tokio::spawn(async move {
            reader.registry_signals(streams).await;
        }));
        let mut workers = tokio::task::JoinSet::new();
        let mut tasks: HashMap<String, tokio::task::AbortHandle> = HashMap::new();
        let mut worker_generation = self.state.lock_or_recover().generation;
        let mut retry = 1;
        loop {
            if reader.0.is_finished() {
                anyhow::bail!("registry stream disconnected");
            }
            let (generation, revision) = {
                let st = self.state.lock_or_recover();
                (st.generation, st.revision)
            };
            if generation != worker_generation {
                workers.abort_all();
                tasks.clear();
                worker_generation = generation;
            }
            let snapshot = async {
                let owner = resolve_owner(&conn, source_registry::SOURCES_DEST).await?;
                let sources = bounded(source_registry::discover(&conn, &owner)).await?;
                Ok::<_, anyhow::Error>((owner, sources))
            }
            .await;
            let success = match snapshot {
                Ok((owner, sources)) => {
                    let known: std::collections::HashSet<_> = self
                        .state
                        .lock_or_recover()
                        .sources
                        .keys()
                        .cloned()
                        .collect();
                    let eligible: HashMap<_, _> = sources
                        .values()
                        .filter(|s| s.calendar || (!s.valid && known.contains(&s.uid)))
                        .map(|s| {
                            (
                                s.uid.clone(),
                                source_registry::eligibility(&s.uid, &sources),
                            )
                        })
                        .collect();
                    let mut launch = Vec::new();
                    let committed = {
                        let mut st = self.state.lock_or_recover();
                        if st.generation != generation || st.revision != revision {
                            false
                        } else {
                            st.sources.retain(|uid, _| eligible.contains_key(uid));
                            tasks.retain(|uid, task| {
                                if !eligible.contains_key(uid) {
                                    task.abort();
                                    false
                                } else {
                                    true
                                }
                            });
                            for (uid, eligibility) in eligible {
                                let source = sources
                                    .get(&uid)
                                    .expect("eligibility source exists")
                                    .clone();
                                let changed =
                                    st.sources.get(&uid).is_none_or(|s| s.source != source);
                                if changed && let Some(task) = tasks.remove(&uid) {
                                    task.abort();
                                }
                                let entry =
                                    st.sources
                                        .entry(uid.clone())
                                        .or_insert_with(|| SourceEntry {
                                            source: source.clone(),
                                            status: CalendarSourceStatus {
                                                source_uid: uid.clone(),
                                                display_name: source.display_name.clone(),
                                                ..Default::default()
                                            },
                                            events: HashMap::new(),
                                            generation: 0,
                                            backend: None,
                                            backend_owner: None,
                                            wake: std::sync::Arc::new(tokio::sync::Notify::new()),
                                            refresh_pending: true,
                                            refresh_active: false,
                                            setup: false,
                                        });
                                entry.source = source.clone();
                                entry.status.display_name = source.display_name.clone();
                                entry.status.goa_account_id =
                                    source_registry::goa_account(&uid, &sources);
                                match eligibility {
                                    Eligibility::Enabled => {
                                        if !tasks.contains_key(&uid) {
                                            entry.generation += 1;
                                            entry.backend = None;
                                            entry.refresh_active = false;
                                            entry.refresh_pending = true;
                                            entry.setup = true;
                                            entry.status.state = CalendarSourceState::Pending;
                                            launch.push((
                                                source,
                                                entry.generation,
                                                entry.wake.clone(),
                                            ));
                                        }
                                    }
                                    other => {
                                        if let Some(task) = tasks.remove(&uid) {
                                            task.abort();
                                        }
                                        entry.generation += 1;
                                        entry.backend = None;
                                        entry.backend_owner = None;
                                        entry.refresh_active = false;
                                        entry.setup = false;
                                        entry.status.state = match other {
                                            Eligibility::Disabled => CalendarSourceState::Disabled,
                                            Eligibility::Pending => CalendarSourceState::Pending,
                                            _ => CalendarSourceState::Unsupported,
                                        };
                                        if other == Eligibility::Disabled {
                                            entry.events.clear();
                                        }
                                        entry.status.error =
                                            (other == Eligibility::Invalid).then(|| {
                                                details(
                                                    "eds.source-invalid",
                                                    "Invalid source ancestry or source data",
                                                )
                                            });
                                    }
                                }
                            }
                            if st.sync.refresh_outcome
                                == waft_protocol::entity::calendar::RefreshOutcome::Queued
                                && !st.sync.syncing
                                && st.sources.values().all(|s| {
                                    matches!(
                                        s.status.state,
                                        CalendarSourceState::Disabled
                                            | CalendarSourceState::Unsupported
                                    )
                                })
                            {
                                st.sync.refresh_outcome =
                                    waft_protocol::entity::calendar::RefreshOutcome::NoBackends;
                            }
                            st.sync.availability = Availability::Ready;
                            st.sync.error = None;
                            st.sync.last_snapshot = Some(unix_now());
                            true
                        }
                    };
                    for (source, epoch, wake) in launch {
                        let plugin = self.clone();
                        let conn = conn.clone();
                        let source_owner = owner.clone();
                        let uid = source.uid.clone();
                        let worker_uid = uid.clone();
                        let task = workers.spawn(async move {
                            plugin
                                .watch_source(conn, source_owner, source, generation, epoch, wake)
                                .await;
                            worker_uid
                        });
                        tasks.insert(uid, task);
                    }
                    if committed {
                        self.generation_changed.notify_waiters();
                    }
                    self.notifier.notify();
                    self.refresh.notify_one();
                    committed
                }
                Err(error) => {
                    self.registry_failure(unknown_method(&error));
                    false
                }
            };
            if success {
                retry = 1;
            }
            let unsupported =
                self.state.lock_or_recover().sync.availability == Availability::Unsupported;
            tokio::select! {
                _ = self.registry_wake.notified() => { tokio::time::sleep(Duration::from_millis(50)).await; }
                _ = tokio::time::sleep(Duration::from_secs(retry)), if !success && !unsupported => { retry = (retry * 2).min(30); }
                Some(result) = workers.join_next(), if !workers.is_empty() => {
                    if result.is_err_and(|e| !e.is_cancelled()) { log::warn!("[eds] source monitor exited unexpectedly; reconciliation scheduled"); }
                    tasks.retain(|_, task| !task.is_finished());
                    self.registry_wake.notify_one();
                }
            }
        }
    }
    async fn registry_signals(&self, streams: Vec<MessageStream>) {
        let mut streams: SelectAll<_> = streams.into_iter().collect();
        while let Some(result) = streams.next().await {
            let Ok(message) = result else {
                break;
            };
            let header = message.header();
            let owner_change = if header
                .sender()
                .is_some_and(|s| s.as_str() == "org.freedesktop.DBus")
                && header
                    .member()
                    .is_some_and(|m| m.as_str() == "NameOwnerChanged")
            {
                message
                    .body()
                    .deserialize::<(String, String, String)>()
                    .ok()
                    .is_some_and(|(name, old, _new)| {
                        name == source_registry::SOURCES_DEST
                            || name == source_registry::FACTORY_DEST
                            || self.state.lock_or_recover().sources.values().any(|s| {
                                s.backend_owner.as_ref().is_some_and(|owner| owner == &old)
                            })
                    })
            } else {
                false
            };
            if owner_change {
                let mut st = self.state.lock_or_recover();
                st.generation += 1;
                st.sync.availability = if st.sync.last_snapshot.is_some() {
                    Availability::Recovering
                } else {
                    Availability::Unavailable
                };
                for source in st.sources.values_mut() {
                    source.backend = None;
                    source.status.state = CalendarSourceState::Unavailable;
                    source.refresh_active = false;
                    source.setup = false;
                }
                drop(st);
                self.generation_changed.notify_waiters();
                self.registry_wake.notify_one();
                self.notifier.notify();
            } else if header.interface().is_some_and(|i| {
                matches!(
                    i.as_str(),
                    "org.freedesktop.DBus.ObjectManager" | "org.freedesktop.DBus.Properties"
                )
            }) {
                // The subscribed sender rule is supplemented by an owner-pinned lookup in discovery.
                self.state.lock_or_recover().revision += 1;
                self.registry_wake.notify_one();
            }
        }
        self.state.lock_or_recover().generation += 1;
        self.generation_changed.notify_waiters();
        self.registry_failure(false);
        self.registry_wake.notify_one();
        log::warn!("[eds] registry signal reader ended");
    }
    fn registry_failure(&self, unsupported: bool) {
        let mut st = self.state.lock_or_recover();
        st.sync.availability = if unsupported {
            Availability::Unsupported
        } else if st.sync.last_snapshot.is_some() {
            Availability::Recovering
        } else {
            Availability::Unavailable
        };
        st.sync.error = Some(details(
            if unsupported {
                "eds.unsupported-api"
            } else {
                "eds.unavailable"
            },
            "Calendar source discovery is unavailable",
        ));
        drop(st);
        self.notifier.notify();
    }
}
struct Reader(tokio::task::JoinHandle<()>);
impl Drop for Reader {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub(crate) async fn bounded<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(2), future)
        .await
        .context("EDS operation timeout")?
}
pub(crate) async fn resolve_owner(conn: &Connection, name: &str) -> Result<String> {
    let lookup = || {
        bounded(async {
            let bus = zbus::fdo::DBusProxy::new(conn).await?;
            Ok(bus.get_name_owner(name.try_into()?).await?.to_string())
        })
    };
    match lookup().await {
        Ok(owner) => Ok(owner),
        Err(_) => {
            bounded(async {
                zbus::fdo::DBusProxy::new(conn)
                    .await?
                    .start_service_by_name(name.try_into()?, 0)
                    .await?;
                Ok(())
            })
            .await?;
            lookup().await
        }
    }
}
pub(crate) fn unknown_method(error: &anyhow::Error) -> bool {
    matches!(error.downcast_ref::<zbus::Error>(), Some(zbus::Error::MethodError(name,_,_)) if matches!(name.as_str(), "org.freedesktop.DBus.Error.UnknownMethod" | "org.freedesktop.DBus.Error.UnknownInterface"))
}
