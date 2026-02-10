//! Plugin socket client for communicating with plugin daemons
//!
//! This module provides an event-driven client for connecting to plugin daemons
//! via Unix domain sockets. The write path uses a dedicated OS thread with
//! `std::sync::mpsc` so that sends from the GTK main thread wake immediately
//! via OS condvar, bypassing the tokio scheduler entirely. The read path stays
//! tokio-based since incoming I/O events wake it naturally.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use waft_ipc::transport::{write_framed, TransportError};
use waft_ipc::{Action, OverviewMessage, PluginMessage};

/// Default timeout for socket operations (5 seconds)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Errors that can occur during plugin client operations
#[derive(Debug)]
pub enum ClientError {
    /// Failed to connect to plugin socket
    ConnectionFailed(std::io::Error),

    /// Socket operation timed out
    Timeout,

    /// Transport/framing error
    Transport(TransportError),

    /// Plugin disconnected unexpectedly
    Disconnected,

    /// Invalid response from plugin
    InvalidResponse(String),

    /// Plugin socket does not exist
    SocketNotFound,

    /// Send channel closed
    SendFailed,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::ConnectionFailed(e) => write!(f, "connection failed: {}", e),
            ClientError::Timeout => write!(f, "operation timed out"),
            ClientError::Transport(e) => write!(f, "transport error: {}", e),
            ClientError::Disconnected => write!(f, "plugin disconnected"),
            ClientError::InvalidResponse(msg) => write!(f, "invalid response: {}", msg),
            ClientError::SocketNotFound => write!(f, "socket not found"),
            ClientError::SendFailed => write!(f, "send channel closed"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClientError::ConnectionFailed(e) => Some(e),
            ClientError::Transport(e) => Some(e),
            _ => None,
        }
    }
}

impl From<TransportError> for ClientError {
    fn from(e: TransportError) -> Self {
        ClientError::Transport(e)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ClientError::Disconnected
        } else {
            ClientError::ConnectionFailed(e)
        }
    }
}

/// Message from a plugin client's background read task
#[derive(Debug)]
pub enum InternalMessage {
    /// A plugin sent a message
    Plugin {
        plugin_id: String,
        msg: PluginMessage,
    },
    /// A plugin disconnected
    Disconnected { plugin_id: String },
}

/// Event-driven client for communicating with a plugin daemon via Unix socket.
///
/// The write path uses a dedicated OS thread (`std::sync::mpsc`) so that sends
/// from the GTK main thread wake the writer immediately via OS condvar. The read
/// path stays tokio-based since incoming I/O events wake it naturally.
pub struct PluginClient {
    write_tx: std::sync::mpsc::Sender<OverviewMessage>,
    plugin_name: String,
    socket_path: PathBuf,
}

impl PluginClient {
    /// Connect to a plugin daemon socket.
    ///
    /// Spawns a background read task that forwards incoming messages into `merged_tx`,
    /// and a dedicated OS writer thread for immediate action delivery.
    pub async fn connect(
        plugin_name: String,
        socket_path: PathBuf,
        merged_tx: mpsc::UnboundedSender<InternalMessage>,
    ) -> Result<Self, ClientError> {
        if !socket_path.exists() {
            return Err(ClientError::SocketNotFound);
        }

        // Connect with tokio for async timeout support
        let stream = timeout(DEFAULT_TIMEOUT, UnixStream::connect(&socket_path))
            .await
            .map_err(|_| ClientError::Timeout)?
            .map_err(ClientError::ConnectionFailed)?;

        // Convert to std for splitting into independent read/write handles
        let std_stream = stream
            .into_std()
            .map_err(ClientError::ConnectionFailed)?;
        let read_std = std_stream
            .try_clone()
            .map_err(ClientError::ConnectionFailed)?;

        // Read handle: convert back to tokio for async reading (sets O_NONBLOCK)
        let read_stream = UnixStream::from_std(read_std)
            .map_err(ClientError::ConnectionFailed)?;

        // Write handle: stays as std, used by dedicated OS thread
        // Set non-blocking so writes don't hang if buffer is somehow full
        std_stream
            .set_nonblocking(true)
            .map_err(ClientError::ConnectionFailed)?;
        let mut write_stream = std_stream;

        // Spawn writer OS thread: wakes immediately via condvar when GTK thread sends
        let (write_tx, write_rx) = std::sync::mpsc::channel::<OverviewMessage>();
        let writer_plugin_name = plugin_name.clone();
        std::thread::Builder::new()
            .name(format!("{}-writer", plugin_name))
            .spawn(move || {
                while let Ok(msg) = write_rx.recv() {
                    let mut buffer = Vec::new();
                    if write_framed(&mut buffer, &msg).is_err() {
                        break;
                    }
                    if write_with_poll(&mut write_stream, &buffer).is_err() {
                        log::warn!("[{}] write failed, stopping writer", writer_plugin_name);
                        break;
                    }
                }
                log::debug!("[{}] writer thread exiting", writer_plugin_name);
            })
            .map_err(|e| ClientError::ConnectionFailed(e))?;

        // Spawn read task: forwards incoming messages to merged channel
        let plugin_id_for_read = plugin_name.clone();
        tokio::spawn(async move {
            let mut read_stream = read_stream;
            loop {
                match read_plugin_message(&mut read_stream).await {
                    Ok(msg) => {
                        if merged_tx
                            .send(InternalMessage::Plugin {
                                plugin_id: plugin_id_for_read.clone(),
                                msg,
                            })
                            .is_err()
                        {
                            break; // merged channel closed
                        }
                    }
                    Err(_) => {
                        let _ = merged_tx.send(InternalMessage::Disconnected {
                            plugin_id: plugin_id_for_read.clone(),
                        });
                        break;
                    }
                }
            }
        });

        Ok(Self {
            write_tx,
            plugin_name,
            socket_path,
        })
    }

