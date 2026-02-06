//! Common types shared across multiple features.

use std::cell::RefCell;
use std::rc::Rc;

/// Type alias for optional callback functions with a parameter.
///
/// This pattern is used throughout the codebase for widget output callbacks.
/// Example: `Callback<FeatureToggleOutput>` for a callback that receives toggle events.
pub type Callback<T> = Rc<RefCell<Option<Box<dyn Fn(T)>>>>;

/// Type alias for optional callback functions without parameters.
///
/// Used for simple event callbacks that don't pass any data.
pub type VoidCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

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
