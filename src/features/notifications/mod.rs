//! Notifications feature module.
//!
//! This module hosts the notifications plugin plus its internal UI/model code,
//! split into submodules for clarity.
//!
//! The actual implementation is defined in:
//! - `types.rs` – domain types (`Notification`, `NotificationIcon`, actions, snapshot types)
//! - `model.rs` – testable grouping/sorting model
//! - `view.rs` – GTK rendering
//! - `controller.rs` – wiring + imperative methods (`add/remove/clear`)
//! - `toast_policy.rs` – pure toast policy/state (no GTK), unit-testable
//! - `plugin.rs` – plugin glue + seeding data (until ingress is implemented)

pub mod controller;
pub mod model;
pub mod plugin;
pub mod toast_policy;
pub mod toast_view;
pub mod types;
pub mod view;

pub mod card;

#[cfg(test)]
mod plugin_tests;

pub use plugin::NotificationsPlugin;
