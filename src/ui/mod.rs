//! UI module entrypoint.
//!
//! This module is intended to keep `main.rs` small by grouping UI-related code into submodules.

pub mod agenda;
pub mod features;
pub mod notifications;

// Re-export commonly used builders/types if you want a flatter import surface.
pub use agenda::{build_agenda_section, MeetingItem};
pub use features::{build_features_section, FeatureSpec};
pub use notifications::{build_notifications_section, Notification};
