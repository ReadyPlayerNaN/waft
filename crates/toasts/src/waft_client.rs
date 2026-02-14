//! Client for communicating with the central waft daemon.
//!
//! This is a simplified version of the overview's WaftClient, tailored for
//! the toasts app which only needs to subscribe to notification and DND entities.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;
use tokio::time::timeout;
use uuid::Uuid;
use waft_protocol::transport::write_framed;
use waft_protocol::{AppMessage, AppNotification, TransportError, Urn};

/// Default timeout for a single connection attempt (5 seconds).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum allowed frame size (10 MB), matching waft_protocol.
const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// D-Bus well-known name of the waft daemon.
const DAEMON_DBUS_NAME: &str = "org.waft.Daemon";

/// Delay between reconnection attempts after the daemon disconnects.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);

/// Entity types the toasts app subscribes to.
const ENTITY_TYPES: &[&str] = &[
    waft_protocol::entity::notification::NOTIFICATION_ENTITY_TYPE,
    waft_protocol::entity::notification::DND_ENTITY_TYPE,
];

/// Errors that can occur during WaftClient operations.
#[derive(Debug)]
pub enum WaftClientError {
    /// Failed to connect to daemon socket.
    ConnectionFailed(std::io::Error),
    /// Connection attempt timed out.
    Timeout,
    /// Transport framing error.
    Transport(TransportError),
    /// Daemon disconnected.
    Disconnected,
    /// Daemon socket does not exist.
    SocketNotFound,
}

impl std::fmt::Display for WaftClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaftClientError::ConnectionFailed(e) => write!(f, "connection failed: {e}"),
            WaftClientError::Timeout => write!(f, "connection timed out"),
            WaftClientError::Transport(e) => write!(f, "transport error: {e}"),
            WaftClientError::Disconnected => write!(f, "daemon disconnected"),
            WaftClientError::SocketNotFound => write!(f, "daemon socket not found"),
        }
    }
}

impl std::error::Error for WaftClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WaftClientError::ConnectionFailed(e) => Some(e),
            WaftClientError::Transport(e) => Some(e),
            _ => None,
        }
    }
}

/// Events delivered to the toasts app's glib main loop.
#[derive(Debug, Clone)]
pub enum ToastEvent {
    /// Successfully connected (or reconnected) to the daemon.
    Connected,
    /// Lost connection to the daemon.
    Disconnected,
    /// Entity updated notification.
    EntityUpdated {
        urn: Urn,
        entity_type: String,
        data: serde_json::Value,
    },
    /// Entity removed notification.
    EntityRemoved { urn: Urn, entity_type: String },
    /// Entity marked as stale (plugin crashed/restarted).
    EntityStale { urn: Urn, entity_type: String },
}

/// Client for the central waft daemon.
///
/// Safe to call `trigger_action` from the GTK main thread — all writes go
/// through a dedicated OS thread.
pub struct WaftClient {
    write_tx: std::sync::mpsc::Sender<AppMessage>,
}

