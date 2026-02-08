//! Plugin socket server implementation.
//!
//! Handles Unix socket creation, client connections, and message routing.

use crate::daemon::PluginDaemon;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use waft_ipc::{OverviewMessage, PluginMessage};

/// Server errors.
#[derive(Debug)]
pub enum ServerError {
    /// I/O error.
    Io(std::io::Error),
    /// JSON serialization error.
    Json(serde_json::Error),
    /// Frame too large.
    FrameTooLarge(usize),
    /// Other error with string message.
    Other(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Io(e) => write!(f, "I/O error: {}", e),
            ServerError::Json(e) => write!(f, "JSON error: {}", e),
            ServerError::FrameTooLarge(size) => write!(f, "Frame too large: {} bytes", size),
            ServerError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for ServerError {}

impl From<std::io::Error> for ServerError {
    fn from(e: std::io::Error) -> Self {
        ServerError::Io(e)
    }
}

impl From<serde_json::Error> for ServerError {
    fn from(e: serde_json::Error) -> Self {
        ServerError::Json(e)
    }
}

impl From<String> for ServerError {
    fn from(s: String) -> Self {
        ServerError::Other(s)
    }
}

/// Plugin server that manages socket connections and message handling.
pub struct PluginServer<D: PluginDaemon> {
    plugin_name: String,
    daemon: D,
}

impl<D: PluginDaemon + 'static> PluginServer<D> {
    /// Create a new plugin server.
    pub fn new(plugin_name: impl Into<String>, daemon: D) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            daemon,
        }
    }

    /// Run the plugin server.
    ///
    /// Creates Unix socket at `/run/user/{uid}/waft/plugins/{name}.sock`,
    /// accepts connections from overview, and handles message send/receive loop.
    pub async fn run(self) -> Result<(), ServerError> {
        let plugin_name = self.plugin_name.clone();
        log::info!("Plugin server started: {}", plugin_name);

        // Get socket path
        let socket_path = Self::socket_path(&plugin_name)?;
        log::info!("Socket path: {}", socket_path.display());

        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
            log::debug!("Created socket directory: {}", parent.display());
        }

        // Remove stale socket if it exists
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
            log::debug!("Removed stale socket: {}", socket_path.display());
        }

        // Bind Unix socket
        let listener = UnixListener::bind(&socket_path)?;
        log::info!("Listening on: {}", socket_path.display());

        // Wrap daemon in Arc<Mutex<>> for sharing between async tasks
        let daemon = Arc::new(Mutex::new(self.daemon));

        // Accept connections loop
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    log::debug!("Client connected");
                    let daemon = daemon.clone();
                    let plugin_name = plugin_name.clone();

                    // Spawn task to handle this client
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, daemon, &plugin_name).await {
                            log::error!("Client handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    log::error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle a single client connection.
    async fn handle_client(
        mut stream: UnixStream,
        daemon: Arc<Mutex<D>>,
        _plugin_name: &str,
    ) -> Result<(), ServerError> {
        log::debug!("Handling client connection");

        loop {
            // Read message from client
            match Self::read_message(&mut stream).await {
                Ok(msg) => {
                    log::debug!("Received message: {:?}", msg);

                    // Handle message and get response
                    let response = Self::handle_message(msg, daemon.clone()).await?;

                    // Send response
                    if let Some(response_msg) = response {
                        log::debug!("Sending response: {:?}", response_msg);
                        Self::write_message(&mut stream, &response_msg).await?;
                    }
                }
                Err(e) => {
                    // Check if it's a clean disconnect
                    if e.to_string().contains("UnexpectedEof")
                        || e.to_string().contains("connection")
                    {
                        log::debug!("Client disconnected");
                        break;
                    } else {
                        log::error!("Failed to read message: {}", e);
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle an overview message and return optional response.
    async fn handle_message(
        msg: OverviewMessage,
        daemon: Arc<Mutex<D>>,
    ) -> Result<Option<PluginMessage>, ServerError> {
        match msg {
            OverviewMessage::GetWidgets => {
                log::debug!("Handling GetWidgets");
                let daemon = daemon.lock().await;
                let widgets = daemon.get_widgets();
                Ok(Some(PluginMessage::SetWidgets { widgets }))
            }
            OverviewMessage::TriggerAction { widget_id, action } => {
                log::debug!("Handling TriggerAction: widget={}, action={:?}", widget_id, action);
                let mut daemon = daemon.lock().await;

                // Handle action and convert error
                if let Err(e) = daemon.handle_action(widget_id, action).await {
                    return Err(ServerError::Other(format!("Action handler error: {}", e)));
                }

                // Send updated widgets after action
                let widgets = daemon.get_widgets();
                Ok(Some(PluginMessage::SetWidgets { widgets }))
            }
        }
    }

    /// Read a framed message from the stream (async version of transport::read_framed).
    async fn read_message(
        stream: &mut UnixStream,
    ) -> Result<OverviewMessage, ServerError> {
        // Read 4-byte length prefix (big-endian)
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Check size limit (10MB)
        const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;
        if len > MAX_FRAME_SIZE {
            return Err(ServerError::FrameTooLarge(len));
        }

        // Read payload
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await?;

        // Deserialize from JSON
        let msg = serde_json::from_slice(&payload)?;
        Ok(msg)
    }

    /// Write a framed message to the stream (async version of transport::write_framed).
    async fn write_message(
        stream: &mut UnixStream,
        msg: &PluginMessage,
    ) -> Result<(), ServerError> {
        // Serialize to JSON
        let payload = serde_json::to_vec(msg)?;
        let len = payload.len();

        // Write length prefix (big-endian u32)
        let len_bytes = (len as u32).to_be_bytes();
        stream.write_all(&len_bytes).await?;

        // Write payload
        stream.write_all(&payload).await?;

        Ok(())
    }

    /// Get the socket path for this plugin.
    fn socket_path(plugin_name: &str) -> Result<PathBuf, ServerError> {
        // Get runtime directory from environment
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
            // Fallback: /run/user/{uid}
            let uid = unsafe { libc::getuid() };
            format!("/run/user/{}", uid)
        });

        // Build socket path: {runtime_dir}/waft/plugins/{plugin_name}.sock
        let mut path = PathBuf::from(runtime_dir);
        path.push("waft");
        path.push("plugins");
        path.push(format!("{}.sock", plugin_name));

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_error_display() {
        let io_err = ServerError::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test"));
        assert!(io_err.to_string().contains("I/O error"));

        let json_err = ServerError::Json(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "test",
        )));
        assert!(json_err.to_string().contains("JSON error"));

        let frame_err = ServerError::FrameTooLarge(20_000_000);
        assert!(frame_err.to_string().contains("Frame too large"));
        assert!(frame_err.to_string().contains("20000000"));

        let other_err = ServerError::Other("custom error".to_string());
        assert_eq!(other_err.to_string(), "custom error");
    }

    #[test]
    fn test_server_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let server_err: ServerError = io_err.into();

        match server_err {
            ServerError::Io(_) => {}
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn test_server_error_from_json_error() {
        let json_err = serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
        let server_err: ServerError = json_err.into();

        match server_err {
            ServerError::Json(_) => {}
            _ => panic!("Expected Json variant"),
        }
    }

    #[test]
    fn test_server_error_from_string() {
        let error_msg = "something went wrong".to_string();
        let server_err: ServerError = error_msg.clone().into();

        match server_err {
            ServerError::Other(msg) => assert_eq!(msg, error_msg),
            _ => panic!("Expected Other variant"),
        }
    }

    #[test]
    fn test_socket_path_from_env() {
        // Set XDG_RUNTIME_DIR temporarily
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", "/custom/runtime") };

        let path = PluginServer::<TestDaemon>::socket_path("test-plugin").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/custom/runtime/waft/plugins/test-plugin.sock")
        );

        // Clean up
        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
    }

    #[test]
    fn test_socket_path_fallback() {
        // Remove XDG_RUNTIME_DIR to test fallback
        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };

        let path = PluginServer::<TestDaemon>::socket_path("test-plugin").unwrap();
        let uid = unsafe { libc::getuid() };
        let expected = PathBuf::from(format!("/run/user/{}/waft/plugins/test-plugin.sock", uid));
        assert_eq!(path, expected);
    }

    #[test]
    fn test_socket_path_sanitizes_plugin_name() {
        let path = PluginServer::<TestDaemon>::socket_path("my-plugin-123").unwrap();
        assert!(path.ends_with("my-plugin-123.sock"));
        assert!(path.to_string_lossy().contains("waft/plugins"));
    }

    // Test daemon for unit tests
    struct TestDaemon;

    #[async_trait::async_trait]
    impl PluginDaemon for TestDaemon {
        fn get_widgets(&self) -> Vec<waft_ipc::widget::NamedWidget> {
            vec![]
        }

        async fn handle_action(
            &mut self,
            _widget_id: String,
            _action: waft_ipc::widget::Action,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
}
