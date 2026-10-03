//! Single-flight bounded backend requests; acknowledgments are not remote freshness.
use crate::{EdsPlugin, details, source_views::Backend, unix_now};
use futures_util::{StreamExt, stream};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
use waft_plugin::StateLocker;
use waft_protocol::entity::calendar::{CalendarSourceState, RefreshOutcome};

pub fn check_debounce(recent: &mut VecDeque<Instant>, base_secs: u64) -> bool {
    let now = Instant::now();
    let base = Duration::from_secs(base_secs.min(u64::MAX / 4));
    recent.retain(|t| now.duration_since(*t) < base * 4);
    if recent
        .iter()
        .filter(|t| now.duration_since(**t) < base)
        .count()
        >= 1
        || recent
            .iter()
            .filter(|t| now.duration_since(**t) < base * 2)
            .count()
            >= 2
        || recent.len() >= 3
    {
        return false;
    }
    recent.push_back(now);
    true
}
struct Job {
    uid: String,
    generation: u64,
    epoch: u64,
    backend: Backend,
}
struct Batch(EdsPlugin);
impl Drop for Batch {
    fn drop(&mut self) {
        let mut st = self.0.state.lock_or_recover();
        if st.sync.syncing {
            st.sync.syncing = false;
            st.sync.refresh_outcome = RefreshOutcome::Cancelled;
            for entry in st.sources.values_mut() {
                entry.refresh_active = false;
            }
        }
        drop(st);
        self.0.notifier.notify();
    }
}
impl EdsPlugin {
    pub(crate) async fn refresh_loop(self) {
        loop {
            self.refresh.notified().await;
            let jobs = {
                let mut st = self.state.lock_or_recover();
                let generation = st.generation;
                let mut jobs = Vec::new();
                for (uid, entry) in &mut st.sources {
                    if entry.refresh_pending
                        && !entry.refresh_active
                        && entry.status.state == CalendarSourceState::Watching
                        && let Some(backend) = &entry.backend
                    {
                        entry.refresh_pending = false;
                        entry.refresh_active = true;
                        jobs.push(Job {
                            uid: uid.clone(),
                            generation,
                            epoch: entry.generation,
                            backend: backend.clone(),
                        });
                    }
                }
                if !jobs.is_empty() {
                    st.sync.syncing = true;
                    st.sync.refresh_outcome = RefreshOutcome::Queued;
                    st.sync.last_refresh = Some(unix_now());
                }
                jobs
            };
            if jobs.is_empty() {
                continue;
            }
            let batch_generation = jobs[0].generation;
            let _batch = Batch(self.clone());
            self.notifier.notify();
            let results = stream::iter(jobs.into_iter().map(|job| {
                let plugin = self.clone();
                async move {
                    let supported = job.backend.capabilities.as_ref().is_none_or(|caps| caps.iter().any(|cap| cap == "refresh-supported"));
                    let outcome = if !supported { RefreshOutcome::Unsupported }
                    else {
                        {
                            let mut st = plugin.state.lock_or_recover();
                            if st.generation == job.generation && let Some(entry) = st.sources.get_mut(&job.uid).filter(|s| s.generation == job.epoch) { entry.status.last_refresh_attempt = Some(unix_now()); }
                            st.last_dispatch = Some(Instant::now());
                        }
                        let call = async { let proxy = zbus::Proxy::new(&job.backend.conn, job.backend.owner.as_str(), job.backend.path.as_str(), "org.gnome.evolution.dataserver.Calendar").await?; proxy.call::<_,_,()>("Refresh", &()).await };
                        let cancelled = async {
                            loop {
                                let notified = plugin.generation_changed.notified(); tokio::pin!(notified); notified.as_mut().enable();
                                let valid = { let st = plugin.state.lock_or_recover(); st.generation == job.generation && st.sources.get(&job.uid).is_some_and(|s| s.generation == job.epoch) };
                                if !valid { break; }
                                notified.await;
                            }
                        };
                        tokio::select! {
                            biased;
                            _ = cancelled => RefreshOutcome::Cancelled,
                            result = tokio::time::timeout(Duration::from_secs(30), call) => if matches!(result, Ok(Ok(()))) { RefreshOutcome::Accepted } else { RefreshOutcome::Failed },
                        }
                    };
                    {
                        let mut st = plugin.state.lock_or_recover();
                        if st.generation == job.generation && let Some(entry) = st.sources.get_mut(&job.uid).filter(|s| s.generation == job.epoch) {
                            entry.refresh_active = false;
                            if outcome == RefreshOutcome::Accepted { entry.status.last_refresh_accepted = Some(unix_now()); if entry.status.error.as_ref().is_some_and(|e| e.code == "eds.refresh-failed") { entry.status.error = None; } }
                            else if outcome == RefreshOutcome::Failed && entry.status.error.as_ref().is_none_or(|e| matches!(e.code.as_str(), "eds.refresh-failed" | "eds.view-failed")) { entry.status.error = Some(details("eds.refresh-failed", "Calendar refresh request failed")); }
                        }
                    }
                    plugin.notifier.notify(); outcome
                }
            })).buffer_unordered(4).collect::<Vec<_>>().await;
            {
                let mut st = self.state.lock_or_recover();
                st.sync.syncing = false;
                st.sync.refresh_outcome = if st.generation == batch_generation {
                    aggregate(&results)
                } else {
                    RefreshOutcome::Cancelled
                };
                st.sync.error = matches!(
                    st.sync.refresh_outcome,
                    RefreshOutcome::Failed | RefreshOutcome::PartialFailure
                )
                .then(|| {
                    details(
                        "eds.refresh-failed",
                        "Some calendar refresh requests failed",
                    )
                });
            }
            self.notifier.notify();
        }
    }
    pub(crate) async fn schedule_refreshes(self) {
        let locked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let changed = Arc::new(tokio::sync::Notify::new());
        let monitor_locked = locked.clone();
        let monitor_changed = changed.clone();
        waft_plugin::spawn_monitored("eds/session", async move {
            monitor_session(monitor_locked, monitor_changed).await;
            Ok(())
        });
        self.schedule_with_lock_state(locked, changed).await;
    }
    async fn schedule_with_lock_state(
        self,
        locked: Arc<std::sync::atomic::AtomicBool>,
        changed: Arc<tokio::sync::Notify>,
    ) {
        let mut previous_lock = false;
        let mut last_deferred = None;
        loop {
            let current_lock = locked.load(std::sync::atomic::Ordering::SeqCst);
            if previous_lock && !current_lock {
                if self.request_refresh(false).is_err() {
                    log_deferred_refresh(&mut last_deferred);
                }
                self.state
                    .lock_or_recover()
                    .debounce
                    .push_back(Instant::now());
            }
            previous_lock = current_lock;
            let seconds = if current_lock {
                self.config.locked_refresh_interval_secs
            } else {
                self.config.refresh_interval_secs
            };
            tokio::select! {
                _ = changed.notified() => {},
                _ = async { if seconds == 0 { std::future::pending::<()>().await; } else { tokio::time::sleep(Duration::from_secs(seconds.min(u64::from(u32::MAX)))).await; } } => {
                    if self.request_refresh(false).is_err() { log_deferred_refresh(&mut last_deferred); }
                }
            }
        }
    }
}
fn log_deferred_refresh(last: &mut Option<Instant>) {
    if last.is_none_or(|last| last.elapsed() >= Duration::from_secs(30)) {
        log::debug!("[eds] refresh deferred by unavailable registry");
        *last = Some(Instant::now());
    }
}
fn aggregate(results: &[RefreshOutcome]) -> RefreshOutcome {
    let accepted = results
        .iter()
        .filter(|r| **r == RefreshOutcome::Accepted)
        .count();
    let failed = results
        .iter()
        .filter(|r| **r == RefreshOutcome::Failed)
        .count();
    if accepted > 0 && failed > 0 {
        RefreshOutcome::PartialFailure
    } else if failed > 0 {
        RefreshOutcome::Failed
    } else if accepted > 0 {
        RefreshOutcome::Accepted
    } else if results.contains(&RefreshOutcome::Cancelled) {
        RefreshOutcome::Cancelled
    } else if results.is_empty() {
        RefreshOutcome::NoBackends
    } else {
        RefreshOutcome::Unsupported
    }
}
async fn monitor_session(
    locked: Arc<std::sync::atomic::AtomicBool>,
    changed: Arc<tokio::sync::Notify>,
) {
    let result = async {
        let conn = zbus::Connection::system().await?;
        let manager = zbus::Proxy::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        let path: zvariant::OwnedObjectPath = if let Ok(id) = std::env::var("XDG_SESSION_ID") {
            manager.call("GetSession", &(id,)).await?
        } else {
            manager
                .call("GetSessionByPID", &(std::process::id(),))
                .await?
        };
        let proxy = zbus::Proxy::new(
            &conn,
            "org.freedesktop.login1",
            path.as_str(),
            "org.freedesktop.login1.Session",
        )
        .await?;
        let mut stream = proxy.receive_property_changed::<bool>("LockedHint").await;
        locked.store(
            proxy.get_property::<bool>("LockedHint").await?,
            std::sync::atomic::Ordering::SeqCst,
        );
        changed.notify_one();
        while let Some(value) = stream.next().await {
            locked.store(value.get().await?, std::sync::atomic::Ordering::SeqCst);
            changed.notify_one();
        }
        Ok::<_, zbus::Error>(())
    }
    .await;
    if result.is_err() {
        log::warn!("[eds] session lock monitoring unavailable; using active refresh interval");
    } else {
        log::warn!("[eds] session lock monitor ended; using active refresh interval");
    }
    locked.store(false, std::sync::atomic::Ordering::SeqCst);
    changed.notify_one();
}
#[cfg(test)]
mod tests {
    async fn scheduling_plugin(config: crate::EdsConfig) -> crate::EdsPlugin {
        let (notifier, _receiver) = waft_plugin::EntityNotifier::new_pair();
        let plugin = crate::EdsPlugin::new(notifier, config);
        plugin.state.lock_or_recover().sync.availability =
            waft_protocol::entity::accounts::Availability::Ready;
        plugin
    }
    #[tokio::test(start_paused = true)]
    async fn disabled_active_interval_sleeps_and_nonzero_locked_interval_executes() {
        let plugin = scheduling_plugin(crate::EdsConfig {
            refresh_interval_secs: 0,
            locked_refresh_interval_secs: 2,
            ..Default::default()
        })
        .await;
        let locked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let changed = Arc::new(tokio::sync::Notify::new());
        let task = tokio::spawn(
            plugin
                .clone()
                .schedule_with_lock_state(locked.clone(), changed.clone()),
        );
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(
            plugin.state.lock_or_recover().sync.refresh_outcome,
            RefreshOutcome::Never
        );
        locked.store(true, std::sync::atomic::Ordering::SeqCst);
        changed.notify_one();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(3)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            plugin.state.lock_or_recover().sync.refresh_outcome,
            RefreshOutcome::NoBackends
        );
        task.abort();
        task.await.expect_err("cancel scheduler");
    }
    #[tokio::test(start_paused = true)]
    async fn zero_intervals_still_merge_an_unlock_request_with_immediate_manual_request() {
        let plugin = scheduling_plugin(crate::EdsConfig {
            refresh_interval_secs: 0,
            locked_refresh_interval_secs: 0,
            ..Default::default()
        })
        .await;
        let locked = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let changed = Arc::new(tokio::sync::Notify::new());
        let task = tokio::spawn(
            plugin
                .clone()
                .schedule_with_lock_state(locked.clone(), changed.clone()),
        );
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(30)).await;
        assert_eq!(
            plugin.state.lock_or_recover().sync.refresh_outcome,
            RefreshOutcome::Never
        );
        locked.store(false, std::sync::atomic::Ordering::SeqCst);
        changed.notify_one();
        tokio::task::yield_now().await;
        assert_eq!(
            plugin.state.lock_or_recover().sync.refresh_outcome,
            RefreshOutcome::NoBackends
        );
        assert_eq!(
            plugin.request_refresh(true).expect("merged manual request")["outcome"],
            "debounced"
        );
        task.abort();
        task.await.expect_err("cancel scheduler");
    }

    use super::*;
    #[test]
    fn debounce_is_explicit_and_overflow_safe() {
        let mut recent = VecDeque::new();
        assert!(check_debounce(&mut recent, 15));
        assert!(!check_debounce(&mut recent, 15));
        assert!(!check_debounce(&mut recent, u64::MAX));
        recent.clear();
        assert!(check_debounce(&mut recent, 0));
        assert!(check_debounce(&mut recent, 0));
    }
    #[test]
    fn refresh_outcomes_are_not_freshness() {
        assert_eq!(aggregate(&[]), RefreshOutcome::NoBackends);
        assert_eq!(
            aggregate(&[RefreshOutcome::Accepted]),
            RefreshOutcome::Accepted
        );
        assert_eq!(aggregate(&[RefreshOutcome::Failed]), RefreshOutcome::Failed);
        assert_eq!(
            aggregate(&[RefreshOutcome::Accepted, RefreshOutcome::Failed]),
            RefreshOutcome::PartialFailure
        );
        assert_eq!(
            aggregate(&[RefreshOutcome::Unsupported]),
            RefreshOutcome::Unsupported
        );
    }
}
