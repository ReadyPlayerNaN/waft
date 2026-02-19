//! Plugin manifest for entity type discovery.
//!
//! Each plugin binary supports a `provides` CLI argument that prints
//! a JSON manifest listing the entity types it can provide. This allows
//! the daemon to discover plugin capabilities without starting them.
//!
//! The extended form `provides --describe` includes per-entity-type
//! descriptions (properties, actions) for self-documenting plugins.

use serde::{Deserialize, Serialize};
use waft_protocol::PluginDescription;

use crate::plugin::Plugin;

/// Manifest describing what entity types a plugin provides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub entity_types: Vec<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Extended manifest with full plugin description, returned by `provides --describe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifestDescribed {
    pub entity_types: Vec<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Full plugin description with per-entity-type detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginDescription>,
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
    handle_provides_full(entity_types, None, None)
}

/// Like [`handle_provides`], but also includes a display name and description
/// in the manifest for richer CLI output.
pub fn handle_provides_full(
    entity_types: &[&str],
    name: Option<&str>,
    description: Option<&str>,
) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "provides" {
        let manifest = PluginManifest {
            entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
            name: name.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
        };
        match serde_json::to_string_pretty(&manifest) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("failed to serialize manifest: {e}"),
        }
        return true;
    }
    false
}

/// Like [`handle_provides_full`], but when `provides --describe` is requested,
/// also includes the full plugin description from [`Plugin::describe()`].
///
/// Falls back to the basic manifest when `--describe` is not requested or when
/// the plugin returns `None` from `describe()`.
///
/// ```rust,no_run
/// use waft_plugin::Plugin;
///
/// # struct MyPlugin;
/// # #[async_trait::async_trait]
/// # impl Plugin for MyPlugin {
/// #     fn get_entities(&self) -> Vec<waft_plugin::Entity> { vec![] }
/// #     async fn handle_action(&self, _: waft_plugin::Urn, _: String, _: serde_json::Value)
/// #         -> Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }
/// # }
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let plugin = MyPlugin;
///     if waft_plugin::manifest::handle_provides_described(
///         &["my-entity"],
///         Some("My Plugin"),
///         Some("Does things"),
///         &plugin,
///     ) {
///         return Ok(());
///     }
///     // ... start tokio runtime
///     Ok(())
/// }
/// ```
pub fn handle_provides_described<P: Plugin>(
    entity_types: &[&str],
    name: Option<&str>,
    description: Option<&str>,
    plugin: &P,
) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "provides" {
        let describe = args.get(2).is_some_and(|a| a == "--describe");

        if describe {
            let plugin_desc = plugin.describe();
            let manifest = PluginManifestDescribed {
                entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
                name: name.map(|s| s.to_string()),
                description: description.map(|s| s.to_string()),
                plugin: plugin_desc,
            };
            match serde_json::to_string_pretty(&manifest) {
                Ok(json) => println!("{json}"),
                Err(e) => eprintln!("failed to serialize described manifest: {e}"),
            }
        } else {
            let manifest = PluginManifest {
                entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
                name: name.map(|s| s.to_string()),
                description: description.map(|s| s.to_string()),
            };
            match serde_json::to_string_pretty(&manifest) {
                Ok(json) => println!("{json}"),
                Err(e) => eprintln!("failed to serialize manifest: {e}"),
            }
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::description::*;

    #[test]
    fn manifest_serde_roundtrip() {
        let manifest = PluginManifest {
            entity_types: vec!["clock".to_string(), "dark-mode".to_string()],
            name: None,
            description: None,
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.entity_types, manifest.entity_types);
        assert_eq!(decoded.name, None);
        assert_eq!(decoded.description, None);
    }

    #[test]
    fn manifest_with_name_and_description() {
        let manifest = PluginManifest {
            entity_types: vec!["night-light".to_string()],
            name: Some("Sunsetr".to_string()),
            description: Some("Night light control via sunsetr".to_string()),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, Some("Sunsetr".to_string()));
        assert_eq!(
            decoded.description,
            Some("Night light control via sunsetr".to_string())
        );
    }

    #[test]
    fn manifest_backward_compat_missing_fields() {
        let json = r#"{"entity_types": ["clock"]}"#;
        let decoded: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.entity_types, vec!["clock".to_string()]);
        assert_eq!(decoded.name, None);
        assert_eq!(decoded.description, None);
    }

    #[test]
    fn described_manifest_roundtrip_with_description() {
        let desc = PluginDescription {
            name: "audio".to_string(),
            display_name: "Audio Control".to_string(),
            description: "Volume control".to_string(),
            entity_types: vec![EntityTypeDescription {
                entity_type: "audio-device".to_string(),
                display_name: "Audio Device".to_string(),
                description: "An audio device".to_string(),
                properties: vec![],
                actions: vec![],
            }],
        };

        let manifest = PluginManifestDescribed {
            entity_types: vec!["audio-device".to_string()],
            name: Some("Audio Control".to_string()),
            description: Some("Volume control".to_string()),
            plugin: Some(desc.clone()),
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: PluginManifestDescribed = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.entity_types, vec!["audio-device".to_string()]);
        assert_eq!(decoded.plugin.unwrap().name, "audio");
    }

    #[test]
    fn described_manifest_without_plugin_field() {
        let manifest = PluginManifestDescribed {
            entity_types: vec!["clock".to_string()],
            name: Some("Clock".to_string()),
            description: None,
            plugin: None,
        };

        let json = serde_json::to_string(&manifest).unwrap();
        // plugin field should be omitted entirely
        assert!(!json.contains("\"plugin\""));

        let decoded: PluginManifestDescribed = serde_json::from_str(&json).unwrap();
        assert!(decoded.plugin.is_none());
    }

    #[test]
    fn basic_manifest_deserializes_as_described() {
        // A basic PluginManifest JSON should deserialize into PluginManifestDescribed
        // with plugin = None (backward compatibility).
        let json = r#"{"entity_types": ["clock"], "name": "Clock"}"#;
        let decoded: PluginManifestDescribed = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.entity_types, vec!["clock".to_string()]);
        assert_eq!(decoded.name, Some("Clock".to_string()));
        assert!(decoded.plugin.is_none());
    }
}
