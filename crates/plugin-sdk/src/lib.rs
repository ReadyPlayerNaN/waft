//! Plugin SDK for building waft plugin daemons.
//!
//! This crate provides the infrastructure for building plugin daemons that
//! communicate with waft-overview via Unix sockets.
//!
//! # Architecture
//!
//! Plugin daemons are standalone processes that:
//! - Create a Unix socket at `/run/user/{uid}/waft/plugins/{name}.sock`
//! - Accept connections from waft-overview
//! - Send widget descriptions when requested
//! - Receive and handle user actions
//! - Update widgets when state changes
//!
//! # Example
//!
//! ```rust,no_run
//! use waft_plugin_sdk::*;
//! use waft_ipc::widget::*;
//!
//! struct MyPlugin {
//!     enabled: std::sync::Mutex<bool>,
//! }
//!
//! #[async_trait::async_trait]
//! impl PluginDaemon for MyPlugin {
//!     fn get_widgets(&self) -> Vec<NamedWidget> {
//!         let enabled = *self.enabled.lock().unwrap();
//!         vec![
//!             NamedWidget {
//!                 id: "my_plugin:toggle".into(),
//!                 weight: 100,
//!                 widget: Widget::FeatureToggle {
//!                     title: "My Feature".into(),
//!                     icon: "emblem-system-symbolic".into(),
//!                     details: None,
//!                     active: enabled,
//!                     busy: false,
//!                     expandable: false,
//!                     expanded_content: None,
//!                     on_toggle: Action {
//!                         id: "toggle".into(),
//!                         params: ActionParams::None,
//!                     },
//!                 },
//!             }
//!         ]
//!     }
//!
//!     async fn handle_action(&self, widget_id: String, action: Action)
//!         -> Result<(), Box<dyn std::error::Error + Send + Sync>>
//!     {
//!         match action.id.as_str() {
//!             "toggle" => {
//!                 let mut enabled = self.enabled.lock().unwrap();
//!                 *enabled = !*enabled;
//!                 Ok(())
//!             }
//!             _ => Ok(()),
//!         }
//!     }
//! }
//! ```

pub mod server;
pub mod daemon;
pub mod builder;
pub mod testing;
pub mod config;
pub mod dbus_monitor;

pub use daemon::PluginDaemon;
pub use server::{PluginServer, WidgetNotifier, plugin_socket_path};
pub use builder::*;
pub use dbus_monitor::{monitor_signal, monitor_signal_async, SignalMonitorConfig};

// Re-export common types from waft-ipc
pub use waft_ipc::widget::{
    Action, ActionParams, NamedWidget, Node, StatusOption, Widget, WidgetSet,
};
pub use waft_ipc::message::{OverviewMessage, PluginMessage, PROTOCOL_VERSION};

/// Initialize env_logger for daemon plugins.
///
/// This helper provides consistent logging setup across all daemon plugins.
/// The log level defaults to the provided `default_level`, but can be overridden
/// via the `RUST_LOG` environment variable.
///
/// # Example
///
/// ```rust,no_run
/// use waft_plugin_sdk::init_daemon_logger;
///
/// fn main() {
///     init_daemon_logger("info");
///     // ... rest of daemon initialization
/// }
/// ```
pub fn init_daemon_logger(default_level: &str) {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(default_level)
    ).init();
}
