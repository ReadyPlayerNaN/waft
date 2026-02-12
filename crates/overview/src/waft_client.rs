//! Client for communicating with the central waft daemon.
//!
//! `WaftClient` replaces the old per-plugin `PluginManager` + `PluginClient`
//! model. Instead of N individual plugin sockets, it connects to ONE central
//! daemon socket and uses entity-based subscriptions.
//!
//! The write path uses a dedicated OS thread with `std::sync::mpsc` so that
//! sends from the GTK main thread wake immediately via OS condvar, bypassing
//! the tokio scheduler entirely. The read path is a tokio task that forwards
//! `AppNotification` messages via a flume channel into glib context.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;
use tokio::time::timeout;
use uuid::Uuid;
use waft_protocol::transport::write_framed;
use waft_protocol::{AppMessage, AppNotification, TransportError};

/// Default timeout for a single connection attempt (5 seconds).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum allowed frame size (10 MB), matching waft_protocol.
const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// Initial retry delay for exponential backoff.
const RETRY_INITIAL_DELAY: Duration = Duration::from_millis(100);

/// Maximum retry delay cap.
const RETRY_MAX_DELAY: Duration = Duration::from_secs(10);

/// Maximum number of retry attempts before giving up.
const RETRY_MAX_ATTEMPTS: u32 = 10;

/// D-Bus well-known name of the waft daemon.
const DAEMON_DBUS_NAME: &str = "org.waft.Daemon";

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
    /// Write channel closed.
    SendFailed,
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
            WaftClientError::SendFailed => write!(f, "write channel closed"),
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

/// Client for the central waft daemon.
///
/// Safe to call `subscribe`, `unsubscribe`, and `trigger_action` from the GTK
/// main thread — all writes go through a dedicated OS thread.
pub struct WaftClient {
    write_tx: std::sync::mpsc::Sender<AppMessage>,
}

impl WaftClient {
    /// Connect to the waft daemon with retry and D-Bus activation.
    ///
    /// If the daemon socket is not available, requests D-Bus activation for
    /// `org.waft.Daemon` and retries with exponential backoff (100ms to 10s,
    /// up to 10 attempts). This handles the case where the overview starts
    /// before the daemon is ready.
    ///
    /// Returns a `(WaftClient, flume::Receiver<AppNotification>)` pair.
    pub async fn connect_with_retry(
    ) -> Result<(Self, flume::Receiver<AppNotification>), WaftClientError> {
        let mut delay = RETRY_INITIAL_DELAY;
        let mut activation_requested = false;

        for attempt in 1..=RETRY_MAX_ATTEMPTS {
            match Self::connect().await {
                Ok(result) => return Ok(result),
                Err(WaftClientError::SocketNotFound) | Err(WaftClientError::ConnectionFailed(_))
                    if attempt < RETRY_MAX_ATTEMPTS =>
                {
                    // Request D-Bus activation on first failure to auto-start the daemon
                    if !activation_requested {
                        activation_requested = true;
                        if let Err(e) = request_dbus_activation().await {
                            log::warn!("[waft-client] D-Bus activation failed: {e}");
                        } else {
                            log::info!("[waft-client] requested D-Bus activation for {DAEMON_DBUS_NAME}");
                        }
                    }

                    log::debug!(
                        "[waft-client] connection attempt {attempt}/{RETRY_MAX_ATTEMPTS} failed, retrying in {}ms",
                        delay.as_millis()
                    );
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(RETRY_MAX_DELAY);
                }
                Err(e) => return Err(e),
            }
        }

        // Final attempt (already exhausted retries above)
        Self::connect().await
    }

