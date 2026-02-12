//! Plugin manifest for entity type discovery.
//!
//! Each plugin binary supports a `provides` CLI argument that prints
//! a JSON manifest listing the entity types it can provide. This allows
//! the daemon to discover plugin capabilities without starting them.

use serde::{Deserialize, Serialize};

/// Manifest describing what entity types a plugin provides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub entity_types: Vec<String>,
}

/// Check CLI args for a `provides` command, print the manifest, and return
/// whether it was handled.
///
/// Call this early in `main()` before starting the tokio runtime:
///
/// ```rust,no_run
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     if waft_plugin::manifest::handle_provides(&["clock"]) {
///         return Ok(());
///     }
///     // ... start tokio runtime
///     Ok(())
/// }
/// ```
pub fn handle_provides(entity_types: &[&str]) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "provides" {
        let manifest = PluginManifest {
            entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
        };
        match serde_json::to_string_pretty(&manifest) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("failed to serialize manifest: {e}"),
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_serde_roundtrip() {
        let manifest = PluginManifest {
            entity_types: vec!["clock".to_string(), "dark-mode".to_string()],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.entity_types, manifest.entity_types);
    }
}
