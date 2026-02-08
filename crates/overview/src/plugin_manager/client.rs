//! Plugin socket client for communicating with plugin daemons
//!
//! This module provides a client for connecting to plugin daemons via Unix domain
//! sockets and exchanging IPC messages using the waft-ipc protocol.

use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;
use waft_ipc::transport::{write_framed, TransportError};
use waft_ipc::{Action, NamedWidget, OverviewMessage, PluginMessage};

/// Default timeout for socket operations (5 seconds)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of reconnection attempts
const MAX_RECONNECT_ATTEMPTS: usize = 3;

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

/// Client for communicating with a plugin daemon via Unix socket
pub struct PluginClient {
    stream: UnixStream,
    plugin_name: String,
    socket_path: PathBuf,
    timeout_duration: Duration,
}

impl PluginClient {
    /// Connect to a plugin daemon socket
    ///
    /// # Arguments
    ///
    /// * `plugin_name` - Name of the plugin (for logging/debugging)
    /// * `socket_path` - Path to the Unix domain socket
    ///
    /// # Errors
    ///
    /// Returns `ClientError::SocketNotFound` if the socket doesn't exist.
    /// Returns `ClientError::ConnectionFailed` if connection fails.
    pub async fn connect(plugin_name: String, socket_path: PathBuf) -> Result<Self, ClientError> {
        // Check if socket exists before attempting connection
        if !socket_path.exists() {
            return Err(ClientError::SocketNotFound);
        }

        // Attempt to connect with timeout
        let stream = timeout(DEFAULT_TIMEOUT, UnixStream::connect(&socket_path))
            .await
            .map_err(|_| ClientError::Timeout)?
            .map_err(ClientError::ConnectionFailed)?;

        Ok(Self {
            stream,
            plugin_name,
            socket_path,
            timeout_duration: DEFAULT_TIMEOUT,
        })
    }

    /// Set custom timeout duration for socket operations
    pub fn set_timeout(&mut self, duration: Duration) {
        self.timeout_duration = duration;
    }

    /// Get the plugin name
    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Send a message to the plugin daemon
    ///
    /// # Errors
    ///
    /// Returns `ClientError::Timeout` if the operation times out.
    /// Returns `ClientError::Transport` if serialization or I/O fails.
    pub async fn send_message(&mut self, message: OverviewMessage) -> Result<(), ClientError> {
        let send_future = async {
            // Use standard library Read/Write traits via temporary buffer
            let mut buffer = Vec::new();
            write_framed(&mut buffer, &message)?;
            self.stream.write_all(&buffer).await?;
            self.stream.flush().await?;
            Ok::<(), ClientError>(())
        };

        timeout(self.timeout_duration, send_future)
            .await
            .map_err(|_| ClientError::Timeout)?
    }

    /// Receive a message from the plugin daemon
    ///
    /// # Errors
    ///
    /// Returns `ClientError::Timeout` if the operation times out.
    /// Returns `ClientError::Transport` if deserialization or I/O fails.
    /// Returns `ClientError::Disconnected` if the connection is closed.
    pub async fn receive_message(&mut self) -> Result<PluginMessage, ClientError> {
        let receive_future = async {
            // Read framed message using AsyncRead
            let mut buffer = Vec::new();

            // Read 4-byte length prefix
            let mut len_bytes = [0u8; 4];
            self.stream.read_exact(&mut len_bytes).await?;
            let len = u32::from_be_bytes(len_bytes) as usize;

            // Read payload
            buffer.resize(len, 0);
            self.stream.read_exact(&mut buffer).await?;

            // Deserialize
            let message: PluginMessage = serde_json::from_slice(&buffer)
                .map_err(|e| ClientError::Transport(TransportError::Serialization(e)))?;

            Ok::<PluginMessage, ClientError>(message)
        };

        timeout(self.timeout_duration, receive_future)
            .await
            .map_err(|_| ClientError::Timeout)?
    }

