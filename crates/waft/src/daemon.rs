use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use log::{debug, error, info, warn};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use uuid::Uuid;
use waft_protocol::urn::Urn;
use waft_protocol::{AppMessage, AppNotification, PluginCommand, PluginMessage};

use waft_protocol::entity::plugin::{self as plugin_entity, PluginState, PluginStatus};

use crate::action_tracker::ActionTracker;
use crate::claim_tracker::{ClaimResolution, ClaimTracker};
use crate::connection::{ClientKind, Connection, ConnectionError, ReadHalf};
use crate::crash_tracker::{CrashOutcome, CrashTracker};
use crate::plugin_discovery::PluginDiscoveryCache;
use crate::plugin_spawner::PluginSpawner;
use crate::registry::{AppRegistry, PluginRegistry};

/// Cached entity data: (urn, entity_type, data).
struct CachedEntity {
    urn: Urn,
    entity_type: String,
    data: serde_json::Value,
}

/// Event from a connection reader.
enum Event {
    /// A message was received from a connection.
    Message(Uuid, Vec<u8>),
    /// A connection disconnected or errored.
    Disconnected(Uuid, Option<ConnectionError>),
}

/// Pending CanStop request awaiting a plugin response.
struct PendingCanStop {
    plugin_conn_id: Uuid,
    retry_at: tokio::time::Instant,
}

/// The waft daemon: accepts connections, identifies clients, routes messages.
pub struct WaftDaemon {
    listener: UnixListener,
    connections: HashMap<Uuid, Connection>,
    plugin_registry: PluginRegistry,
    app_registry: AppRegistry,
    action_tracker: ActionTracker,
    entity_cache: HashMap<String, CachedEntity>,
    /// Pending CanStop retries: plugin_name -> retry info.
    pending_can_stops: HashMap<String, PendingCanStop>,
    claim_tracker: ClaimTracker,
    plugin_spawner: PluginSpawner,
    crash_tracker: CrashTracker,
    /// Plugins currently in a graceful CanStop shutdown (not a crash).
    graceful_stops: HashSet<String>,
    /// Plugins that were stopped gracefully and remain stopped until respawned.
    stopped_plugins: HashSet<String>,
    event_rx: mpsc::Receiver<Event>,
    event_tx: mpsc::Sender<Event>,
}

impl WaftDaemon {
    pub fn new(socket_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = UnixListener::bind(&socket_path)?;
        let (event_tx, event_rx) = mpsc::channel(256);

        // Build the discovery cache on a blocking thread so we don't block the
        // tokio runtime during process spawning and I/O.
        let discovery_cache = PluginDiscoveryCache::build();
        let plugin_spawner = PluginSpawner::new(discovery_cache);

        Ok(WaftDaemon {
            listener,
            connections: HashMap::new(),
            plugin_registry: PluginRegistry::new(),
            app_registry: AppRegistry::new(),
            action_tracker: ActionTracker::new(),
            entity_cache: HashMap::new(),
            pending_can_stops: HashMap::new(),
            claim_tracker: ClaimTracker::new(),
            plugin_spawner,
            crash_tracker: CrashTracker::new(),
            graceful_stops: HashSet::new(),
            stopped_plugins: HashSet::new(),
            event_rx,
            event_tx,
        })
    }

