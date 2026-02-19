use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use log::{info, warn};
use serde::{Deserialize, Serialize};
use waft_protocol::description::PropertyValueType;
use waft_protocol::PluginDescription;

/// Manifest returned by `provides` (basic) or `provides --describe` (extended).
///
/// Uses `#[serde(default)]` so both basic and extended manifests deserialize
/// into this struct. When `--describe` is not supported, `plugin` is `None`.
#[derive(Deserialize)]
struct PluginManifestDescribed {
    entity_types: Vec<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    plugin: Option<PluginDescription>,
}

struct DiscoveredPlugin {
    id: String,
    display_name: String,
    description: String,
    entity_types: Vec<String>,
    binary_path: PathBuf,
    plugin_description: Option<PluginDescription>,
}

/// JSON-serializable representation for `waft plugin ls --json`.
#[derive(Serialize)]
struct PluginListEntry {
    id: String,
    name: String,
    entities: Vec<String>,
    description: String,
}

/// Cached mapping from entity type to plugin binary for on-demand spawning.
pub struct PluginDiscoveryCache {
    /// entity_type -> (plugin_name, binary_path)
    type_to_binary: HashMap<String, (String, PathBuf)>,
    /// plugin_name -> PluginDescription (only for plugins that support --describe)
    descriptions: HashMap<String, PluginDescription>,
}

impl PluginDiscoveryCache {
    /// Run plugin discovery and build the cache.
    ///
    /// This queries all `waft-*-daemon` binaries with `provides --describe` to learn
    /// which entity types each plugin supports and optionally obtain full descriptions.
    /// Falls back to basic `provides` output if the plugin doesn't support `--describe`.
    /// Runs discovery in parallel threads with per-binary timeouts.
    pub fn build() -> Self {
        let daemon_dir = detect_daemon_dir();
        let plugins = discover_plugins(&daemon_dir);

        let mut type_to_binary = HashMap::new();
        let mut descriptions = HashMap::new();

        for plugin in &plugins {
            for entity_type in &plugin.entity_types {
                type_to_binary.insert(
                    entity_type.clone(),
                    (plugin.id.clone(), plugin.binary_path.clone()),
                );
            }
            if let Some(ref desc) = plugin.plugin_description {
                descriptions.insert(plugin.id.clone(), desc.clone());
            }
        }

        let desc_count = descriptions.len();
        info!(
            "discovery cache: {} entity types from {} plugins ({desc_count} with descriptions)",
            type_to_binary.len(),
            plugins.len(),
        );

        PluginDiscoveryCache {
            type_to_binary,
            descriptions,
        }
    }

    /// Look up which binary provides a given entity type.
    pub fn binary_for_entity_type(&self, entity_type: &str) -> Option<(&str, &PathBuf)> {
        self.type_to_binary
            .get(entity_type)
            .map(|(name, path)| (name.as_str(), path))
    }

    /// Get the cached description for a specific plugin.
    pub fn get_description(&self, plugin_name: &str) -> Option<&PluginDescription> {
        self.descriptions.get(plugin_name)
    }

    /// Get all cached plugin descriptions.
    pub fn all_descriptions(&self) -> Vec<&PluginDescription> {
        let mut descs: Vec<&PluginDescription> = self.descriptions.values().collect();
        descs.sort_by_key(|d| &d.name);
        descs
    }