impl WaftClient {
    /// Connect to the waft daemon socket.
    async fn connect() -> Result<(Self, flume::Receiver<AppNotification>), WaftClientError> {
        let socket_path = daemon_socket_path()?;

        if !socket_path.exists() {
            return Err(WaftClientError::SocketNotFound);
        }

        // Connect with tokio for async timeout support
        let stream = timeout(CONNECT_TIMEOUT, UnixStream::connect(&socket_path))
            .await
            .map_err(|_| WaftClientError::Timeout)?
            .map_err(WaftClientError::ConnectionFailed)?;

        // Convert to std for splitting into independent read/write handles
        let std_stream = stream
            .into_std()
            .map_err(WaftClientError::ConnectionFailed)?;
        let read_std = std_stream
            .try_clone()
            .map_err(WaftClientError::ConnectionFailed)?;

        // Read handle: convert back to tokio for async reading
        let read_stream =
            UnixStream::from_std(read_std).map_err(WaftClientError::ConnectionFailed)?;

        // Write handle: stays as std, used by dedicated OS thread
        std_stream
            .set_nonblocking(true)
            .map_err(WaftClientError::ConnectionFailed)?;
        let mut write_stream = std_stream;

        // Spawn writer OS thread: wakes immediately via condvar when GTK thread sends
        let (write_tx, write_rx) = std::sync::mpsc::channel::<AppMessage>();
        std::thread::Builder::new()
            .name("waft-toasts-writer".to_string())
            .spawn(move || {
                while let Ok(msg) = write_rx.recv() {
                    let mut buffer = Vec::new();
                    if write_framed(&mut buffer, &msg).is_err() {
                        log::warn!("[waft-toasts] serialization failed, stopping writer");
                        break;
                    }
                    if write_with_poll(&mut write_stream, &buffer).is_err() {
                        log::warn!("[waft-toasts] write failed, stopping writer");
                        break;
                    }
                }
                log::debug!("[waft-toasts] writer thread exiting");
            })
            .map_err(WaftClientError::ConnectionFailed)?;

        // Spawn read task: forwards incoming notifications via flume
        let (notification_tx, notification_rx) = flume::unbounded::<AppNotification>();
        tokio::spawn(async move {
            let mut read_stream = read_stream;
            loop {
                match read_notification(&mut read_stream).await {
                    Ok(notification) => {
                        if notification_tx.send(notification).is_err() {
                            break; // receiver dropped
                        }
                    }
                    Err(WaftClientError::Disconnected) => {
                        log::info!("[waft-toasts] daemon disconnected");
                        break;
                    }
                    Err(e) => {
                        log::warn!("[waft-toasts] read error: {e}");
                        break;
                    }
                }
            }
            log::debug!("[waft-toasts] read task stopped");
        });

        Ok((Self { write_tx }, notification_rx))
    }

    /// Subscribe to updates for an entity type.
    fn subscribe(&self, entity_type: &str) {
        let msg = AppMessage::Subscribe {
            entity_type: entity_type.to_string(),
        };
        if let Err(e) = self.write_tx.send(msg) {
            log::warn!("[waft-toasts] failed to send Subscribe: {e}");
        }
    }

    /// Request cached entity state for an entity type.
    fn request_status(&self, entity_type: &str) {
        let msg = AppMessage::Status {
            entity_type: entity_type.to_string(),
        };
        if let Err(e) = self.write_tx.send(msg) {
            log::warn!("[waft-toasts] failed to send Status: {e}");
        }
    }

    /// Trigger an action on a specific entity.
    ///
    /// Safe to call from the GTK main thread.
    pub fn trigger_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Uuid {
        let action_id = Uuid::new_v4();
        let msg = AppMessage::TriggerAction {
            urn,
            action,
            action_id,
            params,
            timeout_ms: None,
        };
        if let Err(e) = self.write_tx.send(msg) {
            log::warn!("[waft-toasts] failed to send TriggerAction: {e}");
        }
        action_id
    }
}

