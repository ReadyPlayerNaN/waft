//! Async length-prefixed JSON transport for plugin IPC.
//!
//! Same 4-byte BE length + JSON framing as `waft_protocol::transport`,
//! but using `AsyncReadExt`/`AsyncWriteExt` for tokio integration.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Maximum allowed message size (10 MB), matching waft_protocol::transport.
const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// Errors that can occur during async framed message transport.
#[derive(Debug)]
pub enum TransportError {
    /// I/O error during read or write.
    Io(std::io::Error),
    /// JSON serialization or deserialization error.
    Serialization(serde_json::Error),
    /// Message exceeded maximum allowed size.
    FrameTooLarge(usize),
    /// Peer disconnected (EOF on read).
    Disconnected,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "I/O error: {e}"),
            TransportError::Serialization(e) => write!(f, "serialization error: {e}"),
            TransportError::FrameTooLarge(size) => {
                write!(f, "frame too large: {size} bytes (max: {MAX_FRAME_SIZE})")
            }
            TransportError::Disconnected => write!(f, "peer disconnected"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransportError::Io(e) => Some(e),
            TransportError::Serialization(e) => Some(e),
            TransportError::FrameTooLarge(_) | TransportError::Disconnected => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError::Io(e)
    }
}

impl From<serde_json::Error> for TransportError {
    fn from(e: serde_json::Error) -> Self {
        TransportError::Serialization(e)
    }
}

/// Write a length-prefixed JSON message to an async writer.
pub async fn write_framed<W: AsyncWriteExt + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<(), TransportError> {
    let payload = serde_json::to_vec(msg)?;
    let len_bytes = (payload.len() as u32).to_be_bytes();
    writer.write_all(&len_bytes).await?;
    writer.write_all(&payload).await?;
    Ok(())
}

/// Read a length-prefixed JSON message from an async reader.
///
/// Returns `Ok(None)` on clean disconnect (EOF when reading the length prefix).
pub async fn read_framed<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<Option<T>, TransportError> {
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(TransportError::Io(e)),
    }

    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(TransportError::FrameTooLarge(len));
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;

    let msg = serde_json::from_slice(&payload)?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestMsg {
        id: u32,
        text: String,
    }

    #[tokio::test]
    async fn round_trip() {
        let (mut client, mut server) = duplex(1024);
        let original = TestMsg {
            id: 42,
            text: "hello".to_string(),
        };

        write_framed(&mut client, &original).await.unwrap();
        let decoded: Option<TestMsg> = read_framed(&mut server).await.unwrap();
        assert_eq!(decoded, Some(original));
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let (client, mut server) = duplex(1024);
        drop(client);
        let result: Result<Option<TestMsg>, _> = read_framed(&mut server).await;
        assert!(matches!(result, Ok(None)));
    }

    #[tokio::test]
    async fn oversized_frame_rejected() {
        let (mut client, mut server) = duplex(1024);
        let oversized_len = (MAX_FRAME_SIZE + 1) as u32;
        use tokio::io::AsyncWriteExt;
        client
            .write_all(&oversized_len.to_be_bytes())
            .await
            .unwrap();

        let result: Result<Option<TestMsg>, _> = read_framed(&mut server).await;
        assert!(matches!(result, Err(TransportError::FrameTooLarge(_))));
    }
}