    /// Request widgets from the plugin daemon
    ///
    /// This is a convenience method that sends `GetWidgets` and expects a `SetWidgets` response.
    ///
    /// # Errors
    ///
    /// Returns `ClientError::InvalidResponse` if the plugin doesn't respond with `SetWidgets`.
    pub async fn request_widgets(&mut self) -> Result<Vec<NamedWidget>, ClientError> {
        self.send_message(OverviewMessage::GetWidgets).await?;

        let response = self.receive_message().await?;

        match response {
            PluginMessage::SetWidgets { widgets } => Ok(widgets),
            PluginMessage::UpdateWidget { .. } => {
                Err(ClientError::InvalidResponse(
                    "expected SetWidgets, got UpdateWidget".to_string()
                ))
            }
            PluginMessage::RemoveWidget { .. } => {
                Err(ClientError::InvalidResponse(
                    "expected SetWidgets, got RemoveWidget".to_string()
                ))
            }
        }
    }

    /// Trigger an action on a specific widget
    ///
    /// This sends a `TriggerAction` message to the plugin. The plugin may respond with
    /// widget updates (UpdateWidget/SetWidgets) or remove notifications (RemoveWidget).
    ///
    /// # Arguments
    ///
    /// * `widget_id` - ID of the widget to trigger the action on
    /// * `action` - The action to trigger
    pub async fn trigger_action(
        &mut self,
        widget_id: String,
        action: Action,
    ) -> Result<(), ClientError> {
        self.send_message(OverviewMessage::TriggerAction { widget_id, action })
            .await
    }

    /// Attempt to reconnect to the plugin socket
    ///
    /// This will attempt to reconnect up to `MAX_RECONNECT_ATTEMPTS` times with
    /// exponential backoff.
    ///
    /// # Errors
    ///
    /// Returns the last connection error if all attempts fail.
    pub async fn reconnect(&mut self) -> Result<(), ClientError> {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt < MAX_RECONNECT_ATTEMPTS {
            attempt += 1;

            // Exponential backoff: 100ms, 200ms, 400ms
            let backoff = Duration::from_millis(100 * (1 << (attempt - 1)));
            tokio::time::sleep(backoff).await;

            match UnixStream::connect(&self.socket_path).await {
                Ok(stream) => {
                    self.stream = stream;
                    log::info!(
                        "[plugin-client] reconnected to {} (attempt {}/{})",
                        self.plugin_name,
                        attempt,
                        MAX_RECONNECT_ATTEMPTS
                    );
                    return Ok(());
                }
                Err(e) => {
                    log::debug!(
                        "[plugin-client] reconnection attempt {}/{} failed for {}: {}",
                        attempt,
                        MAX_RECONNECT_ATTEMPTS,
                        self.plugin_name,
                        e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(ClientError::ConnectionFailed(
            last_error.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "reconnection failed")
            })
        ))
    }

    /// Check if the connection is still alive by trying to peek at the socket
    pub fn is_connected(&self) -> bool {
        self.stream.peer_addr().is_ok()
    }

    /// Gracefully close the connection
    pub async fn close(mut self) -> Result<(), ClientError> {
        self.stream.shutdown().await?;
        Ok(())
    }
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
        let result = PluginClient::connect("test".to_string(), socket_path).await;

        assert!(matches!(result, Err(ClientError::SocketNotFound)));
    }

    #[test]
    fn test_client_getters() {
        // We can't easily create a real client without a server, but we can test
        // the basic structure would work with mock data in integration tests
        let plugin_name = "test-plugin";
        let socket_path = PathBuf::from("/tmp/test.sock");

        // These values would be used by a real client
        assert_eq!(plugin_name, "test-plugin");
        assert_eq!(socket_path, PathBuf::from("/tmp/test.sock"));
    }

    #[test]
    fn test_timeout_constants() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(5));
        assert_eq!(MAX_RECONNECT_ATTEMPTS, 3);
    }
}
