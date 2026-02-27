use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use waft_protocol::{AppMessage, AppNotification};

/// Maximum allowed message size (10 MB), matching the daemon.
const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// A test app client that connects to a daemon over a Unix socket.
///
/// Uses raw length-prefixed JSON framing (same as the daemon), without
/// depending on WaftClient or glib.
pub struct TestApp {
    stream: UnixStream,
}

impl TestApp {
    /// Connect to a daemon at the given socket path.
    pub async fn connect(socket_path: &Path) -> Self {
        let stream = UnixStream::connect(socket_path)
            .await
            .expect("failed to connect TestApp to daemon socket");
        TestApp { stream }
    }

    /// Send an AppMessage to the daemon.
    pub async fn send(&mut self, msg: &AppMessage) {
        let payload = serde_json::to_vec(msg).expect("failed to serialize AppMessage");
        let len = payload.len() as u32;
        self.stream
            .write_all(&len.to_be_bytes())
            .await
            .expect("failed to write length prefix");
        self.stream
            .write_all(&payload)
            .await
            .expect("failed to write payload");
    }

    /// Subscribe to an entity type.
    pub async fn subscribe(&mut self, entity_type: &str) {
        self.send(&AppMessage::Subscribe {
            entity_type: entity_type.to_string(),
        })
        .await;
    }

    /// Receive an AppNotification from the daemon, with a timeout.
    ///
    /// Returns `None` if the timeout expires before a message arrives.
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Option<AppNotification> {
        tokio::time::timeout(timeout, self.recv_one()).await.ok()
    }

    /// Read one length-prefixed JSON message from the stream.
    async fn recv_one(&mut self) -> AppNotification {
        let mut len_bytes = [0u8; 4];
        self.stream
            .read_exact(&mut len_bytes)
            .await
            .expect("failed to read length prefix");

        let len = u32::from_be_bytes(len_bytes) as usize;
        assert!(
            len <= MAX_FRAME_SIZE,
            "frame too large: {len} bytes (max: {MAX_FRAME_SIZE})"
        );

        let mut payload = vec![0u8; len];
        self.stream
            .read_exact(&mut payload)
            .await
            .expect("failed to read payload");

        serde_json::from_slice(&payload).expect("failed to deserialize AppNotification")
    }
}
