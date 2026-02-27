use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use waft_protocol::urn::Urn;
use waft_protocol::{PluginCommand, PluginMessage};

/// Maximum allowed message size (10 MB), matching the daemon.
const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// A test plugin client that connects to a daemon over a Unix socket.
///
/// Uses raw length-prefixed JSON framing (same as the daemon), without
/// depending on PluginRuntime.
pub struct TestPlugin {
    stream: UnixStream,
}

impl TestPlugin {
    /// Connect to a daemon at the given socket path.
    pub async fn connect(socket_path: &Path) -> Self {
        let stream = UnixStream::connect(socket_path)
            .await
            .expect("failed to connect TestPlugin to daemon socket");
        TestPlugin { stream }
    }

    /// Send a raw PluginMessage to the daemon.
    pub async fn send(&mut self, msg: &PluginMessage) {
        let payload = serde_json::to_vec(msg).expect("failed to serialize PluginMessage");
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

    /// Send an EntityUpdated message.
    pub async fn send_entity(
        &mut self,
        urn: Urn,
        entity_type: &str,
        data: serde_json::Value,
    ) {
        self.send(&PluginMessage::EntityUpdated {
            urn,
            entity_type: entity_type.to_string(),
            data,
        })
        .await;
    }

    /// Send an EntityRemoved message.
    pub async fn send_entity_removed(&mut self, urn: Urn, entity_type: &str) {
        self.send(&PluginMessage::EntityRemoved {
            urn,
            entity_type: entity_type.to_string(),
        })
        .await;
    }

    /// Receive a PluginCommand from the daemon, with a timeout.
    ///
    /// Returns `None` if the timeout expires before a message arrives.
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Option<PluginCommand> {
        tokio::time::timeout(timeout, self.recv_one()).await.ok()
    }

    /// Read one length-prefixed JSON message from the stream.
    async fn recv_one(&mut self) -> PluginCommand {
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

        serde_json::from_slice(&payload).expect("failed to deserialize PluginCommand")
    }
}
