//! Shared client library for connecting to the central waft daemon.
//!
//! Provides `WaftClient` for socket communication, `daemon_connection_task`
//! for connection lifecycle management, and `EntityStore` for observable
//! entity caching with per-type subscriptions.

mod connection;
mod connection_task;
mod entity_store;

pub use connection::{WaftClient, WaftClientError};
pub use connection_task::{ClientEvent, daemon_connection_task};
pub use entity_store::{EntityActionCallback, EntityStore};
