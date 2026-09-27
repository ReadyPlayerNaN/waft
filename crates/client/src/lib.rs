//! Shared client library for connecting to the central waft daemon.
//!
//! Provides `WaftClient` for socket communication, `daemon_connection_task`
//! for connection lifecycle management, and `EntityStore` for observable
//! entity caching with per-type subscriptions.

mod action_gate;
mod connection;
mod connection_task;
mod entity_store;

pub use action_gate::ActionGate;
pub use connection::{WaftClient, WaftClientError};
pub use connection_task::{ClientEvent, daemon_connection_task};
pub use entity_store::{EntityActionCallback, EntityStore};
