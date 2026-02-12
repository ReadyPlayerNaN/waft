//! Minimal library entry point intended for integration tests.
//!
//! This crate is primarily a binary, but integration tests (`tests/`) need a
//! library target to import code. To keep changes small and avoid pulling in the
//! full application/plugin surface (GTK init, DBus server/client wiring, etc.),
//! we only expose the minimal shared types.

pub mod common;
pub mod menu_state;
pub mod store;
