//! Plugin manager for discovering and managing IPC-based plugins
//!
//! This module handles the discovery of plugins via Unix domain sockets
//! located in the runtime directory.

mod client;
mod diff;
mod discovery;
mod manager;
mod registry;
mod router;

pub use client::{ClientError, PluginClient};
pub use diff::{diff_widgets, WidgetDiff};
pub use discovery::{discover_plugins, PluginInfo};
pub use manager::{PluginManager, PluginManagerConfig, PluginUpdate};
pub use registry::WidgetRegistry;
pub use router::{ActionRouter, RouterError};