    /// Return all discovered plugin names with their entity types.
    ///
    /// Returns a sorted list of (plugin_name, entity_types) pairs.
    pub fn all_plugins(&self) -> Vec<(String, Vec<String>)> {
        let mut plugins: HashMap<String, Vec<String>> = HashMap::new();
        for (entity_type, (plugin_name, _)) in &self.type_to_binary {
            plugins
                .entry(plugin_name.clone())
                .or_default()
                .push(entity_type.clone());
        }
        // Sort entity types within each plugin
        for types in plugins.values_mut() {
            types.sort();
        }
        let mut result: Vec<(String, Vec<String>)> = plugins.into_iter().collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
}

pub fn print_plugin_list(json: bool) {
    let daemon_dir = detect_daemon_dir();
    let plugins = discover_plugins(&daemon_dir);

    if plugins.is_empty() {
        eprintln!("No plugins found in {}", daemon_dir.display());
        return;
    }

    if json {
        print_plugin_list_json(&plugins);
    } else {
        print_plugin_list_text(&plugins);
    }
}

fn print_plugin_list_text(plugins: &[DiscoveredPlugin]) {
    let max_label = plugins
        .iter()
        .map(|p| p.display_name.len() + p.id.len() + 3) // " (id)"
        .max()
        .unwrap_or(0);

    for plugin in plugins {
        let label = format!("{} ({})", plugin.display_name, plugin.id);
        let types = plugin.entity_types.join(", ");
        println!("{:<width$}  {types}", label, width = max_label);
    }
}

fn print_plugin_list_json(plugins: &[DiscoveredPlugin]) {
    let entries: Vec<PluginListEntry> = plugins
        .iter()
        .map(|p| PluginListEntry {
            id: p.id.clone(),
            name: p.display_name.clone(),
            entities: p.entity_types.clone(),
            description: p.description.clone(),
        })
        .collect();

    match serde_json::to_string_pretty(&entries) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("[waft] failed to serialize plugin list: {e}"),
    }
}

/// JSON-serializable representation for `waft plugin describe --json`.
#[derive(Serialize)]
struct PluginDescribeOutput {
    id: String,
    name: String,
    binary: String,
    description: String,
    entity_types: Vec<PluginDescribeEntityType>,
}

/// JSON-serializable entity type info for describe output.
#[derive(Serialize)]
struct PluginDescribeEntityType {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    properties: Vec<PluginDescribeProperty>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    actions: Vec<PluginDescribeAction>,
}

#[derive(Serialize)]
struct PluginDescribeProperty {
    name: String,
    #[serde(rename = "type")]
    value_type: String,
    label: String,
    description: String,
}

#[derive(Serialize)]
struct PluginDescribeAction {
    name: String,
    label: String,
    description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    params: Vec<PluginDescribeActionParam>,
}

#[derive(Serialize)]
struct PluginDescribeActionParam {
    name: String,
    #[serde(rename = "type")]
    value_type: String,
    label: String,
    description: String,
    required: bool,
}

pub fn print_plugin_description(plugin_name: &str, json: bool) {
    let daemon_dir = detect_daemon_dir();
    let plugins = discover_plugins(&daemon_dir);

    let plugin = match plugins.iter().find(|p| p.id == plugin_name) {
        Some(p) => p,
        None => {
            eprintln!(
                "error: plugin '{plugin_name}' not found. Run 'waft plugin ls' to see available plugins."
            );
            std::process::exit(1);
        }
    };

    if json {
        print_describe_json(plugin);
    } else {
        print_describe_text(plugin);
    }
}

