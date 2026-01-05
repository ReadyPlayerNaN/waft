//! UI module entrypoint.
//!
//! This module is intended to keep `main.rs` small by grouping UI-related code into submodules.

pub mod features;
pub mod notifications;

// Re-export commonly used builders/types if you want a flatter import surface.
pub use features::{FeatureSpec, build_features_section};
pub use notifications::{Notification, build_notifications_section};