/// Long-running tokio task that manages the daemon connection lifecycle.
pub async fn daemon_connection_task(
    event_tx: flume::Sender<ToastEvent>,
    client_handle: Arc<std::sync::Mutex<Option<WaftClient>>>,
) {
    let mut activation_requested = false;

    loop {
        // Request D-Bus activation on first attempt to auto-start the daemon
        if !activation_requested {
            activation_requested = true;
            if let Err(e) = request_dbus_activation().await {
                log::warn!("[waft-toasts] D-Bus activation failed: {e}");
            } else {
                log::info!(
                    "[waft-toasts] requested D-Bus activation for {DAEMON_DBUS_NAME}"
                );
            }
        }

        match WaftClient::connect().await {
            Ok((client, notification_rx)) => {
                log::info!("[waft-toasts] connected to daemon");

                // Subscribe to notification and DND entity types
                for et in ENTITY_TYPES {
                    client.subscribe(et);
                }
                for et in ENTITY_TYPES {
                    client.request_status(et);
                }
                log::info!(
                    "[waft-toasts] subscribed to {} entity types",
                    ENTITY_TYPES.len()
                );

                // Store client for write path (actions from GTK thread)
                match client_handle.lock() {
                    Ok(mut guard) => *guard = Some(client),
                    Err(e) => {
                        log::warn!("[waft-toasts] client handle poisoned: {e}");
                        *e.into_inner() = Some(client);
                    }
                }

                // Signal connected
                if event_tx.send(ToastEvent::Connected).is_err() {
                    log::debug!("[waft-toasts] app closed, stopping connection task");
                    return;
                }

                // Reset activation flag for next reconnect cycle
                activation_requested = false;

                // Forward notifications until disconnect
                while let Ok(notification) = notification_rx.recv_async().await {
                    let event = match notification {
                        AppNotification::EntityUpdated {
                            urn,
                            entity_type,
                            data,
                        } => ToastEvent::EntityUpdated {
                            urn,
                            entity_type,
                            data,
                        },
                        AppNotification::EntityRemoved { urn, entity_type } => {
                            ToastEvent::EntityRemoved { urn, entity_type }
                        }
                        AppNotification::EntityStale { urn, entity_type } => {
                            ToastEvent::EntityStale { urn, entity_type }
                        }
                        _ => continue, // Ignore other notifications
                    };

                    if event_tx.send(event).is_err() {
                        log::debug!("[waft-toasts] app closed, stopping connection task");
                        return;
                    }
                }

                // Notification channel closed = daemon disconnected
                log::info!("[waft-toasts] daemon disconnected, will retry");

                // Clear write path so actions are dropped during disconnect
                match client_handle.lock() {
                    Ok(mut guard) => *guard = None,
                    Err(e) => {
                        log::warn!("[waft-toasts] client handle poisoned: {e}");
                        *e.into_inner() = None;
                    }
                }

                // Signal disconnected
                if event_tx.send(ToastEvent::Disconnected).is_err() {
                    log::debug!("[waft-toasts] app closed, stopping connection task");
                    return;
                }
            }
            Err(e) => {
                log::debug!("[waft-toasts] connection attempt failed: {e}");
            }
        }

        tokio::time::sleep(RECONNECT_INTERVAL).await;
    }
}

/// Resolve the daemon socket path from `$XDG_RUNTIME_DIR/waft/daemon.sock`.
fn daemon_socket_path() -> Result<PathBuf, WaftClientError> {
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").map_err(|_| WaftClientError::SocketNotFound)?;
    let mut path = PathBuf::from(runtime_dir);
    path.push("waft");
    path.push("daemon.sock");
    Ok(path)
}

/// Request D-Bus activation for the waft daemon.
async fn request_dbus_activation() -> Result<(), Box<dyn std::error::Error>> {
    let conn = zbus::Connection::session().await?;
    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn).await?;

    dbus_proxy
        .start_service_by_name(DAEMON_DBUS_NAME.try_into()?, 0)
        .await?;

    Ok(())
}

/// Write a buffer to a non-blocking socket, using `libc::poll` to wait for
/// writability if the kernel buffer is full.
fn write_with_poll(
    stream: &mut std::os::unix::net::UnixStream,
    buf: &[u8],
) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;

    let mut written = 0;
    while written < buf.len() {
        match stream.write(&buf[written..]) {
            Ok(n) => written += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let mut pollfd = libc::pollfd {
                    fd: stream.as_raw_fd(),
                    events: libc::POLLOUT,
                    revents: 0,
                };
                // Safety: single pollfd, valid fd, bounded timeout
                let ret = unsafe { libc::poll(&mut pollfd, 1, 5000) };
                if ret < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if ret == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "poll timeout waiting for socket writability",
                    ));
                }
            }
            Err(e) => return Err(e),
        }
    }
    stream.flush()
}

/// Read a framed `AppNotification` from the async reader.
async fn read_notification(
    reader: &mut (impl AsyncReadExt + Unpin),
) -> Result<AppNotification, WaftClientError> {
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(WaftClientError::Disconnected);
        }
        Err(e) => return Err(WaftClientError::ConnectionFailed(e)),
    }

    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(WaftClientError::Transport(TransportError::FrameTooLarge(
            len,
        )));
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            WaftClientError::Disconnected
        } else {
            WaftClientError::ConnectionFailed(e)
        }
    })?;

    let notification: AppNotification = serde_json::from_slice(&payload)
        .map_err(|e| WaftClientError::Transport(TransportError::Serialization(e)))?;

    Ok(notification)
}
