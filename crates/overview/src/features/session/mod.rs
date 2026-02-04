//! Session lock detection via logind D-Bus signals.
//!
//! This module monitors the systemd-logind session for Lock/Unlock signals
//! and broadcasts them to the application. This allows the UI to pause
//! animations and hide windows before the compositor stops sending events.

mod dbus;

pub use dbus::{SessionEvent, SessionMonitor};
