//! Plugin runtime: connects to the waft daemon and manages the event loop.
//!
//! The runtime is the core of a plugin process. It connects to the daemon's
//! Unix socket as a client, sends entity updates, and handles incoming
//! commands (actions, stop requests).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::net::UnixStream;
use tokio::sync::{Mutex, mpsc, watch};
use uuid::Uuid;
use waft_protocol::urn::Urn;
use waft_protocol::{
    CAP_DERIVED_ENTITY_TYPE, CAP_HANDSHAKE, CAP_SCHEMA_METADATA, CAP_STATUS_COMPLETE,
    CAP_STRUCTURED_ERRORS, HandshakeMessage, Hello, PluginCommand, PluginMessage,
    PROTOCOL_VERSION, ProtocolError,
};

use crate::notifier::EntityNotifier;
use crate::plugin::Plugin;
use crate::transport::{read_framed, write_framed};

/// Plugin runtime that connects to the waft daemon and runs the event loop.
pub struct PluginRuntime<P: Plugin> {
    name: String,
    plugin: Arc<P>,
    notifier_rx: watch::Receiver<u64>,
}

impl<P: Plugin + 'static> PluginRuntime<P> {
    /// Create a new plugin runtime.
    ///
    /// Returns `(runtime, notifier)`. The plugin calls `notifier.notify()`
    /// whenever its state changes.
    pub fn new(name: impl Into<String>, plugin: P) -> (Self, EntityNotifier) {
        let (notifier, notifier_rx) = EntityNotifier::new();
        let runtime = Self {
            name: name.into(),
            plugin: Arc::new(plugin),
            notifier_rx,
        };
        (runtime, notifier)
    }

    /// Create a runtime from pre-built parts.
    ///
    /// Used by [`crate::runner::PluginRunner`] where the notifier is created
    /// before the plugin so background tasks can capture it.
    pub fn from_parts(
        name: impl Into<String>,
        plugin: P,
        notifier_rx: tokio::sync::watch::Receiver<u64>,
    ) -> Self {
        Self {
            name: name.into(),
            plugin: Arc::new(plugin),
            notifier_rx,
        }
    }

    /// Run the plugin: connect to the daemon and start the event loop.
    pub async fn run(self) -> anyhow::Result<()> {
        let socket_path = daemon_socket_path();
        log::info!(
            "[{}] connecting to daemon at {}",
            self.name,
            socket_path.display()
        );

        let stream = UnixStream::connect(&socket_path).await.map_err(|e| {
            log::error!(
                "[{}] failed to connect to daemon at {}: {e}",
                self.name,
                socket_path.display()
            );
            e
        })?;

        log::info!("[{}] connected to daemon", self.name);

        let (mut read_half, mut write_half) = stream.into_split();
        let hello = HandshakeMessage::Hello(Hello::plugin(
            self.name.clone(),
            format!("waft-{}-daemon", self.name),
            PROTOCOL_VERSION,
            vec![
                CAP_HANDSHAKE.to_string(),
                CAP_STRUCTURED_ERRORS.to_string(),
                CAP_DERIVED_ENTITY_TYPE.to_string(),
                CAP_STATUS_COMPLETE.to_string(),
                CAP_SCHEMA_METADATA.to_string(),
            ],
        ));
        write_framed(&mut write_half, &hello).await?;
        let negotiated = match read_framed::<_, HandshakeMessage>(&mut read_half).await? {
            Some(HandshakeMessage::HelloAck(ack)) => ack,
            Some(HandshakeMessage::HelloError(err)) => {
                anyhow::bail!("daemon handshake rejected plugin: {}", err.error.message)
            }
            Some(other) => anyhow::bail!("unexpected handshake response from daemon: {other:?}"),
            None => anyhow::bail!("daemon disconnected before handshake completed"),
        };
        let omit_entity_type = negotiated
            .capabilities
            .iter()
            .any(|cap| cap == CAP_DERIVED_ENTITY_TYPE);

        // Spawn background write task with mpsc channel
        let (write_tx, write_rx) = mpsc::channel::<PluginMessage>(64);
        let name_for_writer = self.name.clone();
        tokio::spawn(write_loop(name_for_writer, write_half, write_rx));

        // Shared previous-entity state for diffing (used by both the main loop and handle_action)
        let previous: Arc<Mutex<HashMap<String, serde_json::Value>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Send initial entities
        send_all_entities(
            &*self.plugin,
            &write_tx,
            &self.name,
            &previous,
            omit_entity_type,
        )
        .await;

        // Event loop
        let mut notifier_rx = self.notifier_rx;

        loop {
            tokio::select! {
                // Entity notifier fired — re-send entities
                changed = notifier_rx.changed() => {
                    if changed.is_err() {
                        log::info!("[{}] notifier dropped, shutting down", self.name);
                        break;
                    }
                    send_all_entities(&*self.plugin, &write_tx, &self.name, &previous, omit_entity_type).await;
                }

                // Incoming command from daemon
                msg = read_framed::<_, PluginCommand>(&mut read_half) => {
                    match msg {
                        Ok(Some(cmd)) => {
                            match cmd {
                                PluginCommand::TriggerAction { urn, action, action_id, params } => {
                                    let ctx = ActionContext {
                                        plugin: self.plugin.clone(),
                                        tx: write_tx.clone(),
                                        name: self.name.clone(),
                                        previous: previous.clone(),
                                        omit_entity_type,
                                    };
                                    tokio::spawn(async move {
                                        handle_action(ctx, urn, action, action_id, params).await;
                                    });
                                }
                                PluginCommand::CanStop => {
                                    let can_stop = self.plugin.can_stop();
                                    if let Err(e) = write_tx.send(PluginMessage::StopResponse { can_stop }).await {
                                        log::warn!("[{}] failed to send StopResponse: {e}", self.name);
                                    }
                                }
                                PluginCommand::SubscriberCountChanged { entity_type, count } => {
                                    let ctx = ActionContext {
                                        plugin: self.plugin.clone(),
                                        tx: write_tx.clone(),
                                        name: self.name.clone(),
                                        previous: previous.clone(),
                                        omit_entity_type,
                                    };
                                    tokio::spawn(async move {
                                        ctx.plugin.handle_subscriber_count_changed(entity_type, count).await;
                                        send_all_entities(&*ctx.plugin, &ctx.tx, &ctx.name, &ctx.previous, ctx.omit_entity_type).await;
                                    });
                                }
                            }
                        }
                        Ok(None) => {
                            log::info!("[{}] daemon disconnected (EOF)", self.name);
                            break;
                        }
                        Err(e) => {
                            log::error!("[{}] read error: {e}", self.name);
                            break;
                        }
                    }
                }

            }
        }

        log::info!("[{}] runtime exiting", self.name);
        Ok(())
    }
}

