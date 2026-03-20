//! Async length-prefixed JSON transport for plugin IPC.
//!
//! Same 4-byte BE length + JSON framing as `waft_protocol::transport`,
//! but using `AsyncReadExt`/`AsyncWriteExt` for tokio integration.
//!
//! The error type and framing constants are re-exported from
//! `waft_protocol::transport` to avoid duplication.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub use waft_protocol::transport::{MAX_FRAME_SIZE, TransportError};

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
