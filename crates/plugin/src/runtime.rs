//! Plugin runtime: connects to the waft daemon and manages the event loop.
//!
//! The runtime is the core of a plugin process. It connects to the daemon's
//! Unix socket as a client, sends entity updates, and handles incoming
//! commands (actions, stop requests).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;
use waft_protocol::urn::Urn;
use waft_protocol::{PluginCommand, PluginMessage};

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

        let (read_half, write_half) = stream.into_split();

        // Spawn background write task with mpsc channel
        let (write_tx, write_rx) = mpsc::channel::<PluginMessage>(64);
        let name_for_writer = self.name.clone();
        tokio::spawn(write_loop(name_for_writer, write_half, write_rx));

        // Send initial entities
        let mut previous = send_all_entities(&*self.plugin, &write_tx, &self.name, HashMap::new()).await;

        // Event loop
        let mut notifier_rx = self.notifier_rx;
        let mut read_half = read_half;

        loop {
            tokio::select! {
                // Entity notifier fired — re-send entities
                changed = notifier_rx.changed() => {
                    if changed.is_err() {
                        log::info!("[{}] notifier dropped, shutting down", self.name);
                        break;
                    }
                    previous = send_all_entities(&*self.plugin, &write_tx, &self.name, previous).await;
                }

                // Incoming command from daemon
                msg = read_framed::<_, PluginCommand>(&mut read_half) => {
                    match msg {
                        Ok(Some(cmd)) => {
                            match cmd {
                                PluginCommand::TriggerAction { urn, action, action_id, params } => {
                                    let plugin = self.plugin.clone();
                                    let tx = write_tx.clone();
                                    let name = self.name.clone();
                                    tokio::spawn(async move {
                                        handle_action(plugin, &name, &tx, urn, action, action_id, params).await;
                                    });
                                }
                                PluginCommand::CanStop => {
                                    let can_stop = self.plugin.can_stop();
                                    if let Err(e) = write_tx.send(PluginMessage::StopResponse { can_stop }).await {
                                        log::warn!("[{}] failed to send StopResponse: {e}", self.name);
                                    }
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

/// Handle an incoming action: run plugin handler, send success/error, re-send entities.
async fn handle_action<P: Plugin>(
    plugin: Arc<P>,
    name: &str,
    tx: &mpsc::Sender<PluginMessage>,
    urn: Urn,
    action: String,
    action_id: Uuid,
    params: serde_json::Value,
) {
    match plugin.handle_action(urn, action, params).await {
        Ok(()) => {
            if let Err(e) = tx.send(PluginMessage::ActionSuccess { action_id }).await {
                log::warn!("[{name}] failed to send ActionSuccess: {e}");
                return;
            }
        }
        Err(e) => {
            log::error!("[{name}] action {action_id} failed: {e}");
            if let Err(send_err) = tx
                .send(PluginMessage::ActionError {
                    action_id,
                    error: e.to_string(),
                })
                .await
            {
                log::warn!("[{name}] failed to send ActionError: {send_err}");
                return;
            }
        }
    }

    // Re-send entities after action (state may have changed)
    let entities = plugin.get_entities();
    for entity in entities {
        let msg = PluginMessage::EntityUpdated {
            urn: entity.urn,
            entity_type: entity.entity_type,
            data: entity.data,
        };
        if tx.send(msg).await.is_err() {
            break;
        }
    }
}

/// Send all current entities to the daemon, diffing against previous state.
///
/// Returns the new state map for the next diff cycle.
async fn send_all_entities<P: Plugin>(
    plugin: &P,
    tx: &mpsc::Sender<PluginMessage>,
    name: &str,
    previous: HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let entities = plugin.get_entities();
    let mut current: HashMap<String, serde_json::Value> = HashMap::new();

    for entity in &entities {
        let key = entity.urn.as_str().to_string();
        current.insert(key.clone(), entity.data.clone());

        // Only send if data changed or is new
        let changed = match previous.get(&key) {
            Some(prev_data) => prev_data != &entity.data,
            None => true,
        };

        if changed {
            let msg = PluginMessage::EntityUpdated {
                urn: entity.urn.clone(),
                entity_type: entity.entity_type.clone(),
                data: entity.data.clone(),
            };
            if tx.send(msg).await.is_err() {
                log::warn!("[{name}] write channel closed during entity sync");
                return current;
            }
        }
    }

    // Send EntityRemoved for entities that no longer exist
    for (prev_key, _) in &previous {
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
                entity_type: urn.entity_type().to_string(),
            };
            if tx.send(msg).await.is_err() {
                log::warn!("[{name}] write channel closed during entity removal");
                return current;
            }
        }
    }

    current
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
        let uid = unsafe { libc::getuid() };
        format!("/run/user/{uid}")
    });

    let mut path = PathBuf::from(runtime_dir);
    path.push("waft");
    path.push("daemon.sock");
    path
}
