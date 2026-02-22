//! On-demand plugin spawning for the waft daemon.
//!
//! When an app subscribes to an entity type and no plugin currently provides it,
//! `PluginSpawner` looks up the binary from the discovery cache and spawns it.
//! Child processes are tracked and properly reaped.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Child;

use log::{debug, error, info, warn};
use waft_protocol::PluginDescription;

use crate::plugin_discovery::PluginDiscoveryCache;

/// A spawned plugin process.
struct SpawnedPlugin {
    /// The child process, if still tracked. Taken by the reaper thread.
    child: Option<Child>,
}

/// Manages on-demand spawning of plugin binaries.
pub struct PluginSpawner {
    discovery_cache: PluginDiscoveryCache,
    /// plugin_name -> SpawnedPlugin
    spawned: HashMap<String, SpawnedPlugin>,
    /// entity types for which we have already attempted spawning
    spawn_attempted: HashSet<String>,
}

impl PluginSpawner {
    /// Create a new spawner from a pre-built discovery cache.
    pub fn new(discovery_cache: PluginDiscoveryCache) -> Self {
        Self {
            discovery_cache,
            spawned: HashMap::new(),
            spawn_attempted: HashSet::new(),
        }
    }

    /// Ensure a plugin is running for the given entity type.
    ///
    /// If a plugin is already spawned (or no binary is known for this type),
    /// this is a no-op. The daemon should call this when an app subscribes
    /// to an entity type.
    pub fn ensure_plugin_for_entity_type(&mut self, entity_type: &str) {
        // Already attempted spawning for this entity type
        if self.spawn_attempted.contains(entity_type) {
            return;
        }

        let providers: Vec<(String, PathBuf)> = self
            .discovery_cache
            .binaries_for_entity_type(entity_type)
            .iter()
            .cloned()
            .collect();

        if providers.is_empty() {
            debug!("no plugin known for entity type '{entity_type}'");
            self.spawn_attempted.insert(entity_type.to_string());
            return;
        }

        for (plugin_name, binary_path) in &providers {
            // Already spawned this plugin (it may provide multiple entity types)
            if self.spawned.contains_key(plugin_name) {
                continue;
            }
            self.spawn_plugin(plugin_name, binary_path);
        }

        self.spawn_attempted.insert(entity_type.to_string());
    }

    fn spawn_plugin(&mut self, plugin_name: &str, binary_path: &PathBuf) {
        match std::process::Command::new(binary_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
        {
            Ok(child) => {
                let pid = child.id();
                info!(
                    "spawned plugin '{}' (pid {}) from {}",
                    plugin_name,
                    pid,
                    binary_path.display()
                );

                let mut spawned = SpawnedPlugin { child: Some(child) };

                // Spawn a reaper thread so we don't create zombie processes
                if let Some(child) = spawned.child.take() {
                    let name = plugin_name.to_string();
                    std::thread::Builder::new()
                        .name(format!("reap-{}", plugin_name))
                        .spawn(move || {
                            let mut child = child;
                            match child.wait() {
                                Ok(status) => {
                                    info!("plugin '{name}' exited with {status}");
                                }
                                Err(e) => {
                                    warn!("failed to wait for plugin '{name}': {e}");
                                }
                            }
                        })
                        .unwrap_or_else(|e| {
                            warn!(
                                "failed to spawn reaper thread for '{plugin_name}': {e}"
                            );
                            // At minimum, we tried. The child will be reaped on daemon exit.
                            std::thread::spawn(|| {})
                        });
                }

                self.spawned.insert(plugin_name.to_string(), spawned);
            }
            Err(e) => {
                error!("failed to spawn plugin '{}': {e}", plugin_name);
            }
        }
    }

    /// Mark a plugin as disconnected, allowing it to be re-spawned.
    ///
    /// Called when a plugin process exits (gracefully or via crash). Clears the
    /// spawn tracking so `ensure_plugin_for_entity_type` will spawn it again.
    pub fn mark_disconnected(&mut self, plugin_name: &str) {
        self.spawned.remove(plugin_name);
        self.spawn_attempted.retain(|et| {
            // Clear entity types that belong to this plugin (may have co-providers)
            !self
                .discovery_cache
                .binaries_for_entity_type(et)
                .iter()
                .any(|(name, _)| name == plugin_name)
        });
    }

    /// Get the cached description for a specific plugin.
    pub fn get_description(&self, plugin_name: &str) -> Option<&PluginDescription> {
        self.discovery_cache.get_description(plugin_name)
    }

    /// Get all cached plugin descriptions.
    pub fn all_descriptions(&self) -> Vec<&PluginDescription> {
        self.discovery_cache.all_descriptions()
    }

    /// Return all discovered plugin names with their entity types.
    pub fn all_plugins(&self) -> Vec<(String, Vec<String>)> {
        self.discovery_cache.all_plugins()
    }

    /// Check if a plugin has been spawned for a given name.
    #[cfg(test)]
    pub fn is_spawned(&self, plugin_name: &str) -> bool {
        self.spawned.contains_key(plugin_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_unknown_entity_type_is_noop() {
        let cache = PluginDiscoveryCache::build();
        let mut spawner = PluginSpawner::new(cache);

        // This entity type doesn't exist in any plugin
        spawner.ensure_plugin_for_entity_type("nonexistent-entity-type-12345");

        // Should not panic, should be a no-op
        assert!(!spawner.is_spawned("nonexistent"));
    }

    #[test]
    fn repeated_ensure_is_idempotent() {
        let cache = PluginDiscoveryCache::build();
        let mut spawner = PluginSpawner::new(cache);

        // Call twice for the same unknown type
        spawner.ensure_plugin_for_entity_type("nonexistent-entity-type-12345");
        spawner.ensure_plugin_for_entity_type("nonexistent-entity-type-12345");

        // Should not panic
    }
}
