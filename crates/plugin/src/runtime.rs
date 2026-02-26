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

        let (read_half, write_half) = stream.into_split();

        // Spawn background write task with mpsc channel
        let (write_tx, write_rx) = mpsc::channel::<PluginMessage>(64);
        let name_for_writer = self.name.clone();
        tokio::spawn(write_loop(name_for_writer, write_half, write_rx));

        // Shared previous-entity state for diffing (used by both the main loop and handle_action)
        let previous: Arc<Mutex<HashMap<String, serde_json::Value>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Claim check channel: plugin -> runtime.
        //
        // We pass a *clone* of the sender to the plugin and retain the original
        // in this scope. Plugins with a no-op set_claim_sender() drop the
        // ClaimSender immediately; without this keepalive the channel would
        // close instantly and claim_rx.recv() would return None on every poll
        // iteration, causing the select! loop below to busy-spin at 100% CPU.
        let (claim_tx, mut claim_rx) =
            tokio::sync::mpsc::channel::<crate::claim::ClaimRequest>(16);
        self.plugin
            .set_claim_sender(crate::claim::ClaimSender::new(claim_tx.clone()));

        // Send initial entities
        send_all_entities(&*self.plugin, &write_tx, &self.name, &previous).await;

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
                    send_all_entities(&*self.plugin, &write_tx, &self.name, &previous).await;
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
                                PluginCommand::ClaimResult { urn, claim_id, claimed } => {
                                    let ctx = ActionContext {
                                        plugin: self.plugin.clone(),
                                        tx: write_tx.clone(),
                                        name: self.name.clone(),
                                        previous: previous.clone(),
                                    };
                                    tokio::spawn(async move {
                                        ctx.plugin.handle_claim_result(urn, claim_id, claimed).await;
                                        send_all_entities(&*ctx.plugin, &ctx.tx, &ctx.name, &ctx.previous).await;
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

                // Plugin requested a claim check
                claim_req = claim_rx.recv() => {
                    if let Some(req) = claim_req {
                        let msg = PluginMessage::ClaimCheck {
                            urn: req.urn,
                            claim_id: req.claim_id,
                        };
                        if write_tx.send(msg).await.is_err() {
                            log::warn!("[{}] write channel closed sending ClaimCheck", self.name);
                            break;
                        }
                    } else {
                        log::debug!("[{}] claim channel closed", self.name);
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
        Ok(()) => {
            if let Err(e) = ctx
                .tx
                .send(PluginMessage::ActionSuccess { action_id })
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
    send_all_entities(&*ctx.plugin, &ctx.tx, &ctx.name, &ctx.previous).await;
}

/// Send all current entities to the daemon, diffing against previous state.
///
/// Updates the shared `previous` map in place for the next diff cycle.
async fn send_all_entities<P: Plugin>(
    plugin: &P,
    tx: &mpsc::Sender<PluginMessage>,
    name: &str,
    previous: &Arc<Mutex<HashMap<String, serde_json::Value>>>,
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
                entity_type: entity.entity_type.clone(),
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
                entity_type: urn.entity_type().to_string(),
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use waft_protocol::urn::Urn;

    use crate::claim::{ClaimRequest, ClaimSender};
    use crate::plugin::{Entity, Plugin};

    struct NoOpPlugin;

    #[async_trait::async_trait]
    impl Plugin for NoOpPlugin {
        fn get_entities(&self) -> Vec<Entity> {
            vec![]
        }

        async fn handle_action(
            &self,
            _urn: Urn,
            _action: String,
            _params: serde_json::Value,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    /// Regression test for the 100% CPU busy-spin caused by a prematurely
    /// closed claim channel.
    ///
    /// The bug: `claim_tx` was moved into `ClaimSender::new(claim_tx)` and
    /// passed to the plugin. The default no-op `set_claim_sender()` drops it
    /// immediately, closing the channel. `claim_rx.recv()` then returned `None`
    /// on every poll. The `else` branch in the `select!` loop logged but did
    /// not `break`, so the runtime spun at 100% CPU with zero syscalls.
    ///
    /// The fix: the runtime passes a clone to the plugin and retains the
    /// original `claim_tx`, keeping the channel open for the entire duration of
    /// `run()`. This test verifies that `claim_rx.recv()` properly awaits after
    /// a no-op `set_claim_sender()` rather than returning `None` immediately.
    #[tokio::test]
    async fn claim_channel_stays_open_after_noop_set_claim_sender() {
        let plugin = Arc::new(NoOpPlugin);

        let (claim_tx, mut claim_rx) = tokio::sync::mpsc::channel::<ClaimRequest>(16);

        // Replicate the fixed pattern from PluginRuntime::run(): pass a clone
        // to the plugin and retain the original as a keepalive.
        plugin.set_claim_sender(ClaimSender::new(claim_tx.clone()));

        // NoOpPlugin drops the ClaimSender immediately. The retained claim_tx
        // must keep the channel open so recv() properly awaits instead of
        // returning None (which would trigger a busy-spin in the select! loop).
        let timed_out = tokio::time::timeout(
            std::time::Duration::from_millis(20),
            claim_rx.recv(),
        )
        .await
        .is_err();

        assert!(
            timed_out,
            "claim_rx.recv() returned immediately — channel was closed prematurely; \
             the keepalive is broken and the runtime would busy-spin"
        );

        // Dropping the keepalive closes the channel cleanly.
        drop(claim_tx);
        assert!(
            claim_rx.recv().await.is_none(),
            "channel should be closed after the keepalive is dropped"
        );
    }
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
