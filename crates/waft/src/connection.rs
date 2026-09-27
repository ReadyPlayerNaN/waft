use std::collections::HashSet;

use waft_protocol::CAP_DERIVED_ENTITY_TYPE;

use log::{debug, trace};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Maximum allowed message size (10 MB), matching waft_protocol::transport.
const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct ProtocolState {
    pub legacy: bool,
    pub negotiated_version: Option<u32>,
    pub implementation: Option<String>,
    pub capabilities: HashSet<String>,
}

impl ProtocolState {
    pub fn legacy() -> Self {
        Self {
            legacy: true,
            ..Self::default()
        }
    }

    pub fn supports_derived_entity_type(&self) -> bool {
        self.capabilities.contains(CAP_DERIVED_ENTITY_TYPE)
    }
}

/// What kind of client is connected.
pub enum ClientKind {
    /// First message not yet received.
    Unknown,
    /// A plugin that provides entities.
    Plugin { name: String },
    /// An app that subscribes to entity types.
    App {
        subscriptions: HashSet<String>,
        in_flight_status: HashSet<String>,
    },
}

/// A connected client (app or plugin) with async send/receive.
pub struct Connection {
    pub id: Uuid,
    pub kind: ClientKind,
    pub protocol: ProtocolState,
    tx: mpsc::Sender<Vec<u8>>,
    control_tx: mpsc::Sender<Vec<u8>>,
}

impl Connection {
    /// Accept a new connection, spawning a background write task.
    pub fn new(stream: UnixStream) -> (Self, ReadHalf) {
        let id = Uuid::new_v4();
        let (read_half, write_half) = stream.into_split();
        let (tx, rx) = mpsc::channel(64);
        let (control_tx, control_rx) = mpsc::channel(64);

        tokio::spawn(write_loop(id, write_half, control_rx, rx));

        let conn = Connection {
            id,
            kind: ClientKind::Unknown,
            protocol: ProtocolState::default(),
            tx,
            control_tx,
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
        self.send_frame(&self.tx, msg)
    }

    /// Send a control-plane command through the priority queue. Cancellation
    /// and shutdown commands must not wait behind bulk entity updates.
    pub async fn send_control<T: Serialize>(&self, msg: &T) -> Result<(), ConnectionError> {
        self.send_frame(&self.control_tx, msg)
    }

    fn send_frame<T: Serialize>(
        &self,
        tx: &mpsc::Sender<Vec<u8>>,
        msg: &T,
    ) -> Result<(), ConnectionError> {
        let payload = serde_json::to_vec(msg)?;
        let len = payload.len() as u32;
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(&payload);

        tx.try_send(frame).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => ConnectionError::Backpressure,
            mpsc::error::TrySendError::Closed(_) => ConnectionError::Closed,
        })?;
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
async fn write_loop(
    conn_id: Uuid,
    mut writer: OwnedWriteHalf,
    mut control_rx: mpsc::Receiver<Vec<u8>>,
    mut rx: mpsc::Receiver<Vec<u8>>,
) {
    let mut control_open = true;
    loop {
        let mut control_closed = false;
        let frame = if control_open {
            tokio::select! {
                biased;
                frame = control_rx.recv() => match frame {
                    Some(frame) => Some(frame),
                    None => {
                        control_open = false;
                        control_closed = true;
                        None
                    }
                },
                frame = rx.recv() => frame,
            }
        } else {
            rx.recv().await
        };
        if control_closed {
            continue;
        }
        let Some(frame) = frame else {
            break;
        };
        if let Err(e) = writer.write_all(&frame).await {
            debug!("write error for connection {conn_id}: {e}");
            break;
        }
    }
    trace!("write loop exited for connection {conn_id}");
}

/// Errors from connection I/O.
#[derive(Debug)]
pub enum ConnectionError {
    Io(std::io::Error),
    Serialization(serde_json::Error),
    FrameTooLarge(usize),
    Closed,
    Backpressure,
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
            ConnectionError::Backpressure => write!(f, "connection output queue full"),
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
