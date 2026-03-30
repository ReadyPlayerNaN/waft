//! Length-prefixed message framing for IPC.
//!
//! This module provides framing primitives for reading and writing messages over byte streams.
//! Each message is prefixed with its length as a 4-byte big-endian unsigned integer, followed
//! by the JSON-serialized message payload.
//!
//! Frame format:
//! ```text
//! [4 bytes: u32 length (big-endian)][N bytes: JSON payload]
//! ```
//!
//! This framing protocol ensures:
//! - Clear message boundaries in streaming contexts
//! - Protection against oversized messages (10MB limit)
//! - Compatibility with any `Read`/`Write` implementation

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Maximum allowed message size (10 MB).
pub const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

/// Errors that can occur during framed message transport.
#[derive(Debug)]
pub enum TransportError {
    /// I/O error during read or write operation.
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

/// Write a length-prefixed JSON message to a writer.
///
/// The message is serialized to JSON, then written as:
/// 1. 4 bytes: message length as big-endian u32
/// 2. N bytes: JSON payload
///
/// # Errors
///
/// Returns `TransportError::Serialization` if JSON serialization fails.
/// Returns `TransportError::Io` if writing to the stream fails.
pub fn write_framed<W: Write, T: Serialize>(writer: &mut W, msg: &T) -> Result<(), TransportError> {
    // Serialize message to JSON
    let payload = serde_json::to_vec(msg)?;
    let len = payload.len();

    // Write length prefix (big-endian u32)
    let len_bytes = (len as u32).to_be_bytes();
    writer.write_all(&len_bytes)?;

    // Write payload
    writer.write_all(&payload)?;

    Ok(())
}

/// Read a length-prefixed JSON message from a reader.
///
/// Reads the frame structure:
/// 1. 4 bytes: message length as big-endian u32
/// 2. N bytes: JSON payload
///
/// # Errors
///
/// Returns `TransportError::FrameTooLarge` if the message size exceeds `MAX_FRAME_SIZE` (10MB).
/// Returns `TransportError::Io` if reading from the stream fails.
/// Returns `TransportError::Serialization` if JSON deserialization fails.
pub fn read_framed<R: Read, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, TransportError> {
    // Read length prefix (big-endian u32)
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Check size limit
    if len > MAX_FRAME_SIZE {
        return Err(TransportError::FrameTooLarge(len));
    }

    // Read payload
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;

    // Deserialize from JSON
    let msg = serde_json::from_slice(&payload)?;

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::io::Cursor;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestMessage {
        id: u32,
        text: String,
    }

    #[test]
    fn round_trip_simple_message() {
        let original = TestMessage {
            id: 42,
            text: "Hello, IPC!".to_string(),
        };

        // Write to buffer
        let mut buffer = Vec::new();
        write_framed(&mut buffer, &original).expect("write_framed failed");

        // Read back
        let mut cursor = Cursor::new(buffer);
        let decoded: TestMessage = read_framed(&mut cursor).expect("read_framed failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn round_trip_multiple_messages() {
        let messages = vec![
            TestMessage {
                id: 1,
                text: "first".to_string(),
            },
            TestMessage {
                id: 2,
                text: "second".to_string(),
            },
            TestMessage {
                id: 3,
                text: "third".to_string(),
            },
        ];

        // Write all messages
        let mut buffer = Vec::new();
        for msg in &messages {
            write_framed(&mut buffer, msg).expect("write_framed failed");
        }

        // Read all messages back
        let mut cursor = Cursor::new(buffer);
        for expected in &messages {
            let decoded: TestMessage = read_framed(&mut cursor).expect("read_framed failed");
            assert_eq!(expected, &decoded);
        }
    }

    #[test]
    fn large_message_within_limit() {
        // Create a message just under the 10MB limit
        let large_text = "x".repeat(1024 * 1024); // 1 MB of text
        let msg = TestMessage {
            id: 999,
            text: large_text.clone(),
        };

        let mut buffer = Vec::new();
        write_framed(&mut buffer, &msg).expect("write_framed failed");

        let mut cursor = Cursor::new(buffer);
        let decoded: TestMessage = read_framed(&mut cursor).expect("read_framed failed");

        assert_eq!(msg.id, decoded.id);
        assert_eq!(msg.text.len(), decoded.text.len());
    }

    #[test]
    fn oversized_frame_rejected() {
        // Manually create a frame that claims to be larger than MAX_FRAME_SIZE
        let oversized_len = (MAX_FRAME_SIZE + 1) as u32;
        let mut buffer = oversized_len.to_be_bytes().to_vec();
        buffer.extend_from_slice(&[0u8; 100]); // Some dummy data

        let mut cursor = Cursor::new(buffer);
        let result: Result<TestMessage, _> = read_framed(&mut cursor);

        match result {
            Err(TransportError::FrameTooLarge(size)) => {
                assert_eq!(size, (MAX_FRAME_SIZE + 1));
            }
            _ => panic!("Expected FrameTooLarge error, got: {result:?}"),
        }
    }

    #[test]
    fn invalid_json_rejected() {
        // Write a valid length prefix but invalid JSON payload
        let mut buffer = Vec::new();
        let invalid_json = b"{not valid json}";
        let len = (invalid_json.len() as u32).to_be_bytes();
        buffer.extend_from_slice(&len);
        buffer.extend_from_slice(invalid_json);

        let mut cursor = Cursor::new(buffer);
        let result: Result<TestMessage, _> = read_framed(&mut cursor);

        match result {
            Err(TransportError::Serialization(_)) => {
                // Expected
            }
            _ => panic!("Expected Serialization error, got: {result:?}"),
        }
    }

    #[test]
    fn incomplete_frame_io_error() {
        // Write only 2 bytes of the 4-byte length prefix
        let buffer = vec![0u8, 0u8];

        let mut cursor = Cursor::new(buffer);
        let result: Result<TestMessage, _> = read_framed(&mut cursor);

        match result {
            Err(TransportError::Io(_)) => {
                // Expected: UnexpectedEof
            }
            _ => panic!("Expected I/O error, got: {result:?}"),
        }
    }

    #[test]
    fn empty_message() {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Empty {}

        let msg = Empty {};

        let mut buffer = Vec::new();
        write_framed(&mut buffer, &msg).expect("write_framed failed");

        let mut cursor = Cursor::new(buffer);
        let decoded: Empty = read_framed(&mut cursor).expect("read_framed failed");

        assert_eq!(msg, decoded);
    }

    #[test]
    fn transport_error_display() {
        let io_err =
            TransportError::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "test"));
        assert!(io_err.to_string().contains("I/O error"));

        let large_err = TransportError::FrameTooLarge(20_000_000);
        let display = large_err.to_string();
        assert!(display.contains("frame too large"));
        assert!(display.contains("20000000"));
        assert!(display.contains("10485760")); // MAX_FRAME_SIZE

        let disconnected_err = TransportError::Disconnected;
        assert!(disconnected_err.to_string().contains("peer disconnected"));
    }
}