    /// Connect to the waft daemon socket.
    ///
    /// Returns a `(WaftClient, flume::Receiver<AppNotification>)` pair. The
    /// receiver delivers notifications from the daemon and should be consumed
    /// in `glib::spawn_future_local`.
    pub async fn connect() -> Result<(Self, flume::Receiver<AppNotification>), WaftClientError> {
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
            .name("waft-daemon-writer".to_string())
            .spawn(move || {
                while let Ok(msg) = write_rx.recv() {
                    let mut buffer = Vec::new();
                    if write_framed(&mut buffer, &msg).is_err() {
                        log::warn!("[waft-client] serialization failed, stopping writer");
                        break;
                    }
                    if write_with_poll(&mut write_stream, &buffer).is_err() {
                        log::warn!("[waft-client] write failed, stopping writer");
                        break;
                    }
                }
                log::debug!("[waft-client] writer thread exiting");
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
                        log::info!("[waft-client] daemon disconnected");
                        break;
                    }
                    Err(e) => {
                        log::warn!("[waft-client] read error: {e}");
                        break;
                    }
                }
            }
            log::debug!("[waft-client] read task stopped");
        });

        Ok((Self { write_tx }, notification_rx))
    }

    /// Subscribe to updates for an entity type.
    ///
    /// Safe to call from the GTK main thread.
    pub fn subscribe(&self, entity_type: &str) {
        let msg = AppMessage::Subscribe {
            entity_type: entity_type.to_string(),
        };
        if let Err(e) = self.write_tx.send(msg) {
            log::warn!("[waft-client] failed to send Subscribe: {e}");
        }
    }

    /// Unsubscribe from updates for an entity type.
    ///
    /// Safe to call from the GTK main thread.
    pub fn unsubscribe(&self, entity_type: &str) {
        let msg = AppMessage::Unsubscribe {
            entity_type: entity_type.to_string(),
        };
        if let Err(e) = self.write_tx.send(msg) {
            log::warn!("[waft-client] failed to send Unsubscribe: {e}");
        }
    }

    /// Trigger an action on a specific entity.
    ///
    /// Returns the action ID that can be matched against `ActionSuccess`/`ActionError`
    /// notifications from the daemon.
    ///
    /// Safe to call from the GTK main thread.
    pub fn trigger_action(
        &self,
        urn: waft_protocol::Urn,
        action: &str,
        params: serde_json::Value,
    ) -> Uuid {
        let action_id = Uuid::new_v4();
        let msg = AppMessage::TriggerAction {
            urn,
            action: action.to_string(),
            action_id,
            params,
            timeout_ms: None,
        };
        if let Err(e) = self.write_tx.send(msg) {
            log::warn!("[waft-client] failed to send TriggerAction: {e}");
        }
        action_id
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
///
/// Asks the D-Bus broker to start the service identified by `org.waft.Daemon`.
/// The broker looks up the corresponding `.service` file and spawns the binary.
async fn request_dbus_activation() -> Result<(), Box<dyn std::error::Error>> {
    let conn = zbus::Connection::session().await?;
    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn).await?;

    // StartServiceByName(name, flags) -> returns a status code
    // flags=0 means no special flags
    dbus_proxy
        .start_service_by_name(
            DAEMON_DBUS_NAME.try_into()?,
            0,
        )
        .await?;

    Ok(())
}

/// Write a buffer to a non-blocking socket, using `libc::poll` to wait for
/// writability if the kernel buffer is full.
fn write_with_poll(stream: &mut std::os::unix::net::UnixStream, buf: &[u8]) -> std::io::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = WaftClientError::Timeout;
        assert_eq!(err.to_string(), "connection timed out");

        let err = WaftClientError::SocketNotFound;
        assert_eq!(err.to_string(), "daemon socket not found");

        let err = WaftClientError::Disconnected;
        assert_eq!(err.to_string(), "daemon disconnected");

        let err = WaftClientError::SendFailed;
        assert_eq!(err.to_string(), "write channel closed");
    }

    #[test]
    fn daemon_socket_path_from_env() {
        // This test just validates the path construction logic.
        // In real use, XDG_RUNTIME_DIR is always set on Linux.
        let path = daemon_socket_path();
        if std::env::var("XDG_RUNTIME_DIR").is_ok() {
            let path = path.unwrap();
            assert!(path.to_string_lossy().contains("waft/daemon.sock"));
        } else {
            assert!(matches!(path, Err(WaftClientError::SocketNotFound)));
        }
    }

    #[test]
    fn socket_not_found_when_no_runtime_dir() {
        // Verify that a missing runtime dir yields SocketNotFound
        let saved = std::env::var("XDG_RUNTIME_DIR").ok();
        // Safety: test-only, single-threaded context
        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
        let result = daemon_socket_path();
        if let Some(dir) = saved {
            // Safety: restoring the original value
            unsafe { std::env::set_var("XDG_RUNTIME_DIR", dir) };
        }
        assert!(matches!(result, Err(WaftClientError::SocketNotFound)));
    }

    #[test]
    fn retry_backoff_caps_at_max_delay() {
        // Verify that exponential backoff doubles each time and caps at RETRY_MAX_DELAY
        let mut delay = RETRY_INITIAL_DELAY;
        let delays: Vec<Duration> = (0..15)
            .map(|_| {
                let current = delay;
                delay = (delay * 2).min(RETRY_MAX_DELAY);
                current
            })
            .collect();

        assert_eq!(delays[0], Duration::from_millis(100));
        assert_eq!(delays[1], Duration::from_millis(200));
        assert_eq!(delays[2], Duration::from_millis(400));
        assert_eq!(delays[3], Duration::from_millis(800));
        assert_eq!(delays[4], Duration::from_millis(1600));
        assert_eq!(delays[5], Duration::from_millis(3200));
        assert_eq!(delays[6], Duration::from_millis(6400));
        // After 6400ms * 2 = 12800ms > 10s cap
        assert_eq!(delays[7], RETRY_MAX_DELAY);
        assert_eq!(delays[8], RETRY_MAX_DELAY);
    }
}