fn build_describe_output(plugin: &DiscoveredPlugin) -> PluginDescribeOutput {
    let entity_types = match &plugin.plugin_description {
        Some(desc) => desc
            .entity_types
            .iter()
            .map(|et| PluginDescribeEntityType {
                name: et.entity_type.clone(),
                description: Some(et.description.clone()),
                properties: et
                    .properties
                    .iter()
                    .map(|p| PluginDescribeProperty {
                        name: p.name.clone(),
                        value_type: format_value_type(&p.value_type),
                        label: p.label.clone(),
                        description: p.description.clone(),
                    })
                    .collect(),
                actions: et
                    .actions
                    .iter()
                    .map(|a| PluginDescribeAction {
                        name: a.name.clone(),
                        label: a.label.clone(),
                        description: a.description.clone(),
                        params: a
                            .params
                            .iter()
                            .map(|p| PluginDescribeActionParam {
                                name: p.name.clone(),
                                value_type: format_value_type(&p.value_type),
                                label: p.label.clone(),
                                description: p.description.clone(),
                                required: p.required,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
        None => plugin
            .entity_types
            .iter()
            .map(|name| PluginDescribeEntityType {
                name: name.clone(),
                description: None,
                properties: vec![],
                actions: vec![],
            })
            .collect(),
    };

    PluginDescribeOutput {
        id: plugin.id.clone(),
        name: plugin.display_name.clone(),
        binary: plugin.binary_path.display().to_string(),
        description: plugin.description.clone(),
        entity_types,
    }
}

fn print_describe_json(plugin: &DiscoveredPlugin) {
    let output = build_describe_output(plugin);
    match serde_json::to_string_pretty(&output) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("[waft] failed to serialize plugin description: {e}");
            std::process::exit(1);
        }
    }
}

fn print_describe_text(plugin: &DiscoveredPlugin) {
    println!("Plugin: {} ({})", plugin.display_name, plugin.id);
    println!("Binary: {}", plugin.binary_path.display());

    if !plugin.description.is_empty() {
        println!("{}", plugin.description);
    }

    match &plugin.plugin_description {
        Some(desc) => print_describe_text_full(desc),
        None => print_describe_text_basic(&plugin.entity_types),
    }
}

fn print_describe_text_full(desc: &PluginDescription) {
    if desc.entity_types.is_empty() {
        return;
    }

    println!();
    println!("Entity Types:");

    for (i, et) in desc.entity_types.iter().enumerate() {
        if i > 0 {
            println!();
        }

        println!("  {}", et.entity_type);
        println!("    {}", et.description);

        if !et.properties.is_empty() {
            println!();
            println!("    Properties:");

            let max_name = et.properties.iter().map(|p| p.name.len()).max().unwrap_or(0);
            let max_type = et
                .properties
                .iter()
                .map(|p| format_value_type(&p.value_type).len())
                .max()
                .unwrap_or(0);

            for prop in &et.properties {
                let type_str = format_value_type(&prop.value_type);
                println!(
                    "      {:<nw$}  {:<tw$}  {}",
                    prop.name,
                    type_str,
                    prop.description,
                    nw = max_name,
                    tw = max_type,
                );
            }
        }

        if !et.actions.is_empty() {
            println!();
            println!("    Actions:");

            let max_name = et.actions.iter().map(|a| a.name.len()).max().unwrap_or(0);

            for action in &et.actions {
                println!(
                    "      {:<width$}  {}",
                    action.name,
                    action.description,
                    width = max_name,
                );

                for param in &action.params {
                    let req = if param.required { "required" } else { "optional" };
                    println!(
                        "        {}: {} ({})  {}",
                        param.name,
                        format_value_type(&param.value_type),
                        req,
                        param.description,
                    );
                }
            }
        }
    }
}

fn print_describe_text_basic(entity_types: &[String]) {
    if entity_types.is_empty() {
        return;
    }

    println!();
    println!("Entity Types:");
    for et in entity_types {
        println!("  {et}");
        println!("    (no description available)");
    }
}

fn format_value_type(vt: &PropertyValueType) -> String {
    match vt {
        PropertyValueType::String => "string".to_string(),
        PropertyValueType::Bool => "bool".to_string(),
        PropertyValueType::Number => "number".to_string(),
        PropertyValueType::Percent => "percent".to_string(),
        PropertyValueType::Enum { variants } => {
            let names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
            format!("enum({})", names.join("|"))
        }
        PropertyValueType::Object => "object".to_string(),
        PropertyValueType::Array => "array".to_string(),
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
            let plugin_name = name
                .strip_prefix("waft-")?
                .strip_suffix("-daemon")?
                .to_string();
            Some((plugin_name, entry.path()))
        })
        .collect();

    // Query all manifests in parallel — each query has a timeout so we
    // don't want to pay that cost serially for every plugin.
    let handles: Vec<_> = candidates
        .into_iter()
        .map(|(id, path)| {
            std::thread::spawn(move || {
                let manifest = query_manifest_described(&path)?;
                let display_name = if manifest.name.is_empty() {
                    title_case(&id)
                } else {
                    manifest.name
                };
                Some(DiscoveredPlugin {
                    display_name,
                    description: manifest.description,
                    plugin_description: manifest.plugin,
                    id,
                    entity_types: manifest.entity_types,
                    binary_path: path,
                })
            })
        })
        .collect();

    let mut plugins: Vec<DiscoveredPlugin> =
        handles.into_iter().filter_map(|h| h.join().ok()?).collect();

    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    plugins
}

/// Convert a hyphenated id to title case: `"keyboard-layout"` -> `"Keyboard Layout"`.
fn title_case(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run a plugin binary with the given args and return its stdout if it exits
/// successfully within the timeout.
fn run_binary_with_timeout(binary: &PathBuf, args: &[&str]) -> Option<String> {
    let mut child = match Command::new(binary)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            warn!("failed to run {}: {e}", binary.display());
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
                        warn!("failed to kill {}: {e}", binary.display());
                    }
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                warn!("failed to wait for {}: {e}", binary.display());
                return None;
            }
        }
    };

    if !status.success() {
        return None;
    }

    match child.stdout.take() {
        Some(mut pipe) => {
            use std::io::Read;
            let mut buf = String::new();
            if let Err(e) = pipe.read_to_string(&mut buf) {
                warn!(
                    "failed to read stdout from {}: {e}",
                    binary.display()
                );
                return None;
            }
            Some(buf)
        }
        None => None,
    }
}

