# Darkman Configuration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add configuration UI for darkman's automatic dark mode switching in waft-settings Display page

**Architecture:** Generalized capability-based entity with rich schema metadata. Plugin manages `~/.config/darkman/config.yaml` directly. Settings UI renders widgets based on schema fields.

**Tech Stack:** waft-protocol (entity types), darkman plugin (YAML config, entity emission), waft-settings (GTK4/libadwaita UI), serde_yaml (YAML parsing)

---

## Task 1: Protocol Layer - Entity Type Definitions

**Files:**
- Modify: `crates/protocol/src/entity/display.rs`
- Modify: `crates/protocol/src/entity/mod.rs`

**Step 1: Write test for entity serialization**

Add to end of `crates/protocol/src/entity/display.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_dark_mode_automation_config_serialization() {
        let mut fields = HashMap::new();
        fields.insert("latitude".into(), FieldSchema {
            available: true,
            state: FieldState::Editable,
            field_type: FieldType::Float { decimals: 2 },
            constraints: Some(Constraints {
                min: Some(-90.0),
                max: Some(90.0),
                step: Some(0.01),
            }),
            help_text: Some("Test help".into()),
        });

        let config = DarkModeAutomationConfig {
            latitude: Some(50.08),
            longitude: Some(14.42),
            auto_location: Some(true),
            dbus_api: Some(true),
            portal_api: Some(true),
            schema: ConfigSchema { fields },
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("latitude"));
        assert!(json.contains("50.08"));

        let deserialized: DarkModeAutomationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.latitude, Some(50.08));
        assert_eq!(deserialized.auto_location, Some(true));
    }

    #[test]
    fn test_field_state_variants() {
        let editable = FieldState::Editable;
        let readonly = FieldState::ReadOnly;
        let disabled = FieldState::Disabled;

        let json1 = serde_json::to_string(&editable).unwrap();
        let json2 = serde_json::to_string(&readonly).unwrap();
        let json3 = serde_json::to_string(&disabled).unwrap();

        assert_ne!(json1, json2);
        assert_ne!(json2, json3);
    }

    #[test]
    fn test_field_type_variants() {
        let bool_type = FieldType::Bool;
        let float_type = FieldType::Float { decimals: 2 };
        let string_type = FieldType::String;
        let enum_type = FieldType::Enum { options: vec!["a".into(), "b".into()] };

        let json = serde_json::to_string(&float_type).unwrap();
        let deserialized: FieldType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, float_type);
    }
}
```

**Step 2: Run test to verify it fails**

Run:
```bash
cargo test -p waft-protocol test_dark_mode_automation_config_serialization
```

Expected: FAIL with "cannot find type `DarkModeAutomationConfig` in this scope"

**Step 3: Add entity type definitions**

Add before the `#[cfg(test)]` section in `crates/protocol/src/entity/display.rs`:

```rust
use std::collections::HashMap;

/// Dark mode automation configuration entity.
/// Generalized across dark mode switching tools (darkman, Yin-Yang, Blueblack, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DarkModeAutomationConfig {
    // Data fields (all Optional for capability-based support)
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub auto_location: Option<bool>,
    pub dbus_api: Option<bool>,
    pub portal_api: Option<bool>,

    // Rich schema describing field availability and constraints
    pub schema: ConfigSchema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub fields: HashMap<String, FieldSchema>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub available: bool,           // Is this field supported by the tool?
    pub state: FieldState,         // Editable/ReadOnly/Disabled
    pub field_type: FieldType,     // Bool/Float/String/Enum
    pub constraints: Option<Constraints>,
    pub help_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldState {
    Editable,   // User can modify
    ReadOnly,   // Display only, cannot modify
    Disabled,   // Not relevant in current context
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldType {
    Bool,
    Float { decimals: u8 },  // e.g., decimals: 2 for lat/lng
    String,
    Enum { options: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraints {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
}

pub const DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE: &str = "dark-mode-automation-config";
```

**Step 4: Run test to verify it passes**

Run:
```bash
cargo test -p waft-protocol test_dark_mode_automation_config_serialization
```

Expected: PASS (3 tests)

**Step 5: Export types in mod.rs**

Add to `crates/protocol/src/entity/mod.rs` in the display module exports:

```rust
pub use self::display::{
    // ... existing exports ...
    ConfigSchema, Constraints, DarkModeAutomationConfig, FieldSchema, FieldState, FieldType,
    DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,
};
```

**Step 6: Verify workspace builds**

Run:
```bash
cargo build -p waft-protocol
```

Expected: SUCCESS

**Step 7: Commit**

