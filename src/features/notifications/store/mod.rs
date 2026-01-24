//! Notification store module.
//!
//! Manages notification state with instance-based stores.

mod manager;
mod types;

pub use manager::{create_notification_store, NotificationStore};
pub use types::{ItemLifecycle, Notification, NotificationOp, State};
