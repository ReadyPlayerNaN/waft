//! Notification store module.
//!
//! Manages notification state with channel-based updates.

mod manager;
mod types;

pub use manager::{derive_actions, derive_icon_hints, NotificationStore, STORE};
pub use types::{Group, ItemLifecycle, Notification, NotificationOp, State};
