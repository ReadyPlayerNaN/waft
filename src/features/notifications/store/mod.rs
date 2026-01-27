//! Notification store module.
//!
//! Manages notification state with instance-based stores.

mod manager;
mod types;

pub use manager::{NotificationStore, create_notification_store};
pub use types::{ItemLifecycle, Notification, NotificationOp};
