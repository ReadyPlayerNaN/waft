//! Async IPC client/server implementation (Wayland-only by project design).
//!
//! This module is designed to be usable by the Relm4 path without touching GTK from
//! background tasks:
//! - The server runs on Tokio, reading one request per connection (first line / first JSON object).
//! - Parsed commands are forwarded to the UI thread via a simple callback, typically:
//!   `glib::MainContext::default().invoke_local(move || { ... })`.
//!
//! Protocol compatibility:
//! - Accepts legacy JSON payloads like `{"cmd":"toggle"}` or `{"command":"toggle"}`.
//! - Replies with a single JSON line (best-effort).
//!
//! Client/Server policy helpers:
//! - `try_become_server` fails with `AlreadyRunning` if a live server is already bound.
//! - Stale socket files are removed if no server is listening.
//!
//! NOTE: This module intentionally does not depend on GTK/Relm4.

use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::{IpcCommand, IpcError, command_to_json_line, parse_command_from_json};

/// IPC runtime error.
#[derive(Debug)]
pub enum IpcNetError {
    /// Already running (a server answered on the socket).
    AlreadyRunning,

    /// Socket path doesn't exist or cannot be used.
    NoServer,

    /// Underlying IO error.
    Io(std::io::Error),

    /// Failed to parse/understand a command.
    Parse(IpcError),
}

impl std::fmt::Display for IpcNetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcNetError::AlreadyRunning => write!(f, "already running"),
            IpcNetError::NoServer => write!(f, "no server running"),
            IpcNetError::Io(e) => write!(f, "io error: {e}"),
            IpcNetError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for IpcNetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IpcNetError::Io(e) => Some(e),
            IpcNetError::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for IpcNetError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<IpcError> for IpcNetError {
    fn from(value: IpcError) -> Self {
        Self::Parse(value)
    }
}

/// Compute a client-friendly error if server is not available.
fn map_connect_err(e: std::io::Error) -> IpcNetError {
    // We intentionally avoid platform-specific errno matching; treat any connect failure
    // as "no server" for CLI UX.
    let _ = e;
    IpcNetError::NoServer
}

/// Send one IPC command to the running server.
///
/// Returns the raw response string (may be empty).
pub async fn send_command(socket: &Path, cmd: IpcCommand) -> Result<String, IpcNetError> {
    let mut stream = UnixStream::connect(socket).await.map_err(map_connect_err)?;

    let req = command_to_json_line(cmd);
    stream.write_all(req.as_bytes()).await?;
    stream.shutdown().await?;

    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf).trim().to_string())
}

/// Like `send_command`, but send an arbitrary JSON payload (legacy-compatible).
pub async fn send_raw_json(socket: &Path, json: &str) -> Result<String, IpcNetError> {
    let mut stream = UnixStream::connect(socket).await.map_err(map_connect_err)?;

    stream.write_all(json.as_bytes()).await?;
    if !json.ends_with('\n') {
        stream.write_all(b"\n").await?;
    }
    stream.shutdown().await?;

    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf).trim().to_string())
}

/// Attempt to become the IPC server.
///
/// Behavior (legacy-compatible):
/// - If the socket exists and a connect succeeds => return `AlreadyRunning`.
/// - If the socket exists but connect fails => assume stale socket, remove it.
/// - Bind and return a `UnixListener` on success.
pub async fn try_become_server(socket: &Path) -> Result<UnixListener, IpcNetError> {
    if socket.exists() {
        if UnixStream::connect(socket).await.is_ok() {
            return Err(IpcNetError::AlreadyRunning);
        }
        // Stale socket file.
        let _ = std::fs::remove_file(socket);
    }

    let listener = UnixListener::bind(socket)?;
    Ok(listener)
}

/// Read a single request (up to first newline or a small cap) from a stream.
async fn read_one_request_line(stream: &mut UnixStream) -> Result<String, std::io::Error> {
    // Read until newline or cap.
    let mut buf = Vec::<u8>::new();
    let mut tmp = [0u8; 1024];

    // Cap to avoid unbounded growth from a bad client.
    let cap: usize = 64 * 1024;

    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);

        if buf.contains(&b'\n') || buf.len() >= cap {
            break;
        }
    }

    // Take up to first newline if present.
    if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
        buf.truncate(pos);
    }

    Ok(String::from_utf8_lossy(&buf).trim().to_string())
}

/// Produce a simple one-line JSON response.
fn response_for(cmd: IpcCommand) -> String {
    match cmd {
        IpcCommand::Ping => r#"{"ok":true,"reply":"pong"}"#.to_string(),
        IpcCommand::Show => r#"{"ok":true,"queued":"show"}"#.to_string(),
        IpcCommand::Hide => r#"{"ok":true,"queued":"hide"}"#.to_string(),
        IpcCommand::Toggle => r#"{"ok":true,"queued":"toggle"}"#.to_string(),
        IpcCommand::Stop => r#"{"ok":true,"queued":"stop"}"#.to_string(),
    }
}

/// Run the IPC server loop.
///
/// For each accepted connection:
/// - read one line / payload
/// - parse `IpcCommand`
/// - invoke `on_command(cmd)`
/// - reply with a small JSON line
///
/// This function never touches GTK; callers should do any GTK interaction in `on_command`
/// by scheduling onto the GTK main context.
pub async fn run_server<F>(listener: UnixListener, on_command: F) -> Result<(), IpcNetError>
where
    F: Fn(IpcCommand) + Send + Sync + 'static,
{
    let on_command = std::sync::Arc::new(on_command);

    loop {
        let (stream, _addr) = listener.accept().await?;
        let on_command = on_command.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_one_connection(stream, &on_command).await {
                log::debug!("[ipc] connection handler error: {e}");
            }
        });
    }
}

async fn handle_one_connection<F>(
    mut stream: UnixStream,
    on_command: &std::sync::Arc<F>,
) -> Result<(), IpcNetError>
where
    F: Fn(IpcCommand) + Send + Sync + 'static,
{
    let payload = read_one_request_line(&mut stream).await?;
    if payload.is_empty() {
        let _ = stream.write_all(br#"{"ok":false,"error":"empty"}"#).await;
        return Ok(());
    }

    let cmd = parse_command_from_json(&payload)?;
    (on_command)(cmd);

    let resp = response_for(cmd);
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.write_all(b"\n").await;
    let _ = stream.shutdown().await;
    Ok(())
}

/// Helper for cleaning up the socket file on graceful shutdown.
///
/// This is optional; stale sockets are handled by `try_become_server` anyway.
pub fn cleanup_socket_file(socket: &Path) {
    let _ = std::fs::remove_file(socket);
}

/// Convenience: create parent directory for a socket path if needed.
///
/// Typically not required for XDG_RUNTIME_DIR sockets, but useful if you change policy.
pub fn ensure_parent_dir(socket: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Convenience: convert a `PathBuf` to a best-effort owned path string for logs.
pub fn socket_display(socket: &Path) -> String {
    socket.to_string_lossy().to_string()
}

/// Convenience: normalized socket path for tests/logging (no filesystem interaction).
pub fn normalize_socket_path(socket: &PathBuf) -> PathBuf {
    socket.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_for_is_jsonish() {
        assert!(response_for(IpcCommand::Ping).contains("pong"));
        assert!(response_for(IpcCommand::Toggle).contains("toggle"));
    }
}
