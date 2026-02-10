//! Common types shared across plugins.

pub use waft_core::{Callback, VoidCallback};

/// Represents the connection state of a device or service.
///
/// This enum is used by multiple features (Bluetooth, VPN, etc.) to represent
/// the lifecycle state of a connection. It follows the standard 4-state model:
/// - Not connected
/// - In progress to connect
/// - Connected
/// - In progress to disconnect
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}