/// Query a plugin binary for its extended manifest (`provides --describe`).
/// Falls back to basic `provides` if `--describe` fails.
fn query_manifest_described(binary: &PathBuf) -> Option<PluginManifestDescribed> {
    // Try extended manifest first
    if let Some(stdout) = run_binary_with_timeout(binary, &["provides", "--describe"])
        && let Ok(manifest) = serde_json::from_str::<PluginManifestDescribed>(&stdout)
    {
        return Some(manifest);
    }

    // Fall back to basic manifest
    let stdout = run_binary_with_timeout(binary, &["provides"])?;
    match serde_json::from_str::<PluginManifestDescribed>(&stdout) {
        Ok(manifest) => Some(manifest),
        Err(e) => {
            warn!(
                "failed to parse manifest from {}: {e}",
                binary.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_case_single_word() {
        assert_eq!(title_case("sunsetr"), "Sunsetr");
    }

    #[test]
    fn title_case_hyphenated() {
        assert_eq!(title_case("keyboard-layout"), "Keyboard Layout");
    }

    #[test]
    fn title_case_multiple_hyphens() {
        assert_eq!(title_case("systemd-actions"), "Systemd Actions");
    }

    #[test]
    fn title_case_empty() {
        assert_eq!(title_case(""), "");
    }

    #[test]
    fn json_output_structure() {
        let plugins = vec![DiscoveredPlugin {
            id: "sunsetr".to_string(),
            display_name: "Sunsetr".to_string(),
            description: "Night light control".to_string(),
            entity_types: vec!["night-light".to_string()],
            binary_path: PathBuf::from("/usr/bin/waft-sunsetr-daemon"),
            plugin_description: None,
        }];

        let entries: Vec<PluginListEntry> = plugins
            .iter()
            .map(|p| PluginListEntry {
                id: p.id.clone(),
                name: p.display_name.clone(),
                entities: p.entity_types.clone(),
                description: p.description.clone(),
            })
            .collect();

        let json = serde_json::to_value(&entries).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "sunsetr");
        assert_eq!(arr[0]["name"], "Sunsetr");
        assert_eq!(arr[0]["entities"][0], "night-light");
        assert_eq!(arr[0]["description"], "Night light control");
    }

    #[test]
    fn json_output_empty_description() {
        let entry = PluginListEntry {
            id: "clock".to_string(),
            name: "Clock".to_string(),
            entities: vec!["clock".to_string()],
            description: String::new(),
        };

        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["description"], "");
    }

    #[test]
    fn basic_manifest_parses_as_described() {
        let json = r#"{"entity_types": ["clock"], "name": "Clock"}"#;
        let manifest: PluginManifestDescribed = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.entity_types, vec!["clock"]);
        assert_eq!(manifest.name, "Clock");
        assert!(manifest.plugin.is_none());
    }

    #[test]
    fn described_manifest_with_plugin() {
        let json = r#"{
            "entity_types": ["clock"],
            "name": "Clock",
            "description": "Time display",
            "plugin": {
                "name": "clock",
                "display_name": "Clock",
                "description": "Current time and date display",
                "entity_types": [{
                    "entity_type": "clock",
                    "display_name": "Clock",
                    "description": "Current time",
                    "properties": [],
                    "actions": []
                }]
            }
        }"#;
        let manifest: PluginManifestDescribed = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.entity_types, vec!["clock"]);
        let plugin = manifest.plugin.unwrap();
        assert_eq!(plugin.name, "clock");
        assert_eq!(plugin.entity_types.len(), 1);
    }

    #[test]
    fn format_value_type_simple() {
        assert_eq!(format_value_type(&PropertyValueType::String), "string");
        assert_eq!(format_value_type(&PropertyValueType::Bool), "bool");
        assert_eq!(format_value_type(&PropertyValueType::Number), "number");
        assert_eq!(format_value_type(&PropertyValueType::Percent), "percent");
        assert_eq!(format_value_type(&PropertyValueType::Object), "object");
        assert_eq!(format_value_type(&PropertyValueType::Array), "array");
    }

    #[test]
    fn format_value_type_enum() {
        use waft_protocol::description::EnumVariantDescription;
        let vt = PropertyValueType::Enum {
            variants: vec![
                EnumVariantDescription {
                    name: "Output".to_string(),
                    label: "Audio Output".to_string(),
                },
                EnumVariantDescription {
                    name: "Input".to_string(),
                    label: "Audio Input".to_string(),
                },
            ],
        };
        assert_eq!(format_value_type(&vt), "enum(Output|Input)");
    }

    #[test]
    fn describe_output_with_full_description() {
        use waft_protocol::description::*;

        let plugin = DiscoveredPlugin {
            id: "clock".to_string(),
            display_name: "Clock".to_string(),
            description: "Time display".to_string(),
            entity_types: vec!["clock".to_string()],
            binary_path: PathBuf::from("/usr/bin/waft-clock-daemon"),
            plugin_description: Some(PluginDescription {
                name: "clock".to_string(),
                display_name: "Clock".to_string(),
                description: "Time and date display".to_string(),
                entity_types: vec![EntityTypeDescription {
                    entity_type: "clock".to_string(),
                    display_name: "Clock".to_string(),
                    description: "Current time and date".to_string(),
                    properties: vec![PropertyDescription {
                        name: "time".to_string(),
                        label: "Time".to_string(),
                        description: "Formatted time".to_string(),
                        value_type: PropertyValueType::String,
                    }],
                    actions: vec![ActionDescription {
                        name: "click".to_string(),
                        label: "Click".to_string(),
                        description: "Execute click command".to_string(),
                        params: vec![],
                    }],
                }],
            }),
        };

        let output = build_describe_output(&plugin);
        assert_eq!(output.id, "clock");
        assert_eq!(output.name, "Clock");
        assert_eq!(output.binary, "/usr/bin/waft-clock-daemon");
        assert_eq!(output.description, "Time display");
        assert_eq!(output.entity_types.len(), 1);
        assert_eq!(output.entity_types[0].name, "clock");
        assert_eq!(
            output.entity_types[0].description,
            Some("Current time and date".to_string())
        );
        assert_eq!(output.entity_types[0].properties.len(), 1);
        assert_eq!(output.entity_types[0].properties[0].name, "time");
        assert_eq!(output.entity_types[0].properties[0].value_type, "string");
        assert_eq!(output.entity_types[0].actions.len(), 1);
        assert_eq!(output.entity_types[0].actions[0].name, "click");

        // Verify JSON roundtrip
        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["id"], "clock");
        assert_eq!(json["entity_types"][0]["properties"][0]["type"], "string");
    }

    #[test]
    fn describe_output_without_description() {
        let plugin = DiscoveredPlugin {
            id: "sunsetr".to_string(),
            display_name: "Sunsetr".to_string(),
            description: String::new(),
            entity_types: vec!["night-light".to_string()],
            binary_path: PathBuf::from("/usr/bin/waft-sunsetr-daemon"),
            plugin_description: None,
        };

        let output = build_describe_output(&plugin);
        assert_eq!(output.id, "sunsetr");
        assert_eq!(output.entity_types.len(), 1);
        assert_eq!(output.entity_types[0].name, "night-light");
        assert!(output.entity_types[0].description.is_none());
        assert!(output.entity_types[0].properties.is_empty());
        assert!(output.entity_types[0].actions.is_empty());

        // Verify JSON serialization -- empty vecs are omitted, None is absent
        let json = serde_json::to_string(&output).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            !val["entity_types"][0]
                .as_object()
                .unwrap()
                .contains_key("properties")
        );
        assert!(
            !val["entity_types"][0]
                .as_object()
                .unwrap()
                .contains_key("actions")
        );
    }

    #[test]
    fn describe_output_with_action_params() {
        use waft_protocol::description::*;

        let plugin = DiscoveredPlugin {
            id: "audio".to_string(),
            display_name: "Audio".to_string(),
            description: String::new(),
            entity_types: vec!["audio-device".to_string()],
            binary_path: PathBuf::from("/usr/bin/waft-audio-daemon"),
            plugin_description: Some(PluginDescription {
                name: "audio".to_string(),
                display_name: "Audio".to_string(),
                description: "Volume control".to_string(),
                entity_types: vec![EntityTypeDescription {
                    entity_type: "audio-device".to_string(),
                    display_name: "Audio Device".to_string(),
                    description: "An audio device".to_string(),
                    properties: vec![],
                    actions: vec![ActionDescription {
                        name: "set-volume".to_string(),
                        label: "Set Volume".to_string(),
                        description: "Adjust volume".to_string(),
                        params: vec![ActionParamDescription {
                            name: "value".to_string(),
                            label: "Volume".to_string(),
                            description: "Volume level".to_string(),
                            required: true,
                            value_type: PropertyValueType::Percent,
                        }],
                    }],
                }],
            }),
        };

        let output = build_describe_output(&plugin);
        assert_eq!(output.entity_types[0].actions[0].params.len(), 1);
        assert_eq!(output.entity_types[0].actions[0].params[0].name, "value");
        assert_eq!(
            output.entity_types[0].actions[0].params[0].value_type,
            "percent"
        );
        assert!(output.entity_types[0].actions[0].params[0].required);
    }

    #[test]
    fn discovery_cache_description_methods() {
        let cache = PluginDiscoveryCache {
            type_to_binary: HashMap::new(),
            descriptions: {
                let mut m = HashMap::new();
                m.insert(
                    "clock".to_string(),
                    PluginDescription {
                        name: "clock".to_string(),
                        display_name: "Clock".to_string(),
                        description: "Time display".to_string(),
                        entity_types: vec![],
                    },
                );
                m
            },
        };

        assert!(cache.get_description("clock").is_some());
        assert!(cache.get_description("unknown").is_none());
        assert_eq!(cache.all_descriptions().len(), 1);
    }
}
