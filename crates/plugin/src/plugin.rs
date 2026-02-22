//! Plugin trait and Entity type.
//!
//! Plugins implement the `Plugin` trait to provide domain entities and
//! handle actions. Unlike the old `PluginDaemon` trait that returned
//! widget descriptions, `Plugin` returns domain data as entities.

use serde::Serialize;
use uuid::Uuid;
use waft_protocol::PluginDescription;
use waft_protocol::urn::Urn;

use crate::claim::ClaimSender;

/// A domain entity produced by a plugin.
///
/// Each entity has a URN (unique identifier), an entity type string,
/// and a JSON-serialized data payload.
#[derive(Debug, Clone)]
pub struct Entity {
    /// Unique identifier: `{plugin}/{entity-type}/{id}`.
    pub urn: Urn,
    /// Entity type string (e.g. "clock", "dark-mode").
    pub entity_type: String,
    /// Serialized entity data.
    pub data: serde_json::Value,
}

impl Entity {
    /// Create an entity with typed data.
    ///
    /// Serializes `data` to a `serde_json::Value`.
    ///
    /// # Panics
    ///
    /// Panics if `data` cannot be serialized to JSON (should not happen
    /// for well-formed domain structs).
    pub fn new<T: Serialize>(urn: Urn, entity_type: &str, data: &T) -> Self {
        Self {
            urn,
            entity_type: entity_type.to_string(),
            data: serde_json::to_value(data).expect("entity data must be JSON-serializable"),
        }
    }
}

/// Trait for waft plugins.
///
/// Plugins are `Send + Sync` because the runtime may call `get_entities()`
/// from a different context than `handle_action()`. Use interior mutability
/// (e.g. `Arc<StdMutex<State>>`) for shared mutable state.
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    /// Return all current entities.
    ///
    /// Called when connecting to the daemon and whenever the `EntityNotifier`
    /// fires. The runtime diffs against previous state and sends
    /// `EntityUpdated`/`EntityRemoved` messages to the daemon.
    fn get_entities(&self) -> Vec<Entity>;

    /// Handle an action triggered by an app via the daemon.
    ///
    /// # Arguments
    ///
    /// * `urn` - The entity the action targets
    /// * `action` - Action name (e.g. "toggle", "click")
    /// * `params` - Action parameters as JSON
    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Whether the plugin can stop gracefully.
    ///
    /// Called when the daemon sends a `CanStop` command. Returns `true`
    /// by default. Override to return `false` if the plugin needs to
    /// finish work before stopping.
    fn can_stop(&self) -> bool {
        true
    }

    /// Describe this plugin's entity types, properties, and actions.
    ///
    /// Called during manifest generation (`provides --describe`), not at
    /// runtime. Returns `None` by default (plugin has no description).
    /// Override to provide self-documentation for settings UIs and CLI tools.
    fn describe(&self) -> Option<PluginDescription> {
        None
    }

    /// Called by the runtime once before `run()` to give the plugin a claim sender.
    ///
    /// Override to store the sender for use in `handle_action`.
    fn set_claim_sender(&self, _sender: ClaimSender) {}

    /// Called by the runtime when the daemon resolves a ClaimCheck.
    ///
    /// `claimed: true` means at least one subscriber still wants the entity -- keep it.
    /// `claimed: false` means no subscriber wants it -- the plugin should remove it.
    ///
    /// Default: no-op (plugin ignores claim results).
    async fn handle_claim_result(&self, _urn: Urn, _claim_id: Uuid, _claimed: bool) {}
}
