//! Authoritative GOA supervision; account reconciliation and provider probes are independent.
use crate::{dbus, signal_monitor, state::GoaState};
use anyhow::{Context, Result};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use waft_plugin::{EntityNotifier, PluginActionError, StateLocker};
use waft_protocol::{
    entity::accounts::{Availability, OnlineAccountProvider},
    error::{ProtocolError, ProtocolErrorScope},
};
use zbus::Connection;
#[derive(Debug, Clone)]
pub(crate) struct Session {
    pub conn: Connection,
    pub owner: String,
    pub generation: u64,
}
#[derive(Debug)]
struct SnapshotInvalidated;
impl std::fmt::Display for SnapshotInvalidated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("account snapshot invalidated")
    }
}
impl std::error::Error for SnapshotInvalidated {}
#[derive(Clone)]
pub struct GoaLifecycle {
    pub state: Arc<Mutex<GoaState>>,
    pub(crate) wake: Arc<tokio::sync::Notify>,
    pub(crate) changed: Arc<tokio::sync::Notify>,
    pub(crate) notifier: EntityNotifier,
    snapshots: Arc<tokio::sync::Mutex<()>>,
}
type ProviderKey = (u64, BTreeSet<String>);
struct Probe {
    key: ProviderKey,
    task: OwnedTask<Result<Vec<OnlineAccountProvider>>>,
}
struct OwnedTask<T: Send + 'static>(tokio::task::JoinHandle<T>);
impl<T: Send + 'static> Drop for OwnedTask<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl GoaLifecycle {
    pub fn new(notifier: EntityNotifier) -> Self {
        let mut state = GoaState::default();
        state.status.accounts = Availability::Starting;
        state.status.providers = Availability::Starting;
        state.status.launch = waft_protocol::entity::accounts::AccountSettingsLaunch::Idle;
        Self {
            state: Arc::new(Mutex::new(state)),
            wake: Arc::new(tokio::sync::Notify::new()),
            changed: Arc::new(tokio::sync::Notify::new()),
            notifier,
            snapshots: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    pub async fn run(self, address: Option<String>) {
        let mut delay = 1;
        loop {
            let connection = async {
                match &address {
                    Some(address) => Ok(zbus::connection::Builder::address(address.as_str())?
                        .max_queued(256)
                        .build()
                        .await?),
                    None => Ok(zbus::connection::Builder::session()?
                        .max_queued(256)
                        .build()
                        .await?),
                }
            };
            match bounded(connection).await {
                Ok(conn) => {
                    if self.run_connection(conn).await.is_err() {
                        self.state.lock_or_recover().dependency_lost();
                        self.changed.notify_waiters();
                        self.notifier.notify();
                        log::warn!("[goa] connection generation retired");
                    }
                    delay = 1;
                }
                Err(error) => self.fail(&error, true),
            }
            tokio::time::sleep(Duration::from_secs(delay)).await;
            delay = (delay * 2).min(30);
        }
    }
    async fn run_connection(&self, conn: Connection) -> Result<()> {
        let streams = bounded(signal_monitor::subscribe(&conn)).await?;
        let reader = OwnedTask(tokio::spawn(signal_monitor::read_signals(
            streams,
            self.state.clone(),
            self.wake.clone(),
            self.changed.clone(),
            self.notifier.clone(),
        )));
        let mut generation = self.state.lock_or_recover().generation;
        let mut account_retry = None;
        let mut account_delay = 1;
        let mut burst = 0;
        let mut burst_deadline = None;
        let mut observed_providers: Option<ProviderKey> = None;
        let mut checked_providers: Option<ProviderKey> = None;
        let mut provider_retry = None;
        let mut provider_delay = 1;
        let mut probe: Option<Probe> = None;
        loop {
            if reader.0.is_finished() {
                anyhow::bail!("signal stream disconnected");
            }
            let current_generation = self.state.lock_or_recover().generation;
            if current_generation != generation {
                generation = current_generation;
                account_retry = None;
                account_delay = 1;
                burst = 0;
                burst_deadline = None;
                probe = None;
                observed_providers = None;
                checked_providers = None;
                provider_retry = None;
                provider_delay = 1;
            }
            let due = account_retry.is_none_or(|deadline| tokio::time::Instant::now() >= deadline);
            let (dirty, unsupported) = {
                let st = self.state.lock_or_recover();
                (
                    st.session.is_none()
                        || st.revision != st.committed_revision
                        || st.status.accounts != Availability::Ready,
                    st.status.accounts == Availability::Unsupported,
                )
            };
            if due && dirty && !unsupported {
                let session = self.state.lock_or_recover().session.clone();
                let session = match session {
                    Some(session) => Ok(session),
                    None => self.establish(&conn).await,
                };
                let result = match session {
                    Ok(session) => {
                        if self.state.lock_or_recover().generation == session.generation {
                            generation = session.generation;
                        }
                        let deadline = *burst_deadline.get_or_insert_with(|| {
                            tokio::time::Instant::now() + Duration::from_secs(8)
                        });
                        tokio::time::timeout_at(deadline, self.snapshot(&session))
                            .await
                            .context("coherent snapshot burst timeout")
                            .and_then(|r| r)
                            .map(|_| ())
                    }
                    Err(error) => Err(error),
                };
                match result {
                    Ok(()) => {
                        account_retry = None;
                        account_delay = 1;
                        burst = 0;
                        burst_deadline = None;
                        let st = self.state.lock_or_recover();
                        if st.status.accounts == Availability::Ready && st.session.is_some() {
                            generation = st.generation;
                        }
                    }
                    Err(error) => {
                        self.fail(&error, true);
                        burst += 1;
                        let exhausted = burst >= 4
                            || burst_deadline.is_some_and(|d| tokio::time::Instant::now() >= d);
                        if error.downcast_ref::<SnapshotInvalidated>().is_some() && !exhausted {
                            self.coalesce(generation).await;
                            continue;
                        }
                        if !unknown_method(&error) {
                            account_retry = Some(
                                tokio::time::Instant::now() + Duration::from_secs(account_delay),
                            );
                            account_delay = (account_delay * 2).min(30);
                        }
                        burst = 0;
                        burst_deadline = None;
                    }
                }
            }
            let provider_input = {
                let st = self.state.lock_or_recover();
                st.session.clone().map(|session| {
                    let accounts: dbus::AccountSnapshot = st
                        .accounts
                        .iter()
                        .filter_map(|(id, a)| {
                            st.paths.get(id).map(|p| (id.clone(), p.clone(), a.clone()))
                        })
                        .collect();
                    (
                        (session.generation, provider_set(&accounts)),
                        session,
                        accounts,
                    )
                })
            };
            if let Some((key, session, accounts)) = provider_input {
                if observed_providers.as_ref() != Some(&key) {
                    probe = None;
                    observed_providers = Some(key.clone());
                    checked_providers = None;
                    provider_retry = None;
                    provider_delay = 1;
                }
                let needs_probe = {
                    let st = self.state.lock_or_recover();
                    checked_providers.as_ref() != Some(&key)
                        || !matches!(
                            st.status.providers,
                            Availability::Ready | Availability::Unsupported
                        )
                };
                if needs_probe
                    && probe.is_none()
                    && provider_retry.is_none_or(|d| tokio::time::Instant::now() >= d)
                {
                    probe = Some(Probe {
                        key,
                        task: OwnedTask(tokio::spawn(async move {
                            dbus::discover_providers(&session.conn, &session.owner, &accounts).await
                        })),
                    });
                    provider_retry = None;
                }
            }
            tokio::select! {
                _ = self.wake.notified() => { self.coalesce(generation).await; }
                _ = sleep_until(account_retry), if account_retry.is_some() => { account_retry = None; }
                _ = sleep_until(provider_retry), if provider_retry.is_some() => { provider_retry = None; }
                result = async { if let Some(probe) = &mut probe { (&mut probe.task.0).await } else { std::future::pending().await } }, if probe.is_some() => {
                    let Some(finished) = probe.take() else { continue; };
                    let current = { let st = self.state.lock_or_recover(); st.session.as_ref().is_some_and(|s| s.generation == finished.key.0) && provider_set_from_state(&st) == finished.key.1 };
                    if !current { continue; }
                    match result {
                        Ok(Ok(providers)) => {
                            let mut st = self.state.lock_or_recover();
                            let supported: BTreeSet<_> = providers.iter().map(|p| p.provider_type.clone()).collect();
                            st.unsupported_providers = provider_set_from_state(&st).difference(&supported).cloned().collect();
                            let previous = st.capabilities.len(); st.capabilities.retain(|_,(provider,_)| supported.contains(provider));
                            if st.capabilities.len() != previous { st.revision += 1; self.wake.notify_one(); }
                            st.providers = providers; st.status.providers = Availability::Ready; st.status.providers_error = None; st.status.last_providers_snapshot = Some(unix_now());
                            checked_providers = Some(finished.key); provider_delay = 1;
                        }
                        error => {
                            let error = match error { Ok(Err(error)) => error, Err(_) => anyhow::anyhow!("provider probe task failed"), Ok(Ok(_)) => continue };
                            self.fail(&error, false); checked_providers = Some(finished.key);
                            if !unknown_method(&error) { provider_retry = Some(tokio::time::Instant::now() + Duration::from_secs(provider_delay)); provider_delay = (provider_delay * 2).min(30); }
                        }
                    }
                    self.notifier.notify();
                }
            }
        }
    }
    async fn coalesce(&self, generation: u64) {
        let limit = tokio::time::Instant::now() + Duration::from_millis(250);
        loop {
            if self.state.lock_or_recover().generation != generation {
                break;
            }
            tokio::select! {
                _ = self.wake.notified() => {},
                _ = tokio::time::sleep(Duration::from_millis(50)) => break,
                _ = tokio::time::sleep_until(limit) => break,
            }
        }
    }
    async fn establish(&self, conn: &Connection) -> Result<Session> {
        let generation = self.state.lock_or_recover().generation;
        let lookup = || {
            bounded(async {
                let bus = zbus::fdo::DBusProxy::new(conn).await?;
                Ok(bus
                    .get_name_owner(dbus::GOA_BUS_NAME.try_into()?)
                    .await?
                    .to_string())
            })
        };
        let owner = match lookup().await {
            Ok(owner) => owner,
            Err(_) => {
                bounded(async {
                    zbus::fdo::DBusProxy::new(conn)
                        .await?
                        .start_service_by_name(dbus::GOA_BUS_NAME.try_into()?, 0)
                        .await?;
                    Ok(())
                })
                .await?;
                lookup().await?
            }
        };
        let mut st = self.state.lock_or_recover();
        anyhow::ensure!(
            st.generation == generation || st.announced_owner.as_deref() == Some(owner.as_str()),
            "owner lookup invalidated"
        );
        st.generation += 1;
        let session = Session {
            conn: conn.clone(),
            owner,
            generation: st.generation,
        };
        st.session = Some(session.clone());
        Ok(session)
    }
    pub(crate) async fn snapshot(&self, session: &Session) -> Result<dbus::AccountSnapshot> {
        let _permit = self.snapshots.lock().await;
        let (revision, mut capabilities) = {
            let st = self.state.lock_or_recover();
            (st.revision, st.capabilities.clone())
        };
        let objects = bounded(dbus::managed_objects(&session.conn, &session.owner)).await?;
        let identities = dbus::snapshot_identities(&objects);
        let accounts = dbus::parse_snapshot(objects, &mut capabilities)?;
        {
            let mut st = self.state.lock_or_recover();
            if !st
                .session
                .as_ref()
                .is_some_and(|s| s.generation == session.generation)
                || st.revision != revision
            {
                return Err(SnapshotInvalidated.into());
            }
            capabilities.retain(|_, (provider, _)| !st.unsupported_providers.contains(provider));
            st.replace_accounts(accounts.clone(), capabilities, identities);
            st.committed_revision = revision;
            st.status.accounts = Availability::Ready;
            st.status.accounts_error = None;
            st.status.last_accounts_snapshot = Some(unix_now());
        }
        self.changed.notify_waiters();
        self.notifier.notify();
        Ok(accounts)
    }
    /// Read-only retries are safe; a mutation is never repeated.
    pub(crate) async fn coherent_snapshot(
        &self,
        session: &Session,
    ) -> Result<dbus::AccountSnapshot> {
        for _ in 0..4 {
            match self.snapshot(session).await {
                Err(error) if error.downcast_ref::<SnapshotInvalidated>().is_some() => {
                    tokio::task::yield_now().await;
                }
                result => return result,
            }
        }
        Err(SnapshotInvalidated.into())
    }
    pub(crate) fn invalidate(&self) {
        self.state.lock_or_recover().revision += 1;
        self.wake.notify_one();
    }
    fn fail(&self, error: &anyhow::Error, accounts: bool) {
        let unsupported = unknown_method(error);
        let details = ProtocolError::new(
            if unsupported {
                "goa.unsupported-api"
            } else {
                "goa.unavailable"
            },
            if unsupported {
                "Required account API is unavailable"
            } else {
                "Account discovery is unavailable; recovery scheduled"
            },
            if unsupported {
                ProtocolErrorScope::Capability
            } else {
                ProtocolErrorScope::Transport
            },
            !unsupported,
        );
        let mut st = self.state.lock_or_recover();
        let status = if unsupported {
            Availability::Unsupported
        } else if (accounts && st.status.last_accounts_snapshot.is_some())
            || (!accounts && st.status.last_providers_snapshot.is_some())
        {
            Availability::Recovering
        } else {
            Availability::Unavailable
        };
        let old = if accounts {
            st.status.accounts
        } else {
            st.status.providers
        };
        if accounts {
            st.status.accounts = status;
            st.status.accounts_error = Some(details);
        } else {
            st.status.providers = status;
            st.status.providers_error = Some(details);
        }
        drop(st);
        if old != status {
            log::warn!("[goa] discovery state: {status:?} (accounts={accounts})");
        }
        self.notifier.notify();
    }
}
async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    } else {
        std::future::pending::<()>().await;
    }
}
pub(crate) async fn bounded<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(2), future)
        .await
        .context("account operation timeout")?
}
fn unknown_method(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<dbus::UnsupportedAccountApi>()
        .is_some()
        || matches!(error.downcast_ref::<zbus::Error>(), Some(zbus::Error::MethodError(name, _, _)) if matches!(name.as_str(), "org.freedesktop.DBus.Error.UnknownMethod" | "org.freedesktop.DBus.Error.UnknownInterface"))
}
pub(crate) fn failure(code: &str, message: &str) -> anyhow::Error {
    let scope = match code {
        "goa.unavailable" => ProtocolErrorScope::Transport,
        "goa.unsupported-api" | "goa.service-unsupported" => ProtocolErrorScope::Capability,
        "entity.not-found" => ProtocolErrorScope::NotFound,
        "protocol.validation" => ProtocolErrorScope::Validation,
        "action.timeout" => ProtocolErrorScope::Timeout,
        _ => ProtocolErrorScope::Action,
    };
    PluginActionError(ProtocolError::new(
        code,
        message,
        scope,
        code == "goa.unavailable",
    ))
    .into()
}
pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn provider_set(accounts: &dbus::AccountSnapshot) -> BTreeSet<String> {
    accounts
        .iter()
        .map(|(_, _, a)| a.provider_type.clone())
        .collect()
}
fn provider_set_from_state(st: &GoaState) -> BTreeSet<String> {
    st.accounts
        .values()
        .map(|a| a.provider_type.clone())
        .collect()
}