    /// Compute the next wakeup time: the earliest of action timeout, CanStop retry, or claim deadline.
    fn next_wakeup(&self) -> Option<tokio::time::Instant> {
        let action_deadline = self
            .action_tracker
            .next_deadline()
            .map(tokio::time::Instant::from_std);

        let can_stop_deadline = self.pending_can_stops.values().map(|p| p.retry_at).min();

        let claim_deadline = self
            .claim_tracker
            .next_deadline()
            .map(tokio::time::Instant::from_std);

        [action_deadline, can_stop_deadline, claim_deadline]
            .into_iter()
            .flatten()
            .min()
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            // Sleep until the next deadline (action timeout or CanStop retry),
            // or wait indefinitely if nothing is pending.
            let wakeup = self.next_wakeup();
            let timeout_sleep = match wakeup {
                Some(deadline) => tokio::time::sleep_until(deadline),
                None => tokio::time::sleep_until(
                    tokio::time::Instant::now() + std::time::Duration::from_secs(86400),
                ),
            };
            tokio::pin!(timeout_sleep);

            tokio::select! {
                accept = self.listener.accept() => {
                    match accept {
                        Ok((stream, _)) => {
                            let (conn, read_half) = Connection::new(stream);
                            let conn_id = conn.id;
                            debug!("new connection: {conn_id}");
                            self.connections.insert(conn_id, conn);

                            // Spawn read loop for this connection
                            let tx = self.event_tx.clone();
                            tokio::spawn(Self::read_loop(read_half, tx));
                        }
                        Err(e) => {
                            error!("accept error: {e}");
                        }
                    }
                }

                Some(event) = self.event_rx.recv() => {
                    match event {
                        Event::Message(conn_id, bytes) => {
                            if let Err(e) = self.handle_message(conn_id, &bytes).await {
                                warn!("error handling message from {conn_id}: {e}");
                                self.remove_connection(conn_id).await;
                            }
                        }
                        Event::Disconnected(conn_id, err) => {
                            if let Some(e) = err {
                                debug!("connection {conn_id} error: {e}");
                            } else {
                                debug!("connection {conn_id} disconnected");
                            }
                            self.remove_connection(conn_id).await;
                        }
                    }
                }

                _ = &mut timeout_sleep => {
                    self.handle_timeouts().await;
                    self.handle_can_stop_retries().await;
                }
            }
        }
    }

    /// Background task that reads messages from a connection and forwards them as events.
    async fn read_loop(mut read_half: ReadHalf, tx: mpsc::Sender<Event>) {
        let conn_id = read_half.id;
        loop {
            match read_half.read_message().await {
                Ok(Some(bytes)) => {
                    if tx.send(Event::Message(conn_id, bytes)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = tx.send(Event::Disconnected(conn_id, None)).await;
                    break;
                }
                Err(e) => {
                    let _ = tx.send(Event::Disconnected(conn_id, Some(e))).await;
                    break;
                }
            }
        }
    }

    /// Route a raw message from a connection based on its current client kind.
    async fn handle_message(
        &mut self,
        conn_id: Uuid,
        bytes: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let is_unknown = matches!(
            self.connections.get(&conn_id).map(|c| &c.kind),
            Some(ClientKind::Unknown)
        );

        if is_unknown {
            self.identify_and_handle(conn_id, bytes).await
        } else {
            let is_plugin = matches!(
                self.connections.get(&conn_id).map(|c| &c.kind),
                Some(ClientKind::Plugin { .. })
            );

            if is_plugin {
                let msg: PluginMessage = serde_json::from_slice(bytes)?;
                self.handle_plugin_message(conn_id, msg).await
            } else {
                let msg: AppMessage = serde_json::from_slice(bytes)?;
                self.handle_app_message(conn_id, msg).await
            }
        }
    }

    /// First message from a connection identifies it as plugin or app.
    async fn identify_and_handle(
        &mut self,
        conn_id: Uuid,
        bytes: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Try plugin message first (EntityUpdated/EntityRemoved identify a plugin)
        if let Ok(msg) = serde_json::from_slice::<PluginMessage>(bytes) {
            let plugin_name = match &msg {
                PluginMessage::EntityUpdated { urn, .. }
                | PluginMessage::EntityRemoved { urn, .. } => urn.plugin().to_string(),
                _ => {
                    return Err(
                        "plugin must send EntityUpdated or EntityRemoved as first message".into(),
                    );
                }
            };

            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.kind = ClientKind::Plugin {
                    name: plugin_name.clone(),
                };
            }
            self.plugin_registry
                .register(plugin_name.clone(), conn_id);
            // Clear stopped state since the plugin is now running
            self.stopped_plugins.remove(&plugin_name);
            let result = self.handle_plugin_message(conn_id, msg).await;
            // Emit updated plugin-status (now Running)
            self.emit_plugin_status(&plugin_name).await;
            return result;
        }

        // Try app message
        if let Ok(msg) = serde_json::from_slice::<AppMessage>(bytes) {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.kind = ClientKind::App {
                    subscriptions: Default::default(),
                };
            }
            debug!("connection {conn_id} identified as app");
            return self.handle_app_message(conn_id, msg).await;
        }

        Err("could not parse first message as PluginMessage or AppMessage".into())
    }

    /// Handle a message from a plugin.
    async fn handle_plugin_message(
        &mut self,
        conn_id: Uuid,
        msg: PluginMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            PluginMessage::EntityUpdated {
                ref urn,
                ref entity_type,
                ref data,
            } => {
                // Cache the entity data
                self.entity_cache.insert(
                    urn.as_str().to_string(),
                    CachedEntity {
                        urn: urn.clone(),
                        entity_type: entity_type.clone(),
                        data: data.clone(),
                    },
                );

                let notification = AppNotification::EntityUpdated {
                    urn: urn.clone(),
                    entity_type: entity_type.clone(),
                    data: data.clone(),
                };

                let subscribers = self.app_registry.subscribers(entity_type);
                for app_id in subscribers {
                    if let Some(conn) = self.connections.get(&app_id)
                        && let Err(e) = conn.send(&notification).await
                    {
                        warn!("failed to forward EntityUpdated to {app_id}: {e}");
                    }
                }
            }

            PluginMessage::EntityRemoved {
                ref urn,
                ref entity_type,
            } => {
                // Remove from entity cache
                self.entity_cache.remove(urn.as_str());

                let notification = AppNotification::EntityRemoved {
                    urn: urn.clone(),
                    entity_type: entity_type.clone(),
                };

                let subscribers = self.app_registry.subscribers(entity_type);
                for app_id in subscribers {
                    if let Some(conn) = self.connections.get(&app_id)
                        && let Err(e) = conn.send(&notification).await
                    {
                        warn!("failed to forward EntityRemoved to {app_id}: {e}");
                    }
                }
            }

            PluginMessage::ActionSuccess { action_id } => {
                if let Some(action) = self.action_tracker.resolve(action_id) {
                    if let Some(conn) = self.connections.get(&action.app_conn_id)
                        && let Err(e) = conn
                            .send(&AppNotification::ActionSuccess { action_id })
                            .await
                    {
                        warn!(
                            "failed to forward ActionSuccess to {}: {e}",
                            action.app_conn_id
                        );
                    }
                } else {
                    warn!("ActionSuccess for unknown action {action_id}");
                }
            }

            PluginMessage::ActionError { action_id, error } => {
                if let Some(action) = self.action_tracker.resolve(action_id) {
                    if let Some(conn) = self.connections.get(&action.app_conn_id)
                        && let Err(e) = conn
                            .send(&AppNotification::ActionError {
                                action_id,
                                error: error.clone(),
                            })
                            .await
                    {
                        warn!(
                            "failed to forward ActionError to {}: {e}",
                            action.app_conn_id
                        );
                    }
                } else {
                    warn!("ActionError for unknown action {action_id}");
                }
            }

            PluginMessage::ClaimCheck { ref urn, claim_id } => {
                let entity_type = urn.entity_type().to_string();
                let subscribers = self.app_registry.subscribers(&entity_type);

                if subscribers.is_empty() {
                    // No subscribers — immediately resolve as not claimed
                    debug!("ClaimCheck for {urn}: no subscribers, resolving immediately");
                    let cmd = PluginCommand::ClaimResult {
                        urn: urn.clone(),
                        claim_id,
                        claimed: false,
                    };
                    if let Some(conn) = self.connections.get(&conn_id) && let Err(e) = conn.send(&cmd).await {
                        warn!("failed to send immediate ClaimResult to plugin: {e}");
                    }
                } else {
                    // Track the claim and broadcast to all subscribers
                    self.claim_tracker.start(
                        claim_id,
                        urn.clone(),
                        entity_type.clone(),
                        conn_id,
                        subscribers.iter().copied().collect(),
                    );

                    let notification = AppNotification::ClaimCheck {
                        urn: urn.clone(),
                        claim_id,
                    };
                    for app_id in &subscribers {
                        if let Some(conn) = self.connections.get(app_id) && let Err(e) = conn.send(&notification).await {
                            warn!("failed to send ClaimCheck to {app_id}: {e}");
                        }
                    }
                    debug!(
                        "ClaimCheck for {urn}: broadcast to {} subscribers",
                        subscribers.len()
                    );
                }
            }

            PluginMessage::StopResponse { can_stop } => {
                let plugin_name = self.connections.get(&conn_id).and_then(|c| match &c.kind {
                    ClientKind::Plugin { name } => Some(name.clone()),
                    _ => None,
                });

                if let Some(ref name) = plugin_name {
                    if can_stop {
                        info!("plugin {name} confirmed it can stop, disconnecting");
                        self.pending_can_stops.remove(name.as_str());
                        self.graceful_stops.insert(name.clone());
                        self.remove_connection(conn_id).await;
                    } else {
                        debug!("plugin {name} cannot stop, will retry in 30s");
                        self.pending_can_stops.insert(
                            name.clone(),
                            PendingCanStop {
                                plugin_conn_id: conn_id,
                                retry_at: tokio::time::Instant::now()
                                    + std::time::Duration::from_secs(30),
                            },
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle a message from an app.
    async fn handle_app_message(
        &mut self,
        conn_id: Uuid,
        msg: AppMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            AppMessage::Subscribe { entity_type } => {
                self.app_registry.subscribe(entity_type.clone(), conn_id);
                debug!("app {conn_id} subscribed to {entity_type}");

                // Track subscription in connection state
                if let Some(conn) = self.connections.get_mut(&conn_id)
                    && let ClientKind::App { subscriptions } = &mut conn.kind
                {
                    subscriptions.insert(entity_type.clone());
                }

                // For plugin-status, the daemon itself produces entities --
                // no external plugin to spawn. Send all known statuses to this app.
                if entity_type == plugin_entity::ENTITY_TYPE {
                    self.emit_all_plugin_statuses_to(conn_id).await;
                } else {
                    // Spawn plugin on demand if none is connected for this entity type
                    self.plugin_spawner
                        .ensure_plugin_for_entity_type(&entity_type);
                }
            }

            AppMessage::Unsubscribe { entity_type } => {
                self.app_registry.unsubscribe(&entity_type, conn_id);
                debug!("app {conn_id} unsubscribed from {entity_type}");

                if let Some(conn) = self.connections.get_mut(&conn_id)
                    && let ClientKind::App { subscriptions } = &mut conn.kind
                {
                    subscriptions.remove(&entity_type);
                }

                // Check if any plugin now has zero subscribers and can be stopped
                self.check_can_stop_for_entity_type(&entity_type).await;
            }

            AppMessage::Status { entity_type } => {
                debug!("app {conn_id} requested status for {entity_type}");

                if let Some(conn) = self.connections.get(&conn_id) {
                    for cached in self.entity_cache.values() {
                        if cached.entity_type == entity_type {
                            let notification = AppNotification::EntityUpdated {
                                urn: cached.urn.clone(),
                                entity_type: cached.entity_type.clone(),
                                data: cached.data.clone(),
                            };
                            if let Err(e) = conn.send(&notification).await {
                                warn!("failed to send cached entity to {conn_id}: {e}");
                                break;
                            }
                        }
                    }
                }
            }

            AppMessage::TriggerAction {
                urn,
                action,
                action_id,
                params,
                timeout_ms,
            } => {
                if let Some(plugin_conn_id) = self.plugin_registry.connection_for_urn(&urn) {
                    self.action_tracker
                        .track(action_id, conn_id, plugin_conn_id, timeout_ms);

                    let cmd = PluginCommand::TriggerAction {
                        urn,
                        action,
                        action_id,
                        params,
                    };

                    if let Some(plugin_conn) = self.connections.get(&plugin_conn_id)
                        && let Err(e) = plugin_conn.send(&cmd).await
                    {
                        warn!("failed to forward TriggerAction to plugin: {e}");
                        // Resolve the action as failed
                        if let Some(action) = self.action_tracker.resolve(action_id)
                            && let Some(app_conn) = self.connections.get(&action.app_conn_id)
                        {
                            let _ = app_conn
                                .send(&AppNotification::ActionError {
                                    action_id,
                                    error: format!("plugin communication failed: {e}"),
                                })
                                .await;
                        }
                    }
                } else {
                    // No plugin found for this URN
                    if let Some(conn) = self.connections.get(&conn_id) {
                        let _ = conn
                            .send(&AppNotification::ActionError {
                                action_id,
                                error: format!("no plugin found for URN: {urn}"),
                            })
                            .await;
                    }
                }
            }

            AppMessage::ClaimResponse { claim_id, claimed } => {
                if let Some(resolution) =
                    self.claim_tracker.record_response(claim_id, conn_id, claimed)
                {
                    self.send_claim_result(&resolution).await;
                }
            }

            AppMessage::Describe { plugin_name } => {
                let plugins: Vec<_> = if let Some(ref name) = plugin_name {
                    self.plugin_spawner
                        .get_description(name)
                        .into_iter()
                        .cloned()
                        .collect()
                } else {
                    self.plugin_spawner
                        .all_descriptions()
                        .into_iter()
                        .cloned()
                        .collect()
                };

                debug!(
                    "app {conn_id} requested descriptions (filter: {:?}, found: {})",
                    plugin_name,
                    plugins.len(),
                );

                if let Some(conn) = self.connections.get(&conn_id) {
                    let response = AppNotification::DescribeResponse { plugins };
                    if let Err(e) = conn.send(&response).await {
                        warn!("failed to send DescribeResponse to {conn_id}: {e}");
                    }
                }
            }
        }

        Ok(())
    }

    /// Check for timed-out actions and claims, and notify the requesting apps/plugins.
    async fn handle_timeouts(&mut self) {
        let timed_out = self.action_tracker.drain_timed_out();
        for action in timed_out {
            warn!(
                "action {} timed out (app: {})",
                action.action_id, action.app_conn_id
            );
            if let Some(conn) = self.connections.get(&action.app_conn_id) {
                let _ = conn
                    .send(&AppNotification::ActionError {
                        action_id: action.action_id,
                        error: "action timed out".to_string(),
                    })
                    .await;
            }
        }

        // Resolve timed-out claims (treat non-respondents as "pass")
        for resolution in self.claim_tracker.drain_timed_out() {
            self.send_claim_result(&resolution).await;
        }
    }

    /// Retry CanStop for plugins whose retry timer has expired.
    async fn handle_can_stop_retries(&mut self) {
        let now = tokio::time::Instant::now();
        let ready: Vec<String> = self
            .pending_can_stops
            .iter()
            .filter(|(_, p)| p.retry_at <= now)
            .map(|(name, _)| name.clone())
            .collect();

        for name in ready {
            if let Some(pending) = self.pending_can_stops.remove(&name) {
                // Re-check if the plugin still has no subscribers before retrying
                if self.plugin_has_subscribers(&name) {
                    debug!("plugin {name} now has subscribers, cancelling CanStop retry");
                } else if let Some(conn) = self.connections.get(&pending.plugin_conn_id) {
                    debug!("retrying CanStop for plugin {name}");
                    if let Err(e) = conn.send(&PluginCommand::CanStop).await {
                        warn!("failed to send CanStop retry to {name}: {e}");
                    }
                }
            }
        }
    }

    /// Send a ClaimResult back to the originating plugin.
    async fn send_claim_result(&self, resolution: &ClaimResolution) {
        let cmd = PluginCommand::ClaimResult {
            urn: resolution.urn.clone(),
            claim_id: resolution.claim_id,
            claimed: resolution.claimed,
        };
        if let Some(conn) = self.connections.get(&resolution.plugin_conn_id) && let Err(e) = conn.send(&cmd).await {
            warn!(
                "failed to send ClaimResult to plugin {}: {e}",
                resolution.plugin_conn_id
            );
        }
    }

    /// Get the entity types a plugin provides (from the entity cache).
    fn entity_types_for_plugin(&self, plugin_name: &str) -> Vec<String> {
        let mut types: Vec<String> = self
            .entity_cache
            .values()
            .filter(|e| e.urn.plugin() == plugin_name)
            .map(|e| e.entity_type.clone())
            .collect();
        types.sort();
        types.dedup();
        types
    }

    /// Check if a plugin has any subscribers across all its entity types.
    fn plugin_has_subscribers(&self, plugin_name: &str) -> bool {
        let entity_types = self.entity_types_for_plugin(plugin_name);
        entity_types
            .iter()
            .any(|et| self.app_registry.has_subscribers(et))
    }

    /// Check if any plugin providing this entity type now has zero subscribers.
    /// If so, send CanStop to that plugin.
    async fn check_can_stop_for_entity_type(&mut self, entity_type: &str) {
        // Find all plugin names that provide this entity type
        let plugin_names: Vec<String> = self
            .entity_cache
            .values()
            .filter(|e| e.entity_type == entity_type)
            .map(|e| e.urn.plugin().to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for plugin_name in plugin_names {
            // Skip if already pending a CanStop
            if self.pending_can_stops.contains_key(&plugin_name) {
                continue;
            }

            // Check if this plugin has zero subscribers across ALL its entity types
            if !self.plugin_has_subscribers(&plugin_name)
                && let Some(conn_id) = self.plugin_registry.connection_for_plugin(&plugin_name)
            {
                debug!("plugin {plugin_name} has zero subscribers, sending CanStop");
                if let Some(conn) = self.connections.get(&conn_id)
                    && let Err(e) = conn.send(&PluginCommand::CanStop).await
                {
                    warn!("failed to send CanStop to {plugin_name}: {e}");
                }
            }
        }
    }

    /// Clean up all state for a disconnected connection.
    async fn remove_connection(&mut self, conn_id: Uuid) {
        let plugin_name = if let Some(conn) = self.connections.remove(&conn_id) {
            if let ClientKind::Plugin { ref name } = conn.kind {
                info!("plugin {name} disconnected (conn {conn_id})");
                Some(name.clone())
            } else {
                None
            }
        } else {
            None
        };

        // Collect entity info before removing from cache (needed for notifications)
        let plugin_entities: Vec<(Urn, String)> = if let Some(ref name) = plugin_name {
            self.entity_cache
                .values()
                .filter(|e| e.urn.plugin() == name.as_str())
                .map(|e| (e.urn.clone(), e.entity_type.clone()))
                .collect()
        } else {
            Vec::new()
        };
        let plugin_entity_types: Vec<String> = {
            let mut types: Vec<String> = plugin_entities.iter().map(|(_, et)| et.clone()).collect();
            types.sort();
            types.dedup();
            types
        };

        // Remove cached entities and pending CanStop for disconnected plugin
        if let Some(ref name) = plugin_name {
            self.entity_cache
                .retain(|_, cached| cached.urn.plugin() != name.as_str());
            self.pending_can_stops.remove(name.as_str());
        }

        // Cancel any pending claims from this plugin
        let cancelled = self.claim_tracker.remove_plugin_conn(conn_id);
        if !cancelled.is_empty() {
            debug!(
                "cancelled {} pending claims for disconnected plugin",
                cancelled.len()
            );
        }

        self.plugin_registry.remove_connection(conn_id);

        // Handle plugin crash detection and restart
        if let Some(ref name) = plugin_name {
            let graceful = self.graceful_stops.remove(name.as_str());
            if graceful {
                self.stopped_plugins.insert(name.clone());
            }
            self.plugin_spawner.mark_disconnected(name);

            if !graceful {
                // Unexpected disconnect: check crash tracker and potentially restart
                let outcome = self.crash_tracker.record_crash(name);

                // Notify subscribers about stale/outdated entities
                for (urn, entity_type) in &plugin_entities {
                    let notification = match outcome {
                        CrashOutcome::Restart => AppNotification::EntityStale {
                            urn: urn.clone(),
                            entity_type: entity_type.clone(),
                        },
                        CrashOutcome::CircuitBroken => AppNotification::EntityOutdated {
                            urn: urn.clone(),
                            entity_type: entity_type.clone(),
                        },
                    };
                    let subscribers = self.app_registry.subscribers(entity_type);
                    for app_id in subscribers {
                        if let Some(conn) = self.connections.get(&app_id)
                            && let Err(e) = conn.send(&notification).await
                        {
                            warn!(
                                "failed to send stale/outdated notification to {app_id}: {e}"
                            );
                        }
                    }
                }

                match outcome {
                    CrashOutcome::Restart => {
                        // Only restart if there are still subscribers for this plugin's entity types
                        let has_subscribers = plugin_entity_types
                            .iter()
                            .any(|et| self.app_registry.has_subscribers(et));

                        if has_subscribers {
                            info!("plugin {name} crashed, restarting");
                            for et in &plugin_entity_types {
                                self.plugin_spawner.ensure_plugin_for_entity_type(et);
                            }
                        } else {
                            info!(
                                "plugin {name} crashed but has no subscribers, not restarting"
                            );
                        }
                    }
                    CrashOutcome::CircuitBroken => {
                        warn!(
                            "plugin {name} crashed too many times, circuit breaker tripped"
                        );
                    }
                }
            }
        }

        self.app_registry.remove_connection(conn_id);

        // Treat disconnecting app as having responded "pass" for all pending claims
        for resolution in self.claim_tracker.remove_app_conn(conn_id) {
            self.send_claim_result(&resolution).await;
        }

        // When an app disconnects, check if any plugins now have zero subscribers
        if plugin_name.is_none() {
            let all_plugins = self.plugin_registry.all_plugin_names();
            for name in all_plugins {
                if !self.pending_can_stops.contains_key(&name)
                    && !self.plugin_has_subscribers(&name)
                    && let Some(plugin_conn_id) = self.plugin_registry.connection_for_plugin(&name)
                {
                    debug!(
                        "plugin {name} has zero subscribers after app disconnect, sending CanStop"
                    );
                    if let Some(conn) = self.connections.get(&plugin_conn_id)
                        && let Err(e) = conn.send(&PluginCommand::CanStop).await
                    {
                        warn!("failed to send CanStop to {name}: {e}");
                    }
                }
            }
        }

        // Notify apps of failed actions when plugin disconnects
        let orphaned = self.action_tracker.drain_for_connection(conn_id);
        for action in orphaned {
            if let Some(conn) = self.connections.get(&action.app_conn_id) {
                let _ = conn
                    .send(&AppNotification::ActionError {
                        action_id: action.action_id,
                        error: "plugin disconnected".to_string(),
                    })
                    .await;
            }
        }

        // Emit updated plugin-status for the disconnected plugin
        if let Some(ref name) = plugin_name {
            self.emit_plugin_status(name).await;
        }
    }

    /// Compute the current lifecycle state of a plugin.
    fn compute_plugin_state(&self, plugin_name: &str) -> PluginState {
        if self.crash_tracker.circuit_broken(plugin_name) {
            PluginState::Failed
        } else if self.plugin_registry.connection_for_plugin(plugin_name).is_some() {
            PluginState::Running
        } else if self.stopped_plugins.contains(plugin_name) {
            PluginState::Stopped
        } else {
            PluginState::Available
        }
    }

    /// Build and broadcast a plugin-status entity for a single plugin.
    async fn emit_plugin_status(&self, plugin_name: &str) {
        let state = self.compute_plugin_state(plugin_name);
        let entity_types: Vec<String> = self
            .plugin_spawner
            .all_plugins()
            .into_iter()
            .find(|(name, _)| name == plugin_name)
            .map(|(_, types)| types)
            .unwrap_or_default();

        let status = PluginStatus {
            name: plugin_name.to_string(),
            state,
            entity_types,
        };

        let urn = Urn::new("waft", plugin_entity::ENTITY_TYPE, plugin_name);
        let data = match serde_json::to_value(&status) {
            Ok(d) => d,
            Err(e) => {
                error!("failed to serialize plugin-status for {plugin_name}: {e}");
                return;
            }
        };

        let notification = AppNotification::EntityUpdated {
            urn,
            entity_type: plugin_entity::ENTITY_TYPE.to_string(),
            data,
        };

        let subscribers = self.app_registry.subscribers(plugin_entity::ENTITY_TYPE);
        for app_id in subscribers {
            if let Some(conn) = self.connections.get(&app_id)
                && let Err(e) = conn.send(&notification).await
            {
                warn!("failed to send plugin-status to {app_id}: {e}");
            }
        }
    }

    /// Emit plugin-status entities for all discovered plugins to a specific app.
    async fn emit_all_plugin_statuses_to(&self, app_conn_id: Uuid) {
        let all_plugins = self.plugin_spawner.all_plugins();

        if let Some(conn) = self.connections.get(&app_conn_id) {
            for (plugin_name, entity_types) in all_plugins {
                let state = self.compute_plugin_state(&plugin_name);
                let status = PluginStatus {
                    name: plugin_name.clone(),
                    state,
                    entity_types,
                };

                let urn = Urn::new("waft", plugin_entity::ENTITY_TYPE, &plugin_name);
                let data = match serde_json::to_value(&status) {
                    Ok(d) => d,
                    Err(e) => {
                        error!("failed to serialize plugin-status for {plugin_name}: {e}");
                        continue;
                    }
                };

                let notification = AppNotification::EntityUpdated {
                    urn,
                    entity_type: plugin_entity::ENTITY_TYPE.to_string(),
                    data,
                };

                if let Err(e) = conn.send(&notification).await {
                    warn!("failed to send plugin-status to {app_conn_id}: {e}");
                    break;
                }
            }
        }
    }
}
