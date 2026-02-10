//! Plugin manager for discovering and managing IPC-based plugins
//!
//! This module handles the discovery of plugins via Unix domain sockets
//! located in the runtime directory.

mod client;
mod discovery;
mod manager;
mod registry;
mod router;

pub use client::{ClientError, InternalMessage, PluginClient};
pub use discovery::{discover_plugins, PluginInfo};
pub use manager::{PluginManager, PluginManagerConfig, PluginUpdate, SharedRouter};
pub use registry::WidgetRegistry;
pub use router::{ActionRouter, RouterError};
