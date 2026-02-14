//! On-demand plugin spawning for the waft daemon.
//!
//! When an app subscribes to an entity type and no plugin currently provides it,
//! `PluginSpawner` looks up the binary from the discovery cache and spawns it.
//! Child processes are tracked and properly reaped.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Child;

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

        let (plugin_name, binary_path) =
            match self.discovery_cache.binary_for_entity_type(entity_type) {
                Some((name, path)) => (name.to_string(), path.clone()),
                None => {
                    eprintln!("[waft] no plugin known for entity type '{entity_type}'");
                    return;
                }
            };

        // Already spawned this plugin (it may provide multiple entity types)
        if self.spawned.contains_key(&plugin_name) {
            self.spawn_attempted.insert(entity_type.to_string());
            return;
        }

        self.spawn_plugin(&plugin_name, &binary_path);
        self.spawn_attempted.insert(entity_type.to_string());
    }

    fn spawn_plugin(&mut self, plugin_name: &str, binary_path: &PathBuf) {
        eprintln!(
            "[waft] spawning plugin '{}' from {}",
            plugin_name,
            binary_path.display()
        );

        match std::process::Command::new(binary_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
        {
            Ok(child) => {
                let pid = child.id();
                eprintln!("[waft] spawned plugin '{}' (pid {})", plugin_name, pid);

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
                                    eprintln!("[waft] plugin '{name}' exited with {status}");
                                }
                                Err(e) => {
                                    eprintln!("[waft] failed to wait for plugin '{name}': {e}");
                                }
                            }
                        })
                        .unwrap_or_else(|e| {
                            eprintln!(
                                "[waft] failed to spawn reaper thread for '{plugin_name}': {e}"
                            );
                            // At minimum, we tried. The child will be reaped on daemon exit.
                            std::thread::spawn(|| {})
                        });
                }

                self.spawned.insert(plugin_name.to_string(), spawned);
            }
            Err(e) => {
                eprintln!("[waft] failed to spawn plugin '{}': {e}", plugin_name,);
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
            // Clear entity types that belong to this plugin
            self.discovery_cache
                .binary_for_entity_type(et)
                .map(|(name, _)| name != plugin_name)
                .unwrap_or(true)
        });
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
