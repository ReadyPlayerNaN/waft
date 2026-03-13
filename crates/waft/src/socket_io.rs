//! Shared socket I/O helpers for CLI commands that talk to the waft daemon.

use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use waft_protocol::message::{AppMessage, AppNotification};

/// Maximum allowed message size (10 MB), matching waft_protocol::transport.
pub const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// Resolve the daemon socket path (read-only, no directory creation or stale socket removal).
pub fn daemon_socket_path() -> Result<PathBuf, String> {
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR not set".to_string())?;
    let mut path = PathBuf::from(runtime_dir);
    path.push("waft");
    path.push("daemon.sock");
    Ok(path)
}

/// Connect to the daemon socket, returning a helpful error on failure.
pub async fn connect_daemon() -> Result<UnixStream, String> {
    let socket_path = daemon_socket_path()?;
    UnixStream::connect(&socket_path).await.map_err(|e| {
        match e.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                "waft daemon is not running. Start it with `waft` or `waft daemon`.".to_string()
            }
            _ => format!("Failed to connect to daemon: {e}"),
        }
    })
}

/// Send a length-prefixed JSON message to the daemon.
pub async fn send_message(
    stream: &mut UnixStream,
    msg: &AppMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::to_vec(msg)?;
    let len = payload.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&payload).await?;
    Ok(())
}

/// Read one length-prefixed JSON message from the daemon.
/// Returns `None` on clean disconnect.
pub async fn read_message(
    stream: &mut UnixStream,
) -> Result<Option<AppNotification>, Box<dyn std::error::Error>> {
    let mut len_bytes = [0u8; 4];
    match stream.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }

    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(format!("frame too large: {len} bytes (max: {MAX_FRAME_SIZE})").into());
    }

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;

    let notification: AppNotification = serde_json::from_slice(&payload)?;
    Ok(Some(notification))
}
