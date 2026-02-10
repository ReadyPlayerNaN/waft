//! IPC protocol and utilities for waft-overview and plugin daemons.
//!
//! This crate is intentionally:
#![allow(dead_code)]
//! - GTK-free (protocol types have no rendering dependencies)
//! - fast to unit-test
//! - serialization-ready (all protocol types derive Serialize/Deserialize)
//!
//! It provides:
//! - **Widget protocol types** (`widget`) - declarative UI widget descriptions
//! - **Protocol messages** (`message`) - plugin-to-overview communication
//! - **Command parsing** (`command`) - CLI commands like `toggle`/`show`/`hide`
//! - **Transport framing** (`transport`) - length-prefixed message serialization
//! - **Async networking** (`net`) - Unix socket client/server helpers
//!
//! This crate enables plugin daemons to run as separate processes and communicate
//! their UI state to the overview via a well-defined protocol.

pub mod command;
pub mod message;
pub mod net;
pub mod transport;
pub mod widget;

// Re-export command module items at crate root for backwards compatibility
pub use command::{
    command_from_args, command_name_from_json, command_to_json_line, ipc_socket_path,
    parse_command_from_json, parse_command_word, socket_exists, IpcCommand, IpcError,
};

// Re-export message module items at crate root
pub use message::{OverviewMessage, PluginMessage, PROTOCOL_VERSION};

// Re-export widget module items at crate root
pub use widget::{
    Action, ActionParams, NamedWidget, Node, StatusOption, Widget, WidgetSet,
};
