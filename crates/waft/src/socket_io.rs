//! Shared socket I/O helpers for CLI commands that talk to the waft daemon.

use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use waft_protocol::message::{AppMessage, AppNotification};
use waft_protocol::{
    CAP_DERIVED_ENTITY_TYPE, CAP_HANDSHAKE, CAP_SCHEMA_METADATA, CAP_STATUS_COMPLETE,
    CAP_STRUCTURED_ERRORS, HandshakeMessage, Hello, PROTOCOL_VERSION,
};

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
    let mut stream = UnixStream::connect(&socket_path)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                "waft daemon is not running. Start it with `waft` or `waft daemon`.".to_string()
            }
            _ => format!("Failed to connect to daemon: {e}"),
        })?;

    let hello = HandshakeMessage::Hello(Hello::app(
        "waft-cli",
        PROTOCOL_VERSION,
        vec![
            CAP_HANDSHAKE.to_string(),
            CAP_STRUCTURED_ERRORS.to_string(),
            CAP_DERIVED_ENTITY_TYPE.to_string(),
            CAP_STATUS_COMPLETE.to_string(),
            CAP_SCHEMA_METADATA.to_string(),
        ],
    ));
    let payload = serde_json::to_vec(&hello).map_err(|e| format!("Failed to serialize hello: {e}"))?;
    let len = payload.len() as u32;
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| format!("Failed to send handshake: {e}"))?;
    stream
        .write_all(&payload)
        .await
        .map_err(|e| format!("Failed to send handshake: {e}"))?;

    match read_handshake(&mut stream).await? {
        HandshakeMessage::HelloAck(_) => Ok(stream),
        HandshakeMessage::HelloError(err) => Err(format!("Daemon rejected handshake: {}", err.error.message)),
        other => Err(format!("Unexpected handshake response: {other:?}")),
    }
}

async fn read_handshake(stream: &mut UnixStream) -> Result<HandshakeMessage, String> {
    let mut len_bytes = [0u8; 4];
    match stream.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err("daemon disconnected during handshake".to_string())
        }
        Err(e) => return Err(format!("Failed to read handshake: {e}")),
    }

    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(format!("frame too large: {len} bytes (max: {MAX_FRAME_SIZE})"));
    }

    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| format!("Failed to read handshake: {e}"))?;
    serde_json::from_slice(&payload).map_err(|e| format!("Failed to decode handshake: {e}"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::net::UnixListener;
    use waft_protocol::{ProtocolError, ProtocolErrorScope};

    async fn write_frame<T: serde::Serialize>(stream: &mut UnixStream, value: &T) {
        let payload = serde_json::to_vec(value).expect("serialize");
        stream
            .write_all(&(payload.len() as u32).to_be_bytes())
            .await
            .expect("len");
        stream.write_all(&payload).await.expect("payload");
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    async fn connect_daemon_performs_handshake() {
        let runtime_dir = TempDir::new().expect("tempdir");
        let socket_dir = runtime_dir.path().join("waft");
        std::fs::create_dir_all(&socket_dir).expect("socket dir");
        let socket_path = socket_dir.join("daemon.sock");
        let listener = UnixListener::bind(&socket_path).expect("bind");
        let runtime_path = runtime_dir.path().to_path_buf();
        let saved_runtime = std::env::var("XDG_RUNTIME_DIR").ok();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime_path) };

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            match read_handshake(&mut stream).await.expect("hello") {
                HandshakeMessage::Hello(hello) => {
                    assert_eq!(hello.role, waft_protocol::PeerRole::App);
                    assert_eq!(hello.implementation, "waft-cli");
                    assert!(hello.capabilities.contains(&CAP_HANDSHAKE.to_string()));
                }
                other => panic!("expected Hello, got: {other:?}"),
            }
            write_frame(
                &mut stream,
                &HandshakeMessage::HelloAck(waft_protocol::HelloAck {
                    negotiated_version: PROTOCOL_VERSION,
                    capabilities: vec![CAP_HANDSHAKE.to_string()],
                }),
            )
            .await;
        });

        let stream = connect_daemon().await.expect("connect_daemon");
        drop(stream);
        server.await.expect("server task");
        match saved_runtime {
            Some(path) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", path) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    async fn connect_daemon_surfaces_hello_error() {
        let runtime_dir = TempDir::new().expect("tempdir");
        let socket_dir = runtime_dir.path().join("waft");
        std::fs::create_dir_all(&socket_dir).expect("socket dir");
        let socket_path = socket_dir.join("daemon.sock");
        let listener = UnixListener::bind(&socket_path).expect("bind");
        let runtime_path = runtime_dir.path().to_path_buf();
        let saved_runtime = std::env::var("XDG_RUNTIME_DIR").ok();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime_path) };

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let _ = read_handshake(&mut stream).await.expect("hello");
            write_frame(
                &mut stream,
                &HandshakeMessage::HelloError(waft_protocol::HelloError {
                    error: ProtocolError::new(
                        "handshake.denied",
                        "no thanks",
                        ProtocolErrorScope::Handshake,
                        false,
                    ),
                }),
            )
            .await;
        });

        let error = connect_daemon().await.expect_err("handshake should fail");
        assert!(error.contains("Daemon rejected handshake: no thanks"));
        server.await.expect("server task");
        match saved_runtime {
            Some(path) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", path) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }
}