    /// Get the plugin name
    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Send a GetWidgets request. Response arrives via the merged channel.
    pub fn send_get_widgets(&self) -> Result<(), ClientError> {
        self.write_tx
            .send(OverviewMessage::GetWidgets)
            .map_err(|_| ClientError::SendFailed)
    }

    /// Send a TriggerAction request. Response arrives via the merged channel.
    pub fn send_action(&self, widget_id: String, action: Action) -> Result<(), ClientError> {
        self.write_tx
            .send(OverviewMessage::TriggerAction { widget_id, action })
            .map_err(|_| ClientError::SendFailed)
    }
}

/// Write a buffer to a non-blocking socket, using `libc::poll` to wait for
/// writability if the kernel buffer is full. For typical IPC messages (~100-200
/// bytes) this always succeeds on the first `write` call since the Unix socket
/// kernel buffer is 128KB+.
fn write_with_poll(stream: &mut std::os::unix::net::UnixStream, buf: &[u8]) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;

    let mut written = 0;
    while written < buf.len() {
        match stream.write(&buf[written..]) {
            Ok(n) => written += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Wait for socket to become writable
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

/// Read a framed PluginMessage from an async reader.
async fn read_plugin_message(
    reader: &mut (impl AsyncReadExt + Unpin),
) -> Result<PluginMessage, ClientError> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;
    if len > MAX_FRAME_SIZE {
        return Err(ClientError::Transport(TransportError::FrameTooLarge(len)));
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;

    let msg: PluginMessage = serde_json::from_slice(&payload)
        .map_err(|e| ClientError::Transport(TransportError::Serialization(e)))?;

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_client_error_display() {
        let err = ClientError::Timeout;
        assert_eq!(err.to_string(), "operation timed out");

        let err = ClientError::SocketNotFound;
        assert_eq!(err.to_string(), "socket not found");

        let err = ClientError::Disconnected;
        assert_eq!(err.to_string(), "plugin disconnected");

        let err = ClientError::InvalidResponse("test".to_string());
        assert!(err.to_string().contains("invalid response"));

        let err = ClientError::SendFailed;
        assert_eq!(err.to_string(), "send channel closed");
    }

    #[test]
    fn test_client_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "test");
        let client_err: ClientError = io_err.into();

        match client_err {
            ClientError::Disconnected => {}
            _ => panic!("Expected Disconnected variant"),
        }

        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "test");
        let client_err: ClientError = io_err.into();

        match client_err {
            ClientError::ConnectionFailed(_) => {}
            _ => panic!("Expected ConnectionFailed variant"),
        }
    }

    #[tokio::test]
    async fn test_connect_nonexistent_socket() {
        let socket_path = PathBuf::from("/tmp/nonexistent_plugin.sock");
        let (tx, _rx) = mpsc::unbounded_channel();
        let result = PluginClient::connect("test".to_string(), socket_path, tx).await;

        assert!(matches!(result, Err(ClientError::SocketNotFound)));
    }

    #[test]
    fn test_timeout_constants() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(5));
    }
}