/// Context for handling plugin actions.
struct ActionContext<P: Plugin> {
    plugin: Arc<P>,
    name: String,
    tx: mpsc::Sender<PluginMessage>,
    previous: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    omit_entity_type: bool,
}

/// Handle an incoming action: run plugin handler, send success/error, re-send entities.
async fn handle_action<P: Plugin>(
    ctx: ActionContext<P>,
    urn: Urn,
    action: String,
    action_id: Uuid,
    params: serde_json::Value,
) {
    match ctx.plugin.handle_action(urn, action, params).await {
        Ok(response_data) => {
            let data = if response_data.is_null() {
                None
            } else {
                Some(response_data)
            };
            if let Err(e) = ctx
                .tx
                .send(PluginMessage::ActionSuccess { action_id, data })
                .await
            {
                log::warn!("[{}] failed to send ActionSuccess: {e}", ctx.name);
                return;
            }
        }
        Err(e) => {
            log::error!("[{}] action {action_id} failed: {e}", ctx.name);
            if let Err(send_err) = ctx
                .tx
                .send(PluginMessage::ActionError {
                    action_id,
                    error: e.to_string(),
                    error_details: Some(ProtocolError::action(e.to_string())),
                })
                .await
            {
                log::warn!("[{}] failed to send ActionError: {send_err}", ctx.name);
                return;
            }
        }
    }

    // Re-send entities after action, diffing against previous state
    // so that removed entities get EntityRemoved messages.
    send_all_entities(
        &*ctx.plugin,
        &ctx.tx,
        &ctx.name,
        &ctx.previous,
        ctx.omit_entity_type,
    )
    .await;
}

/// Send all current entities to the daemon, diffing against previous state.
///
/// Updates the shared `previous` map in place for the next diff cycle.
async fn send_all_entities<P: Plugin>(
    plugin: &P,
    tx: &mpsc::Sender<PluginMessage>,
    name: &str,
    previous: &Arc<Mutex<HashMap<String, serde_json::Value>>>,
    omit_entity_type: bool,
) {
    let entities = plugin.get_entities();
    let mut current: HashMap<String, serde_json::Value> = HashMap::new();

    let prev_snapshot = previous.lock().await.clone();

    for entity in &entities {
        let key = entity.urn.as_str().to_string();
        current.insert(key.clone(), entity.data.clone());

        // Only send if data changed or is new
        let changed = match prev_snapshot.get(&key) {
            Some(prev_data) => prev_data != &entity.data,
            None => true,
        };

        if changed {
            let msg = PluginMessage::EntityUpdated {
                urn: entity.urn.clone(),
                entity_type: (!omit_entity_type).then(|| entity.entity_type.clone()),
                data: entity.data.clone(),
            };
            if tx.send(msg).await.is_err() {
                log::warn!("[{name}] write channel closed during entity sync");
                *previous.lock().await = current;
                return;
            }
        }
    }

    // Send EntityRemoved for entities that no longer exist
    for prev_key in prev_snapshot.keys() {
        if !current.contains_key(prev_key) {
            let urn = match Urn::parse(prev_key) {
                Ok(u) => u,
                Err(e) => {
                    log::warn!("[{name}] failed to parse previous URN {prev_key}: {e}");
                    continue;
                }
            };
            let msg = PluginMessage::EntityRemoved {
                urn: urn.clone(),
                entity_type: (!omit_entity_type).then(|| urn.entity_type().to_string()),
            };
            if tx.send(msg).await.is_err() {
                log::warn!("[{name}] write channel closed during entity removal");
                *previous.lock().await = current;
                return;
            }
        }
    }

    *previous.lock().await = current;
}

/// Background task that writes queued messages to the socket.
async fn write_loop(
    name: String,
    mut writer: tokio::net::unix::OwnedWriteHalf,
    mut rx: mpsc::Receiver<PluginMessage>,
) {
    while let Some(msg) = rx.recv().await {
        if let Err(e) = write_framed(&mut writer, &msg).await {
            log::error!("[{name}] write error: {e}");
            break;
        }
    }
    log::info!("[{name}] write loop exited");
}

/// Get the daemon socket path.
///
/// Default: `$XDG_RUNTIME_DIR/waft/daemon.sock`
/// Override: `WAFT_DAEMON_SOCKET` environment variable
pub fn daemon_socket_path() -> PathBuf {
    if let Ok(custom) = std::env::var("WAFT_DAEMON_SOCKET") {
        return PathBuf::from(custom);
    }

    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        #[allow(unsafe_code)]
        let uid = unsafe { libc::getuid() };
        format!("/run/user/{uid}")
    });

    let mut path = PathBuf::from(runtime_dir);
    path.push("waft");
    path.push("daemon.sock");
    path
}
