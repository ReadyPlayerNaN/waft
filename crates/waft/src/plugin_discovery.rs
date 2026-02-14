use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
struct PluginManifest {
    entity_types: Vec<String>,
}

struct DiscoveredPlugin {
    name: String,
    entity_types: Vec<String>,
    binary_path: PathBuf,
}

/// Cached mapping from entity type to plugin binary for on-demand spawning.
pub struct PluginDiscoveryCache {
    /// entity_type -> (plugin_name, binary_path)
    type_to_binary: HashMap<String, (String, PathBuf)>,
}

impl PluginDiscoveryCache {
    /// Run plugin discovery and build the cache.
    ///
    /// This queries all `waft-*-daemon` binaries with `provides` to learn
    /// which entity types each plugin supports. Runs discovery in parallel
    /// threads with per-binary timeouts.
    pub fn build() -> Self {
        let daemon_dir = detect_daemon_dir();
        let plugins = discover_plugins(&daemon_dir);

        let mut type_to_binary = HashMap::new();
        for plugin in &plugins {
            for entity_type in &plugin.entity_types {
                type_to_binary.insert(
                    entity_type.clone(),
                    (plugin.name.clone(), plugin.binary_path.clone()),
                );
            }
        }

        eprintln!(
            "[waft] discovery cache: {} entity types from {} plugins",
            type_to_binary.len(),
            plugins.len(),
        );

        PluginDiscoveryCache { type_to_binary }
    }

    /// Look up which binary provides a given entity type.
    pub fn binary_for_entity_type(&self, entity_type: &str) -> Option<(&str, &PathBuf)> {
        self.type_to_binary
            .get(entity_type)
            .map(|(name, path)| (name.as_str(), path))
    }

}

pub fn print_plugin_list() {
    let daemon_dir = detect_daemon_dir();
    let plugins = discover_plugins(&daemon_dir);

    if plugins.is_empty() {
        eprintln!("No plugins found in {}", daemon_dir.display());
        return;
    }

    let max_name = plugins.iter().map(|p| p.name.len()).max().unwrap_or(0);

    for plugin in &plugins {
        let types = plugin.entity_types.join(", ");
        println!("{:<width$}  {types}", plugin.name, width = max_name);
    }
}

fn detect_daemon_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("WAFT_DAEMON_DIR") {
        return PathBuf::from(dir);
    }

    let debug_dir = PathBuf::from("./target/debug");
    if debug_dir.join("waft-clock-daemon").exists() {
        return debug_dir;
    }

    let release_dir = PathBuf::from("./target/release");
    if release_dir.join("waft-clock-daemon").exists() {
        return release_dir;
    }

    PathBuf::from("/usr/bin")
}

fn discover_plugins(dir: &PathBuf) -> Vec<DiscoveredPlugin> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read daemon directory {}: {e}", dir.display());
            return Vec::new();
        }
    };

    let candidates: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let plugin_name = name.strip_prefix("waft-")?.strip_suffix("-daemon")?.to_string();
            Some((plugin_name, entry.path()))
        })
        .collect();

    // Query all manifests in parallel — each query has a timeout so we
    // don't want to pay that cost serially for every plugin.
    let handles: Vec<_> = candidates
        .into_iter()
        .map(|(name, path)| {
            std::thread::spawn(move || {
                let entity_types = query_manifest(&path)?;
                Some(DiscoveredPlugin {
                    name,
                    entity_types,
                    binary_path: path,
                })
            })
        })
        .collect();

    let mut plugins: Vec<DiscoveredPlugin> = handles
        .into_iter()
        .filter_map(|h| h.join().ok()?)
        .collect();

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

fn query_manifest(binary: &PathBuf) -> Option<Vec<String>> {
    let mut child = match Command::new(binary)
        .arg("provides")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("[waft] failed to run {}: {e}", binary.display());
            return None;
        }
    };

    // Plugins that don't support `provides` start their daemon loop instead
    // of exiting, so we poll with a timeout and kill if needed.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    if let Err(e) = child.kill() {
                        eprintln!("[waft] failed to kill {}: {e}", binary.display());
                    }
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                eprintln!("[waft] failed to wait for {}: {e}", binary.display());
                return None;
            }
        }
    };

    if !status.success() {
        return None;
    }

    let stdout = match child.stdout.take() {
        Some(mut pipe) => {
            use std::io::Read;
            let mut buf = String::new();
            if let Err(e) = pipe.read_to_string(&mut buf) {
                eprintln!("[waft] failed to read stdout from {}: {e}", binary.display());
                return None;
            }
            buf
        }
        None => return None,
    };

    match serde_json::from_str::<PluginManifest>(&stdout) {
        Ok(manifest) => Some(manifest.entity_types),
        Err(e) => {
            eprintln!(
                "[waft] failed to parse manifest from {}: {e}",
                binary.display()
            );
            None
        }
    }
}
