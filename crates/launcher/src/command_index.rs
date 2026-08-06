//! Command search index for the command palette.
//!
//! Rebuilt from entity store state whenever command-related entities change.

use std::collections::HashMap;

use waft_client::EntityStore;
use waft_protocol::Urn;

use crate::normalize::{Normalized, normalize_for_search};
use waft_protocol::commands::resolve_commands;

/// A single searchable command entry derived from a live entity + action.
pub struct CommandSearchEntry {
    pub urn: Urn,
    pub action: String,
    pub label: String,
    pub icon: String,
    pub subtitle: Option<String>,
    pub label_norm: Normalized,
}

/// Index of all available commands, rebuilt from entity store state.
pub struct CommandIndex {
    pub commands: Vec<CommandSearchEntry>,
}

impl CommandIndex {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

impl Default for CommandIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandIndex {
    /// Rebuild the command list from current entity store state.
    ///
    /// Iterates all command definitions, looks up matching entities in the store,
    /// and generates one `CommandSearchEntry` per (entity, action) pair.
    pub fn rebuild(&mut self, store: &EntityStore) {
        let mut entity_map: HashMap<String, Vec<(Urn, serde_json::Value)>> = HashMap::new();
        for entity_type in waft_protocol::commands::command_entity_types() {
            entity_map.insert((*entity_type).to_string(), store.get_entities_raw(entity_type));
        }

        self.commands = resolve_commands(&entity_map)
            .into_iter()
            .map(|cmd| {
                let label_norm = normalize_for_search(&cmd.label);
                CommandSearchEntry {
                    urn: cmd.urn,
                    action: cmd.action,
                    label: cmd.label,
                    icon: cmd.icon,
                    subtitle: cmd.subtitle,
                    label_norm,
                }
            })
            .collect();
    }

    /// Returns true if no commands are available.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
