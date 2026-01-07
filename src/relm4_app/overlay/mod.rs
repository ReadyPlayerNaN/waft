//! Overlay host module (migration step 05).
//!
//! This module will contain the Relm4 overlay window/component implementation and
//! supporting GTK-free helpers.
//!
//! For step 05, we expose a pure bucketing/layout helper that maps mounted plugin
//! placements (slot + weight) into the Top/Left/Right buckets with deterministic ordering.

pub mod layout;

pub use layout::{OverlayBuckets, bucketize_mounted_plugins};
