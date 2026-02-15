//! Niri compositor plugin for Waft.
//!
//! Provides keyboard layout and display output entities by communicating
//! with the Niri compositor via `niri msg` CLI commands and monitoring
//! the `niri msg --json event-stream` for real-time updates.
//!
//! ## Entity Types
//!
//! - `keyboard-layout` - Active keyboard layout and available alternatives
//! - `display-output` - Display outputs with resolution, refresh rate, and VRR
//!
//! ## Communication
//!
//! All communication with Niri uses CLI commands executed on background threads
//! (via `std::thread::spawn` + flume channels) to avoid depending on tokio's
//! IO reactor.

pub mod commands;
pub mod config;
pub mod display;
pub mod event_stream;
pub mod keyboard;
pub mod state;
