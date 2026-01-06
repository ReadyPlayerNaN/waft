//! Notifications feature module.
//!
//! This module will host the notifications plugin plus its internal UI/model code,
//! split into submodules for clarity.
//!
//! The actual implementation is defined in:
//! - `plugin.rs` (plugin glue / exported plugin type)
//! - `types.rs` (notification domain types: `Notification`, `NotificationIcon`, ...)
//! - `model.rs` (testable grouping/sorting state model)
//! - `view.rs` (GTK rendering)
//! - `controller.rs` (wires model<->view and exposes imperative controls)

pub mod controller;
pub mod model;
pub mod plugin;
pub mod types;
pub mod view;

pub use plugin::NotificationsPlugin;
pub use types::{Notification, NotificationAction, NotificationGroup, NotificationIcon};