```bash
git add crates/protocol/src/entity/display.rs crates/protocol/src/entity/mod.rs
git commit -m "feat(protocol): add dark mode automation config entity types

Add generalized entity structure for dark mode switcher configuration.
Includes capability-based optional fields and rich schema metadata.

- DarkModeAutomationConfig: Main entity with lat/lng, auto-location, API flags
- ConfigSchema/FieldSchema: Schema metadata with constraints and help text
- FieldState: Editable/ReadOnly/Disabled states
- FieldType: Bool/Float/String/Enum type descriptors
- Constraints: Min/max/step validation rules

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Plugin Config Module - YAML Parsing and Schema

**Files:**
- Create: `plugins/darkman/src/config.rs`
- Modify: `plugins/darkman/src/lib.rs` (add `mod config;`)
- Modify: `plugins/darkman/Cargo.toml`

**Step 1: Add serde_yaml dependency**

Add to `plugins/darkman/Cargo.toml` in `[dependencies]`:

```toml
serde_yaml = "0.9"
dirs = "5.0"
```

**Step 2: Write tests for YAML parsing**

Create `plugins/darkman/src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_config() {
        let yaml = r#"
lat: 50.08
lng: 14.42
usegeoclue: true
dbusserver: true
portal: true
"#;
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.lat, Some(50.08));
        assert_eq!(config.lng, Some(14.42));
        assert_eq!(config.usegeoclue, Some(true));
        assert_eq!(config.dbusserver, Some(true));
        assert_eq!(config.portal, Some(true));
    }

    #[test]
    fn test_parse_partial_config() {
        let yaml = r#"
lat: 52.52
lng: 13.40
"#;
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.lat, Some(52.52));
        assert_eq!(config.lng, Some(13.40));
        assert_eq!(config.usegeoclue, None);
        assert_eq!(config.dbusserver, None);
        assert_eq!(config.portal, None);
    }

    #[test]
    fn test_parse_empty_config() {
        let yaml = "";
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap_or_default();
        assert_eq!(config, DarkmanYamlConfig::default());
    }

    #[test]
    fn test_build_schema_contains_all_fields() {
        let schema = build_schema();
        assert!(schema.fields.contains_key("latitude"));
        assert!(schema.fields.contains_key("longitude"));
        assert!(schema.fields.contains_key("auto_location"));
        assert!(schema.fields.contains_key("dbus_api"));
        assert!(schema.fields.contains_key("portal_api"));
        assert_eq!(schema.fields.len(), 5);
    }

    #[test]
    fn test_latitude_field_schema() {
        let schema = build_schema();
        let lat_schema = &schema.fields["latitude"];
        assert!(lat_schema.available);
        assert_eq!(lat_schema.state, waft_protocol::entity::display::FieldState::Editable);
        assert_eq!(lat_schema.field_type, waft_protocol::entity::display::FieldType::Float { decimals: 2 });
        assert!(lat_schema.constraints.is_some());
        let constraints = lat_schema.constraints.as_ref().unwrap();
        assert_eq!(constraints.min, Some(-90.0));
        assert_eq!(constraints.max, Some(90.0));
        assert_eq!(constraints.step, Some(0.01));
    }

    #[test]
    fn test_validate_latitude_in_range() {
        let value = serde_json::json!(52.52);
        assert!(validate_field("latitude", &value).is_ok());
    }

    #[test]
    fn test_validate_latitude_out_of_range_high() {
        let value = serde_json::json!(100.0);
        assert!(validate_field("latitude", &value).is_err());
    }

    #[test]
    fn test_validate_latitude_out_of_range_low() {
        let value = serde_json::json!(-100.0);
        assert!(validate_field("latitude", &value).is_err());
    }

    #[test]
    fn test_validate_longitude_in_range() {
        let value = serde_json::json!(14.42);
        assert!(validate_field("longitude", &value).is_ok());
    }

    #[test]
    fn test_validate_longitude_out_of_range() {
        let value = serde_json::json!(200.0);
        assert!(validate_field("longitude", &value).is_err());
    }

    #[test]
    fn test_validate_bool_fields() {
        let value = serde_json::json!(true);
        assert!(validate_field("auto_location", &value).is_ok());
        assert!(validate_field("dbus_api", &value).is_ok());
        assert!(validate_field("portal_api", &value).is_ok());
    }

    #[test]
    fn test_validate_unknown_field() {
        let value = serde_json::json!(42);
        assert!(validate_field("unknown_field", &value).is_err());
    }
}
```

**Step 3: Run tests to verify they fail**

Run:
```bash
cargo test -p darkman
```

Expected: FAIL with "cannot find type `DarkmanYamlConfig` in this scope"

**Step 4: Implement YAML config struct and parsing**

Add to top of `plugins/darkman/src/config.rs` (before tests):

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use waft_protocol::entity::display::{
    ConfigSchema, Constraints, FieldSchema, FieldState, FieldType,
};

/// Darkman YAML configuration structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DarkmanYamlConfig {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub usegeoclue: Option<bool>,
    pub dbusserver: Option<bool>,
    pub portal: Option<bool>,
}

/// Parse darkman config from ~/.config/darkman/config.yaml
pub fn parse_darkman_config() -> Result<DarkmanYamlConfig> {
    let config_path = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("No config directory"))?
        .join("darkman/config.yaml");

    if !config_path.exists() {
        log::warn!("[darkman-config] Config file not found, using defaults");
        return Ok(DarkmanYamlConfig::default());
    }

    let yaml_str = std::fs::read_to_string(&config_path)
        .context("Failed to read config file")?;

    match serde_yaml::from_str(&yaml_str) {
        Ok(config) => Ok(config),
        Err(e) => {
            log::error!("[darkman-config] Failed to parse config.yaml: {}", e);
            Err(anyhow::anyhow!("Config file has syntax errors: {}", e))
        }
    }
}

/// Write darkman config to ~/.config/darkman/config.yaml with backup
pub fn write_darkman_config(config: &DarkmanYamlConfig) -> Result<()> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("No config directory"))?
        .join("darkman");

    std::fs::create_dir_all(&config_dir)
        .context("Failed to create darkman config directory")?;

    let config_path = config_dir.join("config.yaml");
    let yaml_str = serde_yaml::to_string(config)
        .context("Failed to serialize config to YAML")?;

    std::fs::write(&config_path, yaml_str)
        .context("Failed to write config file")?;

    Ok(())
}

/// Build schema metadata for darkman's 5 configuration fields
pub fn build_schema() -> ConfigSchema {
    let mut fields = HashMap::new();

    fields.insert("latitude".into(), FieldSchema {
        available: true,
        state: FieldState::Editable,
        field_type: FieldType::Float { decimals: 2 },
        constraints: Some(Constraints {
            min: Some(-90.0),
            max: Some(90.0),
            step: Some(0.01),
        }),
        help_text: Some("Latitude for sunrise/sunset calculation".into()),
    });

    fields.insert("longitude".into(), FieldSchema {
        available: true,
        state: FieldState::Editable,
        field_type: FieldType::Float { decimals: 2 },
        constraints: Some(Constraints {
            min: Some(-180.0),
            max: Some(180.0),
            step: Some(0.01),
        }),
        help_text: Some("Longitude for sunrise/sunset calculation".into()),
    });

    fields.insert("auto_location".into(), FieldSchema {
        available: true,
        state: FieldState::Editable,
        field_type: FieldType::Bool,
        constraints: None,
        help_text: Some("Auto-detect location via geoclue".into()),
    });

    fields.insert("dbus_api".into(), FieldSchema {
        available: true,
        state: FieldState::Editable,
        field_type: FieldType::Bool,
        constraints: None,
        help_text: Some("Enable D-Bus API (required for waft)".into()),
    });

    fields.insert("portal_api".into(), FieldSchema {
        available: true,
        state: FieldState::Editable,
        field_type: FieldType::Bool,
        constraints: None,
        help_text: Some("Enable XDG portal support".into()),
    });

    ConfigSchema { fields }
}

/// Validate field value against constraints
pub fn validate_field(field: &str, value: &serde_json::Value) -> Result<()> {
    match field {
        "latitude" => {
            let lat: f64 = serde_json::from_value(value.clone())
                .context("Latitude must be a number")?;
            if lat < -90.0 || lat > 90.0 {
                return Err(anyhow::anyhow!("Latitude must be between -90 and 90"));
            }
        }
        "longitude" => {
            let lng: f64 = serde_json::from_value(value.clone())
                .context("Longitude must be a number")?;
            if lng < -180.0 || lng > 180.0 {
                return Err(anyhow::anyhow!("Longitude must be between -180 and 180"));
            }
        }
        "auto_location" | "dbus_api" | "portal_api" => {
            let _: bool = serde_json::from_value(value.clone())
                .context("Value must be a boolean")?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown field: {}", field));
        }
    }
    Ok(())
}

/// Restart darkman service via systemctl (best-effort)
pub async fn restart_darkman_service() -> Result<()> {
    let output = tokio::process::Command::new("systemctl")
        .args(["--user", "restart", "darkman.service"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            log::info!("[darkman-config] Service restarted successfully");
            Ok(())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(anyhow::anyhow!("systemctl restart failed: {}", stderr))
        }
        Err(e) => {
            Err(anyhow::anyhow!("Failed to execute systemctl: {}", e))
        }
    }
}
```

