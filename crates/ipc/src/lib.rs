//! IPC utilities for waft-overview.
//!
//! This crate provides:
//! - **Command parsing** (`command`) - CLI commands like `toggle`/`show`/`hide`
//! - **Async networking** (`net`) - Unix socket client/server helpers

pub mod command;
pub mod net;

pub use command::{
    command_from_args, command_name_from_json, command_to_json_line, ipc_socket_path,
    parse_command_from_json, parse_command_word, socket_exists, IpcCommand, IpcError,
};
