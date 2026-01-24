//! Notification store module.
//!
//! Manages notification state with channel-based updates.

mod manager;
mod types;

pub use manager::STORE;
pub use types::{ItemLifecycle, Notification, NotificationOp};
