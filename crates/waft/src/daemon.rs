use std::collections::HashMap;
use std::path::PathBuf;

use tokio::net::UnixListener;
use tokio::sync::mpsc;
use uuid::Uuid;
use waft_protocol::{AppMessage, AppNotification, PluginCommand, PluginMessage};

use crate::action_tracker::ActionTracker;
use crate::connection::{ClientKind, Connection, ConnectionError, ReadHalf};
use crate::registry::{AppRegistry, PluginRegistry};

/// Event from a connection reader or timeout checker.
enum Event {
    /// A message was received from a connection.
    Message(Uuid, Vec<u8>),
    /// A connection disconnected or errored.
    Disconnected(Uuid, Option<ConnectionError>),
    /// Time to check for timed-out actions.
    CheckTimeouts,
}

/// The waft daemon: accepts connections, identifies clients, routes messages.
pub struct WaftDaemon {
    listener: UnixListener,
    connections: HashMap<Uuid, Connection>,
    plugin_registry: PluginRegistry,
    app_registry: AppRegistry,
    action_tracker: ActionTracker,
    event_rx: mpsc::Receiver<Event>,
    event_tx: mpsc::Sender<Event>,
}

impl WaftDaemon {
    pub fn new(socket_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = UnixListener::bind(&socket_path)?;
        let (event_tx, event_rx) = mpsc::channel(256);

        Ok(WaftDaemon {
            listener,
            connections: HashMap::new(),
            plugin_registry: PluginRegistry::new(),
            app_registry: AppRegistry::new(),
            action_tracker: ActionTracker::new(),
            event_rx,
            event_tx,
        })
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Spawn timeout checker
        let timeout_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                if timeout_tx.send(Event::CheckTimeouts).await.is_err() {
                    break;
                }
            }
            eprintln!("[waft] timeout checker stopped");
        });

        loop {
            tokio::select! {
                accept = self.listener.accept() => {
                    match accept {
                        Ok((stream, _)) => {
                            let (conn, read_half) = Connection::new(stream);
                            let conn_id = conn.id;
                            eprintln!("[waft] new connection: {conn_id}");
                            self.connections.insert(conn_id, conn);

                            // Spawn read loop for this connection
                            let tx = self.event_tx.clone();
                            tokio::spawn(Self::read_loop(read_half, tx));
                        }
                        Err(e) => {
                            eprintln!("[waft] accept error: {e}");
                        }
                    }
                }

                Some(event) = self.event_rx.recv() => {
                    match event {
                        Event::Message(conn_id, bytes) => {
                            if let Err(e) = self.handle_message(conn_id, &bytes).await {
                                eprintln!("[waft] error handling message from {conn_id}: {e}");
                                self.remove_connection(conn_id).await;
                            }
                        }
                        Event::Disconnected(conn_id, err) => {
                            if let Some(e) = err {
                                eprintln!("[waft] connection {conn_id} error: {e}");
                            } else {
                                eprintln!("[waft] connection {conn_id} disconnected");
                            }
                            self.remove_connection(conn_id).await;
                        }
                        Event::CheckTimeouts => {
                            self.handle_timeouts().await;
                        }
                    }
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
            self.plugin_registry.register(plugin_name, conn_id);
            return self.handle_plugin_message(conn_id, msg).await;
        }

        // Try app message
        if let Ok(msg) = serde_json::from_slice::<AppMessage>(bytes) {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.kind = ClientKind::App {
                    subscriptions: Default::default(),
                };
            }
            eprintln!("[waft] connection {conn_id} identified as app");
            return self.handle_app_message(conn_id, msg).await;
        }

        Err("could not parse first message as PluginMessage or AppMessage".into())
    }

    /// Handle a message from a plugin.
    async fn handle_plugin_message(
        &mut self,
        _conn_id: Uuid,
        msg: PluginMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            PluginMessage::EntityUpdated {
                ref urn,
                ref entity_type,
                ..
            } => {
                let notification = AppNotification::EntityUpdated {
                    urn: urn.clone(),
                    entity_type: entity_type.clone(),
                    data: match &msg {
                        PluginMessage::EntityUpdated { data, .. } => data.clone(),
                        _ => unreachable!(),
                    },
                };

                let subscribers = self.app_registry.subscribers(entity_type);
                for app_id in subscribers {
                    if let Some(conn) = self.connections.get(&app_id) {
                        if let Err(e) = conn.send(&notification).await {
                            eprintln!("[waft] failed to forward EntityUpdated to {app_id}: {e}");
                        }
                    }
                }
            }

            PluginMessage::EntityRemoved {
                ref urn,
                ref entity_type,
            } => {
                let notification = AppNotification::EntityRemoved {
                    urn: urn.clone(),
                    entity_type: entity_type.clone(),
                };

                let subscribers = self.app_registry.subscribers(entity_type);
                for app_id in subscribers {
                    if let Some(conn) = self.connections.get(&app_id) {
                        if let Err(e) = conn.send(&notification).await {
                            eprintln!("[waft] failed to forward EntityRemoved to {app_id}: {e}");
                        }
                    }
                }
            }

            PluginMessage::ActionSuccess { action_id } => {
                if let Some(action) = self.action_tracker.resolve(action_id) {
                    if let Some(conn) = self.connections.get(&action.app_conn_id) {
                        if let Err(e) = conn
                            .send(&AppNotification::ActionSuccess { action_id })
                            .await
                        {
                            eprintln!(
                                "[waft] failed to forward ActionSuccess to {}: {e}",
                                action.app_conn_id
                            );
                        }
                    }
                } else {
                    eprintln!("[waft] ActionSuccess for unknown action {action_id}");
                }
            }

            PluginMessage::ActionError { action_id, error } => {
                if let Some(action) = self.action_tracker.resolve(action_id) {
                    if let Some(conn) = self.connections.get(&action.app_conn_id) {
                        if let Err(e) = conn
                            .send(&AppNotification::ActionError {
                                action_id,
                                error: error.clone(),
                            })
                            .await
                        {
                            eprintln!(
                                "[waft] failed to forward ActionError to {}: {e}",
                                action.app_conn_id
                            );
                        }
                    }
                } else {
                    eprintln!("[waft] ActionError for unknown action {action_id}");
                }
            }

            PluginMessage::StopResponse { .. } => {
                // Phase 2: handle graceful stop
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
                self.app_registry
                    .subscribe(entity_type.clone(), conn_id);
                eprintln!("[waft] app {conn_id} subscribed to {entity_type}");

                // Track subscription in connection state
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    if let ClientKind::App { subscriptions } = &mut conn.kind {
                        subscriptions.insert(entity_type);
                    }
                }
            }

            AppMessage::Unsubscribe { entity_type } => {
                self.app_registry.unsubscribe(&entity_type, conn_id);
                eprintln!("[waft] app {conn_id} unsubscribed from {entity_type}");

                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    if let ClientKind::App { subscriptions } = &mut conn.kind {
                        subscriptions.remove(&entity_type);
                    }
                }
            }

            AppMessage::Status { .. } => {
                // Phase 2: request current state from plugin
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

                    if let Some(plugin_conn) = self.connections.get(&plugin_conn_id) {
                        if let Err(e) = plugin_conn.send(&cmd).await {
                            eprintln!("[waft] failed to forward TriggerAction to plugin: {e}");
                            // Resolve the action as failed
                            if let Some(action) = self.action_tracker.resolve(action_id) {
                                if let Some(app_conn) = self.connections.get(&action.app_conn_id) {
                                    let _ = app_conn
                                        .send(&AppNotification::ActionError {
                                            action_id,
                                            error: format!("plugin communication failed: {e}"),
                                        })
                                        .await;
                                }
                            }
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
        }

        Ok(())
    }

    /// Check for timed-out actions and notify the requesting apps.
    async fn handle_timeouts(&mut self) {
        let timed_out = self.action_tracker.drain_timed_out();
        for action in timed_out {
            eprintln!(
                "[waft] action {} timed out (app: {})",
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
    }

    /// Clean up all state for a disconnected connection.
    async fn remove_connection(&mut self, conn_id: Uuid) {
        if let Some(conn) = self.connections.remove(&conn_id) {
            if let ClientKind::Plugin { name } = &conn.kind {
                eprintln!("[waft] plugin {name} disconnected (conn {conn_id})");
            }
        }
        self.plugin_registry.remove_connection(conn_id);
        self.app_registry.remove_connection(conn_id);

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
    }
}
