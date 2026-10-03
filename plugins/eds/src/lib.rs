//! EDS integration: source-local snapshots, supervised lifetimes, bounded refresh dispatch.
pub mod ical;
pub mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
pub mod refresh_scheduler;
pub mod source_registry;
pub mod source_views;

use serde::Deserialize;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use waft_plugin::{
    Entity, EntityNotifier, Plugin, PluginActionError, StateLocker, Urn, serde_json,
};
use waft_protocol::{
    entity::{accounts::Availability, calendar::*},
    error::{ProtocolError, ProtocolErrorScope},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EdsConfig {
    pub refresh_interval_secs: u64,
    pub locked_refresh_interval_secs: u64,
    pub debounce_base_secs: u64,
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
pub(crate) struct SourceEntry {
    pub source: source_registry::CalendarSource,
    pub status: CalendarSourceStatus,
    pub events: HashMap<String, CalendarEvent>,
    pub generation: u64,
    pub backend: Option<source_views::Backend>,
    pub backend_owner: Option<String>,
    pub wake: Arc<tokio::sync::Notify>,
    pub refresh_pending: bool,
    pub setup: bool,
    pub refresh_active: bool,
}
pub(crate) struct EdsState {
    pub sources: HashMap<String, SourceEntry>,
    pub sync: CalendarSync,
    pub generation: u64,
    pub revision: u64,
    pub debounce: VecDeque<Instant>,
    pub last_dispatch: Option<Instant>,
    pub backend_leases:
        HashMap<(String, String, String, String), std::sync::Weak<source_views::BackendLease>>,
}
#[derive(Clone)]
pub struct EdsPlugin {
    pub(crate) state: Arc<Mutex<EdsState>>,
    pub(crate) notifier: EntityNotifier,
    pub(crate) refresh: Arc<tokio::sync::Notify>,
    pub(crate) registry_wake: Arc<tokio::sync::Notify>,
    pub(crate) config: EdsConfig,
    pub(crate) setup_permits: Arc<tokio::sync::Semaphore>,
    pub(crate) generation_changed: Arc<tokio::sync::Notify>,
}
impl EdsPlugin {
    pub fn new(notifier: EntityNotifier, config: EdsConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(EdsState {
                sources: HashMap::new(),
                sync: CalendarSync {
                    availability: Availability::Starting,
                    refresh_outcome: RefreshOutcome::Never,
                    ..Default::default()
                },
                generation: 0,
                revision: 0,
                debounce: VecDeque::new(),
                last_dispatch: None,
                backend_leases: HashMap::new(),
            })),
            notifier,
            refresh: Arc::new(tokio::sync::Notify::new()),
            registry_wake: Arc::new(tokio::sync::Notify::new()),
            config,
            setup_permits: Arc::new(tokio::sync::Semaphore::new(4)),
            generation_changed: Arc::new(tokio::sync::Notify::new()),
        }
    }
    pub fn start(&self) {
        let supervisor = self.clone();
        waft_plugin::spawn_monitored("eds/lifecycle", async move {
            supervisor.supervise(None).await;
            Ok(())
        });
        let scheduler = self.clone();
        waft_plugin::spawn_monitored("eds/refresh", async move {
            scheduler.refresh_loop().await;
            Ok(())
        });
        let scheduler = self.clone();
        waft_plugin::spawn_monitored("eds/schedule", async move {
            scheduler.schedule_refreshes().await;
            Ok(())
        });
    }
    pub(crate) fn request_refresh(&self, debounced: bool) -> anyhow::Result<serde_json::Value> {
        let result;
        {
            let mut st = self.state.lock_or_recover();
            if matches!(
                st.sync.availability,
                Availability::Unavailable | Availability::Recovering | Availability::Unsupported
            ) {
                return Err(failure(
                    if st.sync.availability == Availability::Unsupported {
                        "eds.unsupported-api"
                    } else {
                        "eds.unavailable"
                    },
                    "Calendar source discovery is unavailable",
                ));
            }
            if debounced
                && !refresh_scheduler::check_debounce(
                    &mut st.debounce,
                    self.config.debounce_base_secs,
                )
            {
                return Ok(serde_json::json!({"outcome":"debounced"}));
            }
            let eligible = st
                .sources
                .values_mut()
                .filter(|s| {
                    !matches!(
                        s.status.state,
                        CalendarSourceState::Disabled | CalendarSourceState::Unsupported
                    )
                })
                .fold(0, |n, s| {
                    if !s.refresh_active {
                        s.refresh_pending = true;
                    }
                    n + 1
                });
            if eligible == 0 && st.sync.availability == Availability::Ready {
                st.sync.refresh_outcome = RefreshOutcome::NoBackends;
                result = "no-backends";
            } else {
                st.sync.refresh_outcome = RefreshOutcome::Queued;
                st.sync.last_refresh = Some(unix_now());
                result = "queued";
            }
        }
        self.refresh.notify_one();
        self.notifier.notify();
        Ok(serde_json::json!({"outcome":result}))
    }
}
#[async_trait::async_trait]
impl Plugin for EdsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let st = self.state.lock_or_recover();
        let mut entities = vec![Entity::new(
            Urn::new("eds", CALENDAR_SYNC_ENTITY_TYPE, "singleton"),
            CALENDAR_SYNC_ENTITY_TYPE,
            &st.sync,
        )];
        for source in st.sources.values() {
            entities.push(Entity::new(
                Urn::new(
                    "eds",
                    CALENDAR_SOURCE_STATUS_ENTITY_TYPE,
                    &source_registry::encode_identifier(&source.source.uid),
                ),
                CALENDAR_SOURCE_STATUS_ENTITY_TYPE,
                &source.status,
            ));
            for (id, event) in &source.events {
                entities.push(Entity::new(
                    Urn::new("eds", ENTITY_TYPE, id),
                    ENTITY_TYPE,
                    event,
                ));
            }
        }
        entities.sort_by_key(|e| e.urn.to_string());
        entities
    }
    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if urn != Urn::new("eds", CALENDAR_SYNC_ENTITY_TYPE, "singleton") || action != "refresh" {
            return Err(failure(
                "protocol.validation",
                "Unsupported calendar action",
            ));
        }
        self.request_refresh(true)
    }
    fn can_stop(&self) -> bool {
        let st = self.state.lock_or_recover();
        !st.sync.syncing
            && !st.sources.values().any(|s| {
                s.setup
                    || matches!(
                        s.status.state,
                        CalendarSourceState::Opening | CalendarSourceState::Loading
                    )
            })
            && st
                .last_dispatch
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(120))
    }
}
pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub(crate) fn details(code: &str, message: &str) -> ProtocolError {
    let scope = match code {
        "eds.unavailable" => ProtocolErrorScope::Transport,
        "eds.unsupported-api" => ProtocolErrorScope::Capability,
        "eds.source-invalid" | "protocol.validation" => ProtocolErrorScope::Validation,
        _ => ProtocolErrorScope::Action,
    };
    ProtocolError::new(
        code,
        message,
        scope,
        matches!(
            code,
            "eds.unavailable" | "eds.view-failed" | "eds.offline" | "eds.refresh-failed"
        ),
    )
}
fn failure(code: &str, message: &str) -> anyhow::Error {
    PluginActionError(details(code, message)).into()
}
