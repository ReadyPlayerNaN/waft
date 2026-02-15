//! Notification store module.
//!
//! Manages notification state with direct function calls.

pub mod manager;
pub mod types;
pub mod workspace_extract;

pub use manager::{process_op, reorder_icon_hints_for_group};
pub use types::{ItemLifecycle, Notification, NotificationOp, State};
