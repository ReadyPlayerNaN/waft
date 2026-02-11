//! Plugin daemon trait definition.
//!
//! Implement this trait to create a plugin daemon.

use waft_ipc::widget::{Action, NamedWidget};

/// Trait for plugin daemons.
///
/// Implement this to define your plugin's behavior.
#[async_trait::async_trait]
pub trait PluginDaemon: Send + Sync {
    /// Get the current widget set for this plugin.
    ///
    /// Called when overview requests widgets or after an action is handled.
    fn get_widgets(&self) -> Vec<NamedWidget>;

    /// Handle a user action from the overview.
    ///
    /// Called when the user interacts with a widget (toggle, slider, button, etc.)
    /// Takes `&self` (not `&mut self`) so the server can call `get_widgets()`
    /// concurrently — enabling intermediate state pushes (e.g. busy spinners)
    /// while an action is in progress. Use interior mutability for state changes.
    async fn handle_action(
        &self,
        widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