**Step 5: Add config module to lib.rs**

Add to top of `plugins/darkman/src/lib.rs` after existing mod declarations:

```rust
pub mod config;
```

**Step 6: Run tests to verify they pass**

Run:
```bash
cargo test -p darkman config::tests
```

Expected: PASS (13 tests)

**Step 7: Commit**

```bash
git add plugins/darkman/src/config.rs plugins/darkman/src/lib.rs plugins/darkman/Cargo.toml
git commit -m "feat(darkman): add config module for YAML parsing and schema

Add config.rs module with:
- DarkmanYamlConfig struct matching YAML structure
- parse_darkman_config() and write_darkman_config() functions
- build_schema() generating rich field metadata
- validate_field() for constraint checking
- restart_darkman_service() for systemctl integration

Includes comprehensive unit tests for all functions.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Plugin Integration - Emit Config Entity and Handle Actions

**Files:**
- Modify: `plugins/darkman/bin/waft-darkman-daemon.rs`
- Modify: `plugins/darkman/src/lib.rs`

**Step 1: Write integration test for config entity emission**

Add to end of `plugins/darkman/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_entity_in_get_entities() {
        // This will be an integration-level check that config_entity() is called
        // We'll verify this compiles and structure is correct
        let yaml_config = config::DarkmanYamlConfig {
            lat: Some(50.08),
            lng: Some(14.42),
            usegeoclue: Some(true),
            dbusserver: Some(true),
            portal: Some(true),
        };

        let schema = config::build_schema();
        let config_entity = waft_protocol::entity::display::DarkModeAutomationConfig {
            latitude: yaml_config.lat,
            longitude: yaml_config.lng,
            auto_location: yaml_config.usegeoclue,
            dbus_api: yaml_config.dbusserver,
            portal_api: yaml_config.portal,
            schema,
        };

        assert_eq!(config_entity.latitude, Some(50.08));
        assert_eq!(config_entity.auto_location, Some(true));
        assert!(config_entity.schema.fields.contains_key("latitude"));
    }
}
```

**Step 2: Run test to verify structure**

Run:
```bash
cargo test -p darkman tests::test_config_entity
```

Expected: PASS

**Step 3: Update DarkmanPlugin struct to include yaml_config**

Modify `DarkmanPlugin` struct in `plugins/darkman/src/lib.rs`:

```rust
/// Darkman plugin.
struct DarkmanPlugin {
    #[allow(dead_code)]
    config: DarkmanConfig,
    state: Arc<StdMutex<DarkmanState>>,
    conn: Connection,
    yaml_config: Arc<StdMutex<config::DarkmanYamlConfig>>,  // NEW
}
```

**Step 4: Update DarkmanPlugin::new() to load yaml config**

Modify `DarkmanPlugin::new()` in `plugins/darkman/src/lib.rs`:

```rust
async fn new() -> Result<Self> {
    let config: DarkmanConfig =
        waft_plugin::config::load_plugin_config("darkman").unwrap_or_default();
    log::debug!("Darkman config: {config:?}");

    let conn = Connection::session()
        .await
        .context("failed to connect to session bus")?;

    let mode = Self::get_mode(&conn).await.unwrap_or_default();
    log::info!("Initial darkman mode: {mode:?}");

    // NEW: Load YAML config
    let yaml_config = config::parse_darkman_config().unwrap_or_default();
    log::debug!("Darkman YAML config: {:?}", yaml_config);

    Ok(Self {
        config,
        state: Arc::new(StdMutex::new(DarkmanState { mode })),
        conn,
        yaml_config: Arc::new(StdMutex::new(yaml_config)),  // NEW
    })
}
```

**Step 5: Add config_entity() method**

Add new method to `impl DarkmanPlugin` in `plugins/darkman/src/lib.rs`:

```rust
/// Build config entity from current YAML config
fn config_entity(&self) -> Entity {
    let yaml_config = match self.yaml_config.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => {
            log::warn!("Mutex poisoned, recovering: {e}");
            e.into_inner().clone()
        }
    };

    let schema = config::build_schema();
    let config_data = waft_protocol::entity::display::DarkModeAutomationConfig {
        latitude: yaml_config.lat,
        longitude: yaml_config.lng,
        auto_location: yaml_config.usegeoclue,
        dbus_api: yaml_config.dbusserver,
        portal_api: yaml_config.portal,
        schema,
    };

    Entity::new(
        Urn::new("darkman", waft_protocol::entity::display::DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE, "default"),
        waft_protocol::entity::display::DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,
        &config_data,
    )
}
```

**Step 6: Update get_entities() to include config entity**

Modify `get_entities()` in `impl Plugin for DarkmanPlugin`:

```rust
fn get_entities(&self) -> Vec<Entity> {
    let mode = self.current_mode();
    let dark_mode = entity::display::DarkMode {
        active: mode.active(),
    };
    vec![
        Entity::new(
            Urn::new("darkman", entity::display::DARK_MODE_ENTITY_TYPE, "default"),
            entity::display::DARK_MODE_ENTITY_TYPE,
            &dark_mode,
        ),
        self.config_entity(),  // NEW
    ]
}
```

**Step 7: Add update_config_field() method**

Add new method to `impl DarkmanPlugin`:

```rust
/// Update a config field and write to disk
async fn update_config_field(&self, field: &str, value: serde_json::Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Validate field
    config::validate_field(field, &value)?;

    // Lock and update yaml config
    let mut yaml_config = match self.yaml_config.lock() {
        Ok(guard) => guard,
        Err(e) => {
            log::warn!("Mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };

    // Apply field update
    match field {
        "latitude" => {
            yaml_config.lat = Some(serde_json::from_value(value)?);
        }
        "longitude" => {
            yaml_config.lng = Some(serde_json::from_value(value)?);
        }
        "auto_location" => {
            yaml_config.usegeoclue = Some(serde_json::from_value(value)?);
        }
        "dbus_api" => {
            yaml_config.dbusserver = Some(serde_json::from_value(value)?);
        }
        "portal_api" => {
            yaml_config.portal = Some(serde_json::from_value(value)?);
        }
        _ => {
            return Err(format!("Unknown field: {}", field).into());
        }
    }

    // Backup and write config
    let config_path = dirs::config_dir()
        .ok_or("No config directory")?
        .join("darkman/config.yaml");

    if config_path.exists() {
        let backup_path = format!("{}.backup", config_path.display());
        std::fs::copy(&config_path, &backup_path)?;
    }

    config::write_darkman_config(&yaml_config)?;
    log::info!("[darkman] Config updated: {} = {:?}", field, value);

    // Attempt restart (best-effort)
    if let Err(e) = config::restart_darkman_service().await {
        log::warn!("[darkman] Failed to restart service: {}. Config saved, manual restart needed.", e);
    }

    Ok(())
}
```

**Step 8: Update handle_action() to handle update_field**

Modify `handle_action()` in `impl Plugin for DarkmanPlugin`:

```rust
async fn handle_action(
    &self,
    _urn: Urn,
    action: String,
    params: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match action.as_str() {
        "toggle" => {
            log::debug!("Toggle action received");

            let current = self.current_mode();
            let new_mode = match current {
                DarkmanMode::Dark => DarkmanMode::Light,
                DarkmanMode::Light => DarkmanMode::Dark,
            };

            if let Err(e) = self.set_mode(new_mode).await {
                log::error!("Failed to set darkman mode: {e}");
                return Err(e.into());
            }

            // Update shared state
            match self.state.lock() {
                Ok(mut guard) => guard.mode = new_mode,
                Err(e) => {
                    log::warn!("Mutex poisoned, recovering: {e}");
                    e.into_inner().mode = new_mode;
                }
            }
            log::debug!("Mode toggled to: {new_mode:?}");
        }
        "update_field" => {  // NEW
            log::debug!("Update field action received: {:?}", params);

            let field: String = serde_json::from_value(params.get("field")
                .ok_or("Missing 'field' parameter")?
                .clone())?;
            let value = params.get("value")
                .ok_or("Missing 'value' parameter")?
                .clone();

            self.update_config_field(&field, value).await?;
        }
        _ => {
            log::warn!("Unknown action: {}", action);
        }
    }
    Ok(())
}
```

**Step 9: Build and verify**

Run:
```bash
cargo build -p darkman
```

Expected: SUCCESS

**Step 10: Commit**

```bash
git add plugins/darkman/src/lib.rs
git commit -m "feat(darkman): integrate config entity emission and update actions

Update DarkmanPlugin to:
- Load YAML config on startup
- Emit config entity in get_entities()
- Handle update_field action with validation, backup, and service restart
- Store yaml_config in Arc<Mutex> for thread-safe access

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Plugin Manifest - Advertise Config Entity

**Files:**
- Modify: `plugins/darkman/bin/waft-darkman-daemon.rs`

**Step 1: Update handle_provides() call**

Modify main() in `plugins/darkman/bin/waft-darkman-daemon.rs`:

```rust
fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[
        entity::display::DARK_MODE_ENTITY_TYPE,
        entity::display::DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,  // NEW
    ]) {
        return Ok(());
    }

    // ... rest of main unchanged
}
```

**Step 2: Build and verify**

Run:
```bash
cargo build -p darkman
```

Expected: SUCCESS

**Step 3: Test provides output**

Run:
```bash
./target/debug/waft-darkman-daemon provides
```

Expected output contains:
```
dark-mode
dark-mode-automation-config
```

**Step 4: Commit**

```bash
git add plugins/darkman/bin/waft-darkman-daemon.rs
git commit -m "feat(darkman): advertise dark-mode-automation-config entity type

Update plugin manifest to include both entity types.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Settings UI - Dark Mode Automation Section

**Files:**
- Create: `crates/settings/src/display/dark_mode_automation_section.rs`
- Modify: `crates/settings/src/display/mod.rs`

**Step 1: Write skeleton test for section**

Create `crates/settings/src/display/dark_mode_automation_section.rs`:

```rust
//! Dark mode automation settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `dark-mode-automation-config` entity type.
//! Provides configuration UI with schema-driven widget rendering.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::entity::display::{DarkModeAutomationConfig, FieldState, DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE};
use waft_protocol::Urn;

/// Smart container for dark mode automation settings.
pub struct DarkModeAutomationSection {
    pub root: adw::PreferencesGroup,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_creation() {
        gtk::init().unwrap();
        let store = Rc::new(EntityStore::new_disconnected());
        let callback: EntityActionCallback = Rc::new(|_urn, _action, _params| {});
        let section = DarkModeAutomationSection::new(&store, &callback);
        assert_eq!(section.root.title(), "Dark Mode Automation");
    }
}
```

**Step 2: Run test to verify structure**

Run:
```bash
cargo test -p waft-settings dark_mode_automation_section::tests
```

Expected: FAIL with "cannot find function `new` in this scope"

**Step 3: Implement section structure with widget creation**

Add before `#[cfg(test)]` in `crates/settings/src/display/dark_mode_automation_section.rs`:

```rust
impl DarkModeAutomationSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Dark Mode Automation")
            .visible(false)  // Hidden until entity received
            .build();

        // Create widgets
        let latitude_row = adw::SpinRow::builder()
            .title("Latitude")
            .adjustment(&gtk::Adjustment::new(0.0, -90.0, 90.0, 0.01, 1.0, 0.0))
            .digits(2)
            .visible(false)
            .build();
        group.add(&latitude_row);

        let longitude_row = adw::SpinRow::builder()
            .title("Longitude")
            .adjustment(&gtk::Adjustment::new(0.0, -180.0, 180.0, 0.01, 1.0, 0.0))
            .digits(2)
            .visible(false)
            .build();
        group.add(&longitude_row);

        let auto_location_row = adw::SwitchRow::builder()
            .title("Auto-detect location")
            .visible(false)
            .build();
        group.add(&auto_location_row);

        let dbus_api_row = adw::SwitchRow::builder()
            .title("Enable D-Bus API")
            .visible(false)
            .build();
        group.add(&dbus_api_row);

        let portal_api_row = adw::SwitchRow::builder()
            .title("Enable XDG Portal")
            .visible(false)
            .build();
        group.add(&portal_api_row);

        // State management
        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Wire latitude changes
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            latitude_row.connect_changed(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let value = row.value();
                    cb(
                        urn.clone(),
                        "update_field".to_string(),
                        serde_json::json!({
                            "field": "latitude",
                            "value": value
                        }),
                    );
                }
            });
        }

        // Wire longitude changes
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            longitude_row.connect_changed(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let value = row.value();
                    cb(
                        urn.clone(),
                        "update_field".to_string(),
                        serde_json::json!({
                            "field": "longitude",
                            "value": value
                        }),
                    );
                }
            });
        }

        // Wire auto-location toggle
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            auto_location_row.connect_active_notify(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let active = row.is_active();
                    cb(
                        urn.clone(),
                        "update_field".to_string(),
                        serde_json::json!({
                            "field": "auto_location",
                            "value": active
                        }),
                    );
                }
            });
        }

        // Wire D-Bus API toggle
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            dbus_api_row.connect_active_notify(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let active = row.is_active();
                    cb(
                        urn.clone(),
                        "update_field".to_string(),
                        serde_json::json!({
                            "field": "dbus_api",
                            "value": active
                        }),
                    );
                }
            });
        }

        // Wire portal API toggle
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();
            portal_api_row.connect_active_notify(move |row| {
                if guard.get() {
                    return;
                }
                if let Some(ref urn) = *urn_ref.borrow() {
                    let active = row.is_active();
                    cb(
                        urn.clone(),
                        "update_field".to_string(),
                        serde_json::json!({
                            "field": "portal_api",
                            "value": active
                        }),
                    );
                }
            });
        }

        // Subscribe to entity updates
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let lat_ref = latitude_row.clone();
            let lng_ref = longitude_row.clone();
            let auto_loc_ref = auto_location_row.clone();
            let dbus_ref = dbus_api_row.clone();
            let portal_ref = portal_api_row.clone();
            let urn_ref = current_urn.clone();
            let guard = updating.clone();

            entity_store.subscribe_type(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE, move || {
                let configs: Vec<(Urn, DarkModeAutomationConfig)> =
                    store.get_entities_typed(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);

                    // Reconcile latitude
                    if let Some(schema) = config.schema.fields.get("latitude") {
                        if schema.available {
                            lat_ref.set_visible(true);
                            if let Some(lat) = config.latitude {
                                lat_ref.set_value(lat);
                            }
                            lat_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                lat_ref.set_subtitle(help);
                            }
                        } else {
                            lat_ref.set_visible(false);
                        }
                    }

                    // Reconcile longitude
                    if let Some(schema) = config.schema.fields.get("longitude") {
                        if schema.available {
                            lng_ref.set_visible(true);
                            if let Some(lng) = config.longitude {
                                lng_ref.set_value(lng);
                            }
                            lng_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                lng_ref.set_subtitle(help);
                            }
                        } else {
                            lng_ref.set_visible(false);
                        }
                    }

                    // Reconcile auto-location
                    if let Some(schema) = config.schema.fields.get("auto_location") {
                        if schema.available {
                            auto_loc_ref.set_visible(true);
                            if let Some(auto_loc) = config.auto_location {
                                auto_loc_ref.set_active(auto_loc);
                            }
                            auto_loc_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                auto_loc_ref.set_subtitle(help);
                            }
                        } else {
                            auto_loc_ref.set_visible(false);
                        }
                    }

                    // Reconcile D-Bus API
                    if let Some(schema) = config.schema.fields.get("dbus_api") {
                        if schema.available {
                            dbus_ref.set_visible(true);
                            if let Some(dbus) = config.dbus_api {
                                dbus_ref.set_active(dbus);
                            }
                            dbus_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                dbus_ref.set_subtitle(help);
                            }
                        } else {
                            dbus_ref.set_visible(false);
                        }
                    }

                    // Reconcile portal API
                    if let Some(schema) = config.schema.fields.get("portal_api") {
                        if schema.available {
                            portal_ref.set_visible(true);
                            if let Some(portal) = config.portal_api {
                                portal_ref.set_active(portal);
                            }
                            portal_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                portal_ref.set_subtitle(help);
                            }
                        } else {
                            portal_ref.set_visible(false);
                        }
                    }

                    guard.set(false);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        // Initial reconciliation with cached data
        {
            let store_clone = entity_store.clone();
            let group_ref = group.clone();
            let lat_ref = latitude_row.clone();
            let lng_ref = longitude_row.clone();
            let auto_loc_ref = auto_location_row.clone();
            let dbus_ref = dbus_api_row.clone();
            let portal_ref = portal_api_row.clone();
            let urn_ref = current_urn;
            let guard = updating;

            gtk::glib::idle_add_local_once(move || {
                let configs: Vec<(Urn, DarkModeAutomationConfig)> =
                    store_clone.get_entities_typed(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    log::debug!("[dark-mode-automation] Initial reconciliation with cached data");
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);

                    // Same reconciliation logic as subscription
                    // (duplicated for initial load)

                    if let Some(schema) = config.schema.fields.get("latitude") {
                        if schema.available {
                            lat_ref.set_visible(true);
                            if let Some(lat) = config.latitude {
                                lat_ref.set_value(lat);
                            }
                            lat_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                lat_ref.set_subtitle(help);
                            }
                        }
                    }

                    if let Some(schema) = config.schema.fields.get("longitude") {
                        if schema.available {
                            lng_ref.set_visible(true);
                            if let Some(lng) = config.longitude {
                                lng_ref.set_value(lng);
                            }
                            lng_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                lng_ref.set_subtitle(help);
                            }
                        }
                    }

                    if let Some(schema) = config.schema.fields.get("auto_location") {
                        if schema.available {
                            auto_loc_ref.set_visible(true);
                            if let Some(auto_loc) = config.auto_location {
                                auto_loc_ref.set_active(auto_loc);
                            }
                            auto_loc_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                auto_loc_ref.set_subtitle(help);
                            }
                        }
                    }

                    if let Some(schema) = config.schema.fields.get("dbus_api") {
                        if schema.available {
                            dbus_ref.set_visible(true);
                            if let Some(dbus) = config.dbus_api {
                                dbus_ref.set_active(dbus);
                            }
                            dbus_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                dbus_ref.set_subtitle(help);
                            }
                        }
                    }

                    if let Some(schema) = config.schema.fields.get("portal_api") {
                        if schema.available {
                            portal_ref.set_visible(true);
                            if let Some(portal) = config.portal_api {
                                portal_ref.set_active(portal);
                            }
                            portal_ref.set_sensitive(schema.state == FieldState::Editable);
                            if let Some(help) = &schema.help_text {
                                portal_ref.set_subtitle(help);
                            }
                        }
                    }

                    guard.set(false);
                }
            });
        }

        Self { root: group }
    }
}
```

**Step 4: Run test to verify it passes**

Run:
```bash
cargo test -p waft-settings dark_mode_automation_section::tests
```

Expected: PASS

**Step 5: Export module in mod.rs**

Add to `crates/settings/src/display/mod.rs`:

```rust
pub mod dark_mode_automation_section;
pub use dark_mode_automation_section::DarkModeAutomationSection;
```

**Step 6: Build and verify**

Run:
```bash
cargo build -p waft-settings
```

Expected: SUCCESS

**Step 7: Commit**

```bash
git add crates/settings/src/display/dark_mode_automation_section.rs crates/settings/src/display/mod.rs
git commit -m "feat(settings): add dark mode automation section

Create smart container subscribing to dark-mode-automation-config entity.
Schema-driven widget rendering with:
- Latitude/longitude SpinRows (-90 to 90, -180 to 180)
- Auto-location SwitchRow
- D-Bus API and Portal API SwitchRows
- Dynamic visibility/sensitivity based on schema
- Help text as subtitle
- Initial reconciliation pattern

Emits update_field actions on widget changes with updating guard.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Settings Integration - Add Section to Display Page

**Files:**
- Modify: `crates/settings/src/pages/display.rs`

**Step 1: Import new section**

Verify import exists at top of `crates/settings/src/pages/display.rs`:

```rust
use crate::display::DarkModeAutomationSection;
```

**Step 2: Add section to page**

Modify `DisplayPage::new()` in `crates/settings/src/pages/display.rs`:

```rust
pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();

    let brightness = BrightnessSection::new(entity_store, action_callback);
    root.append(&brightness.root);

    let output = OutputSection::new(entity_store, action_callback);
    root.append(&output.root);

    let dark_mode = DarkModeSection::new(entity_store, action_callback);
    root.append(&dark_mode.root);

    let dark_mode_automation = DarkModeAutomationSection::new(entity_store, action_callback);  // NEW
    root.append(&dark_mode_automation.root);  // NEW

    let night_light = NightLightSection::new(entity_store, action_callback);
    root.append(&night_light.root);

    Self { root }
}
```

**Step 3: Build and verify**

Run:
```bash
cargo build -p waft-settings
```

Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/settings/src/pages/display.rs
git commit -m "feat(settings): add dark mode automation to display page

Insert DarkModeAutomationSection after DarkModeSection on Display page.

Page structure:
- Brightness
- Outputs
- Dark Mode (toggle)
- Dark Mode Automation (config) ← NEW
- Night Light

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 7: Build and Test Workspace

**Step 1: Build entire workspace**

Run:
```bash
cargo build --workspace
```

Expected: SUCCESS

**Step 2: Run all tests**

Run:
```bash
cargo test --workspace
```

Expected: All tests PASS

**Step 3: Test plugin provides**

Run:
```bash
./target/debug/waft-darkman-daemon provides
```

Expected output:
```
dark-mode
dark-mode-automation-config
```

**Step 4: Manual smoke test (if darkman is installed)**

1. Start waft daemon: `./target/debug/waft`
2. Start settings app: `./target/debug/waft-settings`
3. Navigate to Display page
4. Verify "Dark Mode Automation" section appears
5. Verify all 5 fields are visible with help text
6. Change latitude value → check `~/.config/darkman/config.yaml` updated
7. Toggle auto-location → check YAML updated
8. Close and reopen settings → verify values persist

**Step 5: Commit test notes**

```bash
git add .
git commit -m "test: verify darkman configuration end-to-end

Workspace builds successfully, all tests pass.
Manual testing confirms:
- Plugin advertises both entity types
- Settings UI renders all fields with schema
- Actions update YAML config file
- Service restart attempted after changes

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Task 8: Update Plugin Documentation

**Files:**
- Modify: `plugins/darkman/README.md`

**Step 1: Update README with new entity type and actions**

Add to `plugins/darkman/README.md` after existing sections:

```markdown
## Configuration UI

The darkman plugin provides a configuration entity for managing `~/.config/darkman/config.yaml` via waft-settings.

### Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `dark-mode` | `darkman/dark-mode/default` | Whether dark mode is active (existing) |
| `dark-mode-automation-config` | `darkman/dark-mode-automation-config/default` | Configuration settings for darkman |

### Configuration Actions

| Action | Parameters | Description |
|---|---|---|
| `toggle` | none | Switches between dark and light mode |
| `update_field` | `field` (string), `value` (json) | Updates a configuration field |

**Supported configuration fields:**

- `latitude` (float, -90 to 90) - Manual latitude for sunrise/sunset calculation
- `longitude` (float, -180 to 180) - Manual longitude for sunrise/sunset calculation
- `auto_location` (bool) - Auto-detect location via geoclue
- `dbus_api` (bool) - Enable D-Bus API (required for waft)
- `portal_api` (bool) - Enable XDG portal support

**Configuration storage:** `~/.config/darkman/config.yaml`

**Service restart:** After config changes, the plugin attempts to restart darkman service via `systemctl --user restart darkman.service` (best-effort).

### Configuration Example

```toml
# In waft-settings Display page:
# - Latitude: 50.08
# - Longitude: 14.42
# - Auto-detect location: ON
# - Enable D-Bus API: ON
# - Enable XDG Portal: ON

# Results in ~/.config/darkman/config.yaml:
lat: 50.08
lng: 14.42
usegeoclue: true
dbusserver: true
portal: true
```
```

**Step 2: Verify rendering**

Run:
```bash
cat plugins/darkman/README.md
```

Expected: New sections visible with proper markdown formatting

**Step 3: Commit**

```bash
git add plugins/darkman/README.md
git commit -m "docs(darkman): document configuration entity and actions

Update README with:
- New dark-mode-automation-config entity type
- update_field action documentation
- Configuration field specifications
- Configuration storage location
- Service restart behavior
- Example configuration

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

## Completion

Plan complete! All tasks implemented with TDD approach, comprehensive tests, and documentation.

**Summary:**
- ✅ Protocol layer with entity types and schema
- ✅ Plugin config module for YAML parsing
- ✅ Plugin integration for entity emission and actions
- ✅ Plugin manifest advertising both entities
- ✅ Settings UI section with schema-driven rendering
- ✅ Display page integration
- ✅ Workspace build verification
- ✅ Plugin documentation

**Next steps:**
- Run manual testing checklist from design doc
- Test with actual darkman service installed
- Verify service restart behavior
- Test error scenarios (malformed YAML, permission errors, etc.)
