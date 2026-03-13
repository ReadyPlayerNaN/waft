//! Command search index for the command palette.
//!
//! Rebuilt from entity store state whenever command-related entities change.

use waft_client::EntityStore;
use waft_protocol::Urn;

use waft_protocol::commands::COMMAND_DEFS;
use crate::normalize::{Normalized, normalize_for_search};

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
        let mut commands = Vec::new();

        for def in COMMAND_DEFS {
            let entities = store.get_entities_raw(def.entity_type);

            if entities.is_empty() {
                if let Some(raw_urn) = def.static_urn
                    && let Ok(urn) = Urn::parse(raw_urn)
                {
                    let label = def.label.to_string();
                    let label_norm = normalize_for_search(&label);
                    commands.push(CommandSearchEntry {
                        urn,
                        action: def.action.to_string(),
                        label,
                        icon: def.icon.to_string(),
                        subtitle: None,
                        label_norm,
                    });
                }
                continue;
            }

            for (urn, value) in &entities {
                let subtitle = (def.subtitle_fn)(value);

                // For multi-instance types, prepend the entity name to the label
                let label = if entities.len() > 1 {
                    match subtitle.as_deref() {
                        Some(name) => format!("{} {}", def.label, name),
                        None => def.label.to_string(),
                    }
                } else {
                    def.label.to_string()
                };

                let label_norm = normalize_for_search(&label);

                commands.push(CommandSearchEntry {
                    urn: urn.clone(),
                    action: def.action.to_string(),
                    label,
                    icon: def.icon.to_string(),
                    subtitle,
                    label_norm,
                });
            }
        }

        self.commands = commands;
    }

    /// Returns true if no commands are available.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
