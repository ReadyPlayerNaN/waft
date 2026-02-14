use std::collections::HashSet;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Maximum allowed message size (10 MB), matching waft_protocol::transport.
const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// What kind of client is connected.
pub enum ClientKind {
    /// First message not yet received.
    Unknown,
    /// A plugin that provides entities.
    Plugin { name: String },
    /// An app that subscribes to entity types.
    App { subscriptions: HashSet<String> },
}

/// A connected client (app or plugin) with async send/receive.
pub struct Connection {
    pub id: Uuid,
    pub kind: ClientKind,
    tx: mpsc::Sender<Vec<u8>>,
}

impl Connection {
    /// Accept a new connection, spawning a background write task.
    pub fn new(stream: UnixStream) -> (Self, ReadHalf) {
        let id = Uuid::new_v4();
        let (read_half, write_half) = stream.into_split();
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(write_loop(id, write_half, rx));

        let conn = Connection {
            id,
            kind: ClientKind::Unknown,
            tx,
        };

        (
            conn,
            ReadHalf {
                id,
                reader: read_half,
            },
        )
    }

    /// Queue a serialized message to send to this client.
    pub async fn send<T: Serialize>(&self, msg: &T) -> Result<(), ConnectionError> {
        let payload = serde_json::to_vec(msg)?;
        let len = payload.len() as u32;
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(&payload);

        self.tx
            .send(frame)
            .await
            .map_err(|_| ConnectionError::Closed)?;
        Ok(())
    }
}

/// The read half of a connection, used to receive messages.
pub struct ReadHalf {
    pub id: Uuid,
    reader: tokio::net::unix::OwnedReadHalf,
}

impl ReadHalf {
    /// Read one length-prefixed message. Returns `None` on clean disconnect.
    pub async fn read_message(&mut self) -> Result<Option<Vec<u8>>, ConnectionError> {
        // Read 4-byte length prefix
        let mut len_bytes = [0u8; 4];
        match self.reader.read_exact(&mut len_bytes).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(ConnectionError::Io(e)),
        }

        let len = u32::from_be_bytes(len_bytes) as usize;
        if len > MAX_FRAME_SIZE {
            return Err(ConnectionError::FrameTooLarge(len));
        }

        let mut payload = vec![0u8; len];
        self.reader.read_exact(&mut payload).await?;

        Ok(Some(payload))
    }
}

/// Background task that writes queued frames to the socket.
async fn write_loop(conn_id: Uuid, mut writer: OwnedWriteHalf, mut rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(frame) = rx.recv().await {
        if let Err(e) = writer.write_all(&frame).await {
            eprintln!("[waft] write error for connection {conn_id}: {e}");
            break;
        }
    }
    eprintln!("[waft] write loop exited for connection {conn_id}");
}

/// Errors from connection I/O.
#[derive(Debug)]
pub enum ConnectionError {
    Io(std::io::Error),
    Serialization(serde_json::Error),
    FrameTooLarge(usize),
    Closed,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Io(e) => write!(f, "I/O error: {e}"),
            ConnectionError::Serialization(e) => write!(f, "serialization error: {e}"),
            ConnectionError::FrameTooLarge(size) => {
                write!(f, "frame too large: {size} bytes (max: {MAX_FRAME_SIZE})")
            }
            ConnectionError::Closed => write!(f, "connection closed"),
        }
    }
}

impl std::error::Error for ConnectionError {}

impl From<std::io::Error> for ConnectionError {
    fn from(e: std::io::Error) -> Self {
        ConnectionError::Io(e)
    }
}

impl From<serde_json::Error> for ConnectionError {
    fn from(e: serde_json::Error) -> Self {
        ConnectionError::Serialization(e)
    }
}
