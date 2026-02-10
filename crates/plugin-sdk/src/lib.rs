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
//!     enabled: bool,
//! }
//!
//! #[async_trait::async_trait]
//! impl PluginDaemon for MyPlugin {
//!     fn get_widgets(&self) -> Vec<NamedWidget> {
//!         vec![
//!             NamedWidget {
//!                 id: "my_plugin:toggle".into(),
//!                 weight: 100,
//!                 widget: Widget::FeatureToggle {
//!                     title: "My Feature".into(),
//!                     icon: "emblem-system-symbolic".into(),
//!                     details: None,
//!                     active: self.enabled,
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
//!     async fn handle_action(&mut self, widget_id: String, action: Action)
//!         -> Result<(), Box<dyn std::error::Error + Send + Sync>>
//!     {
//!         match action.id.as_str() {
//!             "toggle" => {
//!                 self.enabled = !self.enabled;
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

pub use daemon::PluginDaemon;
pub use server::{PluginServer, WidgetNotifier};
pub use builder::*;

// Re-export common types from waft-ipc
pub use waft_ipc::widget::{
    Action, ActionParams, NamedWidget, Node, Orientation, Widget, WidgetSet,
};
pub use waft_ipc::message::{OverviewMessage, PluginMessage, PROTOCOL_VERSION};
