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
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Extended manifest with full plugin description, returned by `provides --describe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifestDescribed {
    pub entity_types: Vec<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Full plugin description with per-entity-type detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginDescription>,
}

/// Check CLI args for a `provides` command, resolve translations via i18n,
/// print the manifest, and return whether it was handled.
///
/// Call this early in `main()` before starting the tokio runtime:
///
/// ```rust,no_run
/// use std::sync::OnceLock;
/// use waft_i18n::I18n;
///
/// static I18N: OnceLock<I18n> = OnceLock::new();
///
/// fn i18n() -> &'static I18n {
///     I18N.get_or_init(|| I18n::new(&[
///         ("en-US", "plugin-name = Clock\nplugin-description = Time display"),
///     ]))
/// }
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     if waft_plugin::manifest::handle_provides_i18n(
///         &["clock"],
///         i18n(),
///         "plugin-name",
///         "plugin-description",
///     ) {
///         return Ok(());
///     }
///     // ... start tokio runtime
///     Ok(())
/// }
/// ```
pub fn handle_provides_i18n(
    entity_types: &[&str],
    i18n: &waft_i18n::I18n,
    name_key: &str,
    description_key: &str,
) -> bool {
    let name = i18n.t(name_key);
    let description = i18n.t(description_key);
    handle_provides_full(entity_types, &name, &description)
}

/// Like [`handle_provides_i18n`], but accepts pre-resolved strings.
pub fn handle_provides_full(
    entity_types: &[&str],
    name: &str,
    description: &str,
) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "provides" {
        let manifest = PluginManifest {
            entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
            name: name.to_string(),
            description: description.to_string(),
        };
        match serde_json::to_string_pretty(&manifest) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("failed to serialize manifest: {e}"),
        }
        return true;
    }
    false
}

/// Combined manifest handler that supports both basic and described manifests.
///
/// When `describe_fn` is `Some` and `provides --describe` is requested, emits a
/// [`PluginManifestDescribed`] with the full plugin description. Otherwise emits
/// a basic [`PluginManifest`].
///
/// Used by [`crate::runner::PluginRunner`] to unify manifest handling.
pub fn handle_manifest(
    entity_types: &[&str],
    name: &str,
    description: &str,
    describe_fn: Option<fn() -> Option<PluginDescription>>,
) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "provides" {
        let want_describe = args.get(2).is_some_and(|a| a == "--describe");

        if want_describe {
            if let Some(f) = describe_fn {
                let manifest = PluginManifestDescribed {
                    entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
                    name: name.to_string(),
                    description: description.to_string(),
                    plugin: f(),
                };
                match serde_json::to_string_pretty(&manifest) {
                    Ok(json) => println!("{json}"),
                    Err(e) => eprintln!("failed to serialize described manifest: {e}"),
                }
            } else {
                print_basic_manifest(entity_types, name, description);
            }
        } else {
            print_basic_manifest(entity_types, name, description);
        }
        return true;
    }
    false
}

fn print_basic_manifest(entity_types: &[&str], name: &str, description: &str) {
    let manifest = PluginManifest {
        entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
        name: name.to_string(),
        description: description.to_string(),
    };
    match serde_json::to_string_pretty(&manifest) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("failed to serialize manifest: {e}"),
    }
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
/// #         -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> { Ok(serde_json::Value::Null) }
/// # }
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let plugin = MyPlugin;
///     if waft_plugin::manifest::handle_provides_described(
///         &["my-entity"],
///         "My Plugin",
///         "Does things",
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
    name: &str,
    description: &str,
    plugin: &P,
) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "provides" {
        let describe = args.get(2).is_some_and(|a| a == "--describe");

        if describe {
            let plugin_desc = plugin.describe();
            let manifest = PluginManifestDescribed {
                entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
                name: name.to_string(),
                description: description.to_string(),
                plugin: plugin_desc,
            };
            match serde_json::to_string_pretty(&manifest) {
                Ok(json) => println!("{json}"),
                Err(e) => eprintln!("failed to serialize described manifest: {e}"),
            }
        } else {
            let manifest = PluginManifest {
                entity_types: entity_types.iter().map(|s| s.to_string()).collect(),
                name: name.to_string(),
                description: description.to_string(),
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
            name: "Clock".to_string(),
            description: "Time display".to_string(),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.entity_types, manifest.entity_types);
        assert_eq!(decoded.name, "Clock");
        assert_eq!(decoded.description, "Time display");
    }

    #[test]
    fn manifest_with_name_and_description() {
        let manifest = PluginManifest {
            entity_types: vec!["night-light".to_string()],
            name: "Sunsetr".to_string(),
            description: "Night light control via sunsetr".to_string(),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let decoded: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "Sunsetr");
        assert_eq!(decoded.description, "Night light control via sunsetr");
    }

    #[test]
    fn manifest_backward_compat_missing_fields() {
        let json = r#"{"entity_types": ["clock"]}"#;
        let decoded: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.entity_types, vec!["clock".to_string()]);
        assert_eq!(decoded.name, "");
        assert_eq!(decoded.description, "");
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
            name: "Audio Control".to_string(),
            description: "Volume control".to_string(),
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
            name: "Clock".to_string(),
            description: String::new(),
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
        assert_eq!(decoded.name, "Clock");
        assert!(decoded.plugin.is_none());
    }
}
