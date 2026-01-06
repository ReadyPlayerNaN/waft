//! UI module entrypoint.
//!
//! This module is intended to keep `main.rs` small by grouping UI-related code into submodules.

pub mod agenda;
pub mod features;
pub mod overlay_animation;

/// High-level UI events that can be emitted by plugins or internal logic
/// and consumed by the UI layer (e.g., to update FeaturesModel).
///
/// This is intentionally generic and plugin-agnostic; plugins convert their
/// own internal state changes into these events.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// A feature's active (on/off) state changed.
    FeatureActiveChanged { key: String, active: bool },

    /// A feature's status text changed.
    FeatureStatusTextChanged { key: String, text: String },

    /// A feature's menu open/closed state changed.
    FeatureMenuOpenChanged { key: String, open: bool },
}

/// Trait for things that can accept UI events.
/// The typical implementation will live in the UI layer and update models
/// such as FeaturesModel.
pub trait UiEventSink {
    fn send(&self, event: UiEvent);
}
// Notifications have been moved under `src/features/notifications` and are now provided
// via the plugin system (see `crate::features::notifications::NotificationsPlugin`).

// Re-export commonly used builders/types if you want a flatter import surface.
pub use agenda::{MeetingItem, build_agenda_section};
pub use features::{FeatureSpec, build_features_section};
