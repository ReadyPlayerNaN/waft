//! Waft protocol types for entity-based communication between apps, waft daemon, and plugins.
//!
//! This crate defines:
//! - Entity types representing domain data, organized by domain (not by plugin)
//! - Protocol messages for app/daemon/plugin communication
//! - URN (Uniform Resource Name) identifiers for entities
//! - Length-prefixed JSON transport framing
//!
//! # Naming convention
//!
//! Entity modules are named after **domains** (`display`, `power`, `session`),
//! never after plugin implementations (`darkman`, `sunsetr`, `caffeine`).
//! See [`entity`] module docs for details.

pub const PROTOCOL_VERSION: u32 = 1;

pub mod description;
pub mod entity;
pub mod message;
pub mod transport;
pub mod urn;

pub use description::PluginDescription;
pub use message::{AppMessage, AppNotification, PluginCommand, PluginMessage};
pub use transport::{MAX_FRAME_SIZE, TransportError, read_framed, write_framed};
pub use urn::{Urn, UrnError};
