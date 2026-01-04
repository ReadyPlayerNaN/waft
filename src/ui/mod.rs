//! UI module entrypoint.
//!
//! This module is intended to keep `main.rs` small by grouping UI-related code into submodules.

pub mod features;

// Re-export commonly used builders/types if you want a flatter import surface.
pub use features::{FeatureSpec, build_features_section};
