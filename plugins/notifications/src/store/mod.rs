//! Notification store module.
//!
//! Manages notification state with instance-based stores.

mod deprioritize;
mod manager;
mod types;

pub use manager::{NotificationStore, create_notification_store, reorder_icon_hints_for_group};
pub use types::{ItemLifecycle, Notification, NotificationOp};
