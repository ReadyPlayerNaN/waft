//! Plugin SDK for building waft plugins (entity-based architecture).
//!
//! This crate provides the infrastructure for building plugin processes that
//! connect to the waft daemon via Unix socket and provide domain entities.
//!
//! # Architecture
//!
//! Plugins are socket **clients** that connect to the waft daemon at
//! `$XDG_RUNTIME_DIR/waft/daemon.sock`. They send `EntityUpdated`/`EntityRemoved`
//! messages and receive `TriggerAction`/`CanStop` commands.
//!
//! # Example
//!
//! ```rust,no_run
//! use waft_plugin::*;
//! use waft_protocol::urn::Urn;
//!
//! struct MyPlugin;
//!
//! #[async_trait::async_trait]
//! impl Plugin for MyPlugin {
//!     fn get_entities(&self) -> Vec<Entity> {
//!         vec![Entity::new(
//!             Urn::new("my-plugin", "my-entity", "default"),
//!             "my-entity",
//!             &serde_json::json!({"value": 42}),
//!         )]
//!     }
//!
//!     async fn handle_action(
//!         &self,
//!         _urn: Urn,
//!         _action: String,
//!         _params: serde_json::Value,
//!     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         Ok(())
//!     }
//! }
//! ```

pub mod claim;
pub mod config;
pub mod dbus_monitor;
pub mod manifest;
pub mod notifier;
pub mod plugin;
pub mod runner;
pub mod runtime;
pub mod state_locker;
pub mod transport;

pub use claim::ClaimSender;
pub use notifier::EntityNotifier;
pub use plugin::{Entity, Plugin};
pub use runner::{PluginRunner, spawn_monitored, spawn_monitored_anyhow};
pub use runtime::{PluginRuntime, daemon_socket_path};
pub use state_locker::StateLocker;

// Re-export serde_json for plugin action params
pub use serde_json;

// Re-export protocol types commonly used by plugins
pub use waft_protocol::PluginDescription;
pub use waft_protocol::description;
pub use waft_protocol::entity;
pub use waft_protocol::urn::Urn;
pub use waft_protocol::{PluginCommand, PluginMessage};

/// Initialize env_logger for plugin processes.
///
/// The log level defaults to the provided `default_level`, but can be
/// overridden via the `RUST_LOG` environment variable.
pub fn init_plugin_logger(default_level: &str) {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .init();
}
