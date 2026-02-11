//! Plugin socket server implementation.
//!
//! Handles Unix socket creation, client connections, and message routing.
//! Supports push-based widget updates via WidgetNotifier.

use crate::daemon::PluginDaemon;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::UnixListener;
use tokio::sync::{watch, Mutex};
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

/// Notifier that daemons use to signal that widgets have changed.
///
/// When `notify()` is called, all connected clients receive a fresh
/// `SetWidgets` message with the current widget state.
#[derive(Clone)]
pub struct WidgetNotifier {
    tx: watch::Sender<u64>,
}

impl WidgetNotifier {
    fn new() -> (Self, watch::Receiver<u64>) {
        let (tx, rx) = watch::channel(0u64);
        (Self { tx }, rx)
    }

    /// Signal that widget state has changed.
    ///
    /// All connected clients will receive updated widgets.
    pub fn notify(&self) {
        let cur = *self.tx.borrow();
        let _ = self.tx.send(cur.wrapping_add(1));
    }
}

/// Plugin server that manages socket connections and message handling.
pub struct PluginServer<D: PluginDaemon> {
    plugin_name: String,
    daemon: D,
    notifier_rx: watch::Receiver<u64>,
}

impl<D: PluginDaemon + 'static> PluginServer<D> {
    /// Create a new plugin server.
    ///
    /// Returns `(server, notifier)`. The daemon calls `notifier.notify()`
    /// whenever its state changes, triggering a push to all connected clients.
    pub fn new(plugin_name: impl Into<String>, daemon: D) -> (Self, WidgetNotifier) {
        let (notifier, notifier_rx) = WidgetNotifier::new();
        let server = Self {
            plugin_name: plugin_name.into(),
            daemon,
            notifier_rx,
        };
        (server, notifier)
    }

    /// Run the plugin server.
    ///
    /// Creates Unix socket at `/run/user/{uid}/waft/plugins/{name}.sock`,
    /// accepts connections from overview, and handles message send/receive loop.
    /// Pushes widget updates to all connected clients when the notifier fires.
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

        // Shared state
        let daemon = Arc::new(Mutex::new(self.daemon));
        let clients: Arc<Mutex<Vec<OwnedWriteHalf>>> = Arc::new(Mutex::new(Vec::new()));

        // Spawn push task: watches notifier and sends SetWidgets to all clients
        {
            let daemon = daemon.clone();
            let clients = clients.clone();
            let mut notifier_rx = self.notifier_rx;

            tokio::spawn(async move {
                loop {
                    // Wait for notification (skip the initial value)
                    if notifier_rx.changed().await.is_err() {
                        // Sender dropped, server is shutting down
                        break;
                    }

                    // Get fresh widgets
                    let widgets = {
                        let daemon = daemon.lock().await;
                        daemon.get_widgets()
                    };

                    let msg = PluginMessage::SetWidgets { widgets };

                    // Send to all connected clients, removing dead ones
                    let mut clients_guard = clients.lock().await;
                    let mut i = 0;
                    while i < clients_guard.len() {
                        match write_message_to_half(&mut clients_guard[i], &msg).await {
                            Ok(()) => {
                                i += 1;
                            }
                            Err(_) => {
                                log::debug!("Removing disconnected client during push");
                                clients_guard.swap_remove(i);
                            }
                        }
                    }
                }
            });
        }

        // Accept connections loop
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    log::debug!("Client connected");
                    let daemon = daemon.clone();
                    let clients = clients.clone();

                    // Split the stream
                    let (read_half, write_half) = stream.into_split();

                    // Register write half for push notifications
                    clients.lock().await.push(write_half);

                    // Spawn read task for this client
                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_client_reads(read_half, daemon, clients).await
                        {
                            let err_str = e.to_string();
                            if err_str.contains("UnexpectedEof")
                                || err_str.contains("early eof")
                                || err_str.contains("connection")
                            {
                                log::debug!("Client disconnected: {}", err_str);
                            } else {
                                log::error!("Client handler error: {}", e);
                            }
                        }
                    });
                }
                Err(e) => {
                    log::error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle reads from a single client connection.
    ///
    /// Processes GetWidgets and TriggerAction messages. After handling an action,
    /// pushes updated widgets to ALL connected clients (state may have changed).
    async fn handle_client_reads(
        mut read_half: tokio::net::unix::OwnedReadHalf,
        daemon: Arc<Mutex<D>>,
        clients: Arc<Mutex<Vec<OwnedWriteHalf>>>,
    ) -> Result<(), ServerError> {
        log::debug!("Handling client reads");

        loop {
            // Read message from client
            let msg = read_message_from_half(&mut read_half).await?;
            log::debug!("Received message: {:?}", msg);

            match msg {
                OverviewMessage::GetWidgets => {
                    let widgets = {
                        let daemon = daemon.lock().await;
                        daemon.get_widgets()
                    };
                    let response = PluginMessage::SetWidgets { widgets };

                    // Send response to THIS client (find the matching write half)
                    // Since we can't easily match read/write halves, broadcast to all
                    let mut clients_guard = clients.lock().await;
                    let mut i = 0;
                    while i < clients_guard.len() {
                        match write_message_to_half(&mut clients_guard[i], &response).await {
                            Ok(()) => i += 1,
                            Err(_) => {
                                clients_guard.swap_remove(i);
                            }
                        }
                    }
                }
                OverviewMessage::TriggerAction { widget_id, action } => {
                    log::debug!(
                        "Handling TriggerAction: widget={}, action={:?}",
                        widget_id,
                        action
                    );

                    {
                        let mut daemon = daemon.lock().await;
                        if let Err(e) = daemon.handle_action(widget_id, action).await {
                            log::error!("Action handler error: {}", e);
                        }
                    }

                    // After action, push updated widgets to all clients
                    let widgets = {
                        let daemon = daemon.lock().await;
                        daemon.get_widgets()
                    };
                    let response = PluginMessage::SetWidgets { widgets };

                    let mut clients_guard = clients.lock().await;
                    let mut i = 0;
                    while i < clients_guard.len() {
                        match write_message_to_half(&mut clients_guard[i], &response).await {
                            Ok(()) => i += 1,
                            Err(_) => {
                                clients_guard.swap_remove(i);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get the socket path for this plugin (internal helper).
    pub(crate) fn socket_path(plugin_name: &str) -> Result<PathBuf, ServerError> {
        Ok(plugin_socket_path(plugin_name))
    }
}

/// Get the Unix socket path for a plugin daemon.
///
/// Returns the path where a plugin socket should be created:
/// `{XDG_RUNTIME_DIR}/waft/plugins/{plugin_name}.sock`
///
/// Fallback runtime directory: `/run/user/{uid}`
///
/// Can be overridden with `WAFT_PLUGIN_SOCKET_PATH` environment variable
/// (useful for testing).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::plugin_socket_path;
///
/// let path = plugin_socket_path("clock-daemon");
/// // Returns: /run/user/1000/waft/plugins/clock-daemon.sock
/// ```
pub fn plugin_socket_path(plugin_name: &str) -> PathBuf {
    // Allow override via environment variable (for testing)
    if let Ok(custom_path) = std::env::var("WAFT_PLUGIN_SOCKET_PATH") {
        log::debug!(
            "Using custom socket path from WAFT_PLUGIN_SOCKET_PATH: {}",
            custom_path
        );
        return PathBuf::from(custom_path);
    }

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

    path
}

/// Read a framed message from an OwnedReadHalf.
async fn read_message_from_half(
    read_half: &mut tokio::net::unix::OwnedReadHalf,
) -> Result<OverviewMessage, ServerError> {
    let mut len_bytes = [0u8; 4];
    read_half.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;
    if len > MAX_FRAME_SIZE {
        return Err(ServerError::FrameTooLarge(len));
    }

    let mut payload = vec![0u8; len];
    read_half.read_exact(&mut payload).await?;

    let msg = serde_json::from_slice(&payload)?;
    Ok(msg)
}

/// Write a framed message to an OwnedWriteHalf.
async fn write_message_to_half(
    write_half: &mut OwnedWriteHalf,
    msg: &PluginMessage,
) -> Result<(), ServerError> {
    let payload = serde_json::to_vec(msg)?;
    let len_bytes = (payload.len() as u32).to_be_bytes();
    write_half.write_all(&len_bytes).await?;
    write_half.write_all(&payload).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_error_display() {
        let io_err =
            ServerError::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test"));
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
        let json_err =
            serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
        let server_err: ServerError = json_err.into();

        match server_err {
            ServerError::Json(_) => {}
            _ => panic!("Expected Json variant"),
        }
    }

    #[test]
    fn test_socket_path_from_env_override() {
        unsafe {
            std::env::set_var("WAFT_PLUGIN_SOCKET_PATH", "/tmp/custom-test.sock");
        }

        let path =
            PluginServer::<crate::testing::TestPlugin>::socket_path("test-plugin").unwrap();
        assert_eq!(path, PathBuf::from("/tmp/custom-test.sock"));

        unsafe {
            std::env::remove_var("WAFT_PLUGIN_SOCKET_PATH");
        }
    }

    #[test]
    fn test_socket_path_default_behavior() {
        unsafe {
            std::env::remove_var("WAFT_PLUGIN_SOCKET_PATH");
        }

        let path =
            PluginServer::<crate::testing::TestPlugin>::socket_path("test-plugin").unwrap();

        assert!(path.to_string_lossy().contains("test-plugin.sock"));
        assert!(path.to_string_lossy().contains("waft/plugins"));
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
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", "/custom/runtime");
        }

        let path = PluginServer::<TestDaemon>::socket_path("test-plugin").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/custom/runtime/waft/plugins/test-plugin.sock")
        );

        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }

    #[test]
    fn test_socket_path_fallback() {
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }

        let path = PluginServer::<TestDaemon>::socket_path("test-plugin").unwrap();
        let uid = unsafe { libc::getuid() };
        let expected = PathBuf::from(format!(
            "/run/user/{}/waft/plugins/test-plugin.sock",
            uid
        ));
        assert_eq!(path, expected);
    }

    #[test]
    fn test_socket_path_sanitizes_plugin_name() {
        let path = PluginServer::<TestDaemon>::socket_path("my-plugin-123").unwrap();
        assert!(path.ends_with("my-plugin-123.sock"));
        assert!(path.to_string_lossy().contains("waft/plugins"));
    }

    #[test]
    fn test_widget_notifier() {
        let (notifier, mut rx) = WidgetNotifier::new();

        // Initial value
        assert_eq!(*rx.borrow(), 0);

        // After notify
        notifier.notify();
        assert!(rx.has_changed().unwrap());
        rx.mark_changed(); // reset
        assert_eq!(*rx.borrow_and_update(), 1);

        // Multiple notifications
        notifier.notify();
        notifier.notify();
        assert_eq!(*rx.borrow_and_update(), 3);
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
