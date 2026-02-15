# Darkman Configuration Design

**Date:** 2026-02-15
**Status:** Approved
**Target:** waft-settings Display page, darkman plugin

## Overview

Add comprehensive configuration UI for darkman's automatic dark mode switching settings in waft-settings. The design uses a generalized entity structure that can support other dark mode switching tools (Yin-Yang, Blueblack, etc.) in the future.

## Requirements

1. **All darkman config fields** - Expose all 5 fields from `~/.config/darkman/config.yaml`
2. **Generalized entity** - Use capability-based entity design that works across tools
3. **Rich schema** - Entity carries metadata describing field availability, types, and constraints
4. **Display page integration** - Add as new section on existing Display settings page
5. **Direct file management** - Plugin reads/writes darkman's YAML config directly
6. **Service restart** - Detect and restart darkman service after config changes

---

## Architecture

### Entity Structure

**New entity type in `crates/protocol/src/entity/display.rs`:**

```rust
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

**URN format:** `darkman/dark-mode-automation-config/default`

### Field Mappings (Darkman)

Generalized entity fields map to darkman's YAML config as follows:

| Entity Field | Darkman YAML | Type | Description |
|--------------|--------------|------|-------------|
| `latitude` | `lat` | f64 | Manual latitude (-90 to 90) |
| `longitude` | `lng` | f64 | Manual longitude (-180 to 180) |
| `auto_location` | `usegeoclue` | bool | Auto-detect location via geoclue |
| `dbus_api` | `dbusserver` | bool | Enable D-Bus API (required for waft) |
| `portal_api` | `portal` | bool | Enable XDG portal support |

### Darkman Plugin Responsibilities

The darkman plugin (`plugins/darkman/`) will:

1. **Parse config** - Read `~/.config/darkman/config.yaml` on startup
2. **Build schema** - Create schema metadata for supported fields (all 5 for darkman)
3. **Emit entities** - Send both `DarkMode` (toggle) and `DarkModeAutomationConfig` (settings)
4. **Handle actions** - Process `update_field(name, value)` actions
5. **Write config** - Modify YAML, backup, write to `~/.config/darkman/config.yaml`
6. **Restart service** - Execute `systemctl --user restart darkman.service` (best-effort)
7. **Re-emit entity** - Send updated entity after successful config change

### Service Restart Flow

After writing config, plugin attempts to restart darkman:
1. Try `systemctl --user restart darkman.service`
2. If command succeeds → log success
3. If command fails → log warning with manual instructions
4. Action still succeeds (config was saved)

**Rationale:** Config persistence is critical; service restart is best-effort. Users can manually restart if systemctl unavailable.

---

## Components

### Protocol Layer (`crates/protocol/`)

**File: `src/entity/display.rs`**
- Add `DarkModeAutomationConfig`, `ConfigSchema`, `FieldSchema`, `FieldState`, `FieldType`, `Constraints`
- Export `DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE` constant
- Add to `mod.rs` re-exports

### Darkman Plugin (`plugins/darkman/`)

**Updated: `bin/waft-darkman-daemon.rs`**
```rust
fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides(&[
        entity::display::DARK_MODE_ENTITY_TYPE,
        entity::display::DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,  // NEW
    ]) {
        return Ok(());
    }
    // ... rest of main
}
```

**New module: `src/config.rs`**
- `DarkmanYamlConfig` struct matching YAML structure (`lat`, `lng`, `usegeoclue`, `dbusserver`, `portal`)
- `parse_darkman_config() -> Result<DarkmanYamlConfig>` - Parse `~/.config/darkman/config.yaml`
- `write_darkman_config(config: &DarkmanYamlConfig) -> Result<()>` - Write YAML with backup
- `build_schema() -> ConfigSchema` - Create schema metadata for darkman's 5 fields
- `to_entity(yaml_config: &DarkmanYamlConfig) -> Entity` - Convert YAML config to entity
- `restart_darkman_service() -> Result<()>` - Systemd restart with fallback
- `validate_field(field: &str, value: &serde_json::Value) -> Result<()>` - Validate constraints

**Schema Example (darkman):**
```rust
fn build_schema() -> ConfigSchema {
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
```

**Updated: `src/lib.rs`**
```rust
struct DarkmanPlugin {
    config: DarkmanConfig,
    state: Arc<StdMutex<DarkmanState>>,
    conn: Connection,
    yaml_config: Arc<StdMutex<DarkmanYamlConfig>>,  // NEW - for config management
}

impl Plugin for DarkmanPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        vec![
            self.dark_mode_entity(),      // Existing toggle entity
            self.config_entity(),          // NEW - config entity
        ]
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "toggle" => { /* existing dark mode toggle */ }
            "update_field" => {           // NEW
                let field: String = serde_json::from_value(params["field"].clone())?;
                let value: serde_json::Value = params["value"].clone();
                self.update_config_field(&field, value).await?;
            }
            _ => {}
        }
        Ok(())
    }
}
```

### Settings UI (`crates/settings/`)

**New section: `src/display/dark_mode_automation_section.rs`**

Smart container that:
- Subscribes to `DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE`
- Renders `adw::PreferencesGroup` with title "Dark Mode Automation"
- Creates widgets based on schema (flat organization):
  - Latitude: `adw::SpinRow` (-90 to 90, 2 decimals)
  - Longitude: `adw::SpinRow` (-180 to 180, 2 decimals)
  - Auto-detect location: `adw::SwitchRow`
  - Enable D-Bus API: `adw::SwitchRow`
  - Enable XDG Portal: `adw::SwitchRow`
- Emits `update_field` actions on widget changes
- Uses updating guard to prevent loops
- Sets widget visibility/sensitivity based on `FieldSchema.available` and `FieldSchema.state`
- Shows `help_text` as subtitle on each widget

**Widget Reconciliation Pattern:**
```rust
fn reconcile(config: &DarkModeAutomationConfig, widgets: &Widgets, updating: &Rc<Cell<bool>>) {
    updating.set(true);

    // Reconcile each field based on schema
    if let Some(schema) = config.schema.fields.get("latitude") {
        if schema.available {
            widgets.latitude_row.set_visible(true);
            if let Some(lat) = config.latitude {
                widgets.latitude_row.set_value(lat);
            }
            widgets.latitude_row.set_sensitive(schema.state == FieldState::Editable);
            if let Some(help) = &schema.help_text {
                widgets.latitude_row.set_subtitle(help);
            }
        } else {
            widgets.latitude_row.set_visible(false);
        }
    }

    // ... repeat for longitude, auto_location, dbus_api, portal_api

    updating.set(false);
}
```

**Updated: `src/display/mod.rs`**
- Add `mod dark_mode_automation_section;`
- Export `DarkModeAutomationSection`

**Updated: `src/pages/display.rs`**
```rust
let dark_mode_automation = DarkModeAutomationSection::new(entity_store, action_callback);
root.append(&dark_mode_automation.root);
```

**Page Structure:**
```
Display Settings
├── Brightness
├── Outputs
├── Dark Mode (existing toggle)
├── Dark Mode Automation (NEW)
└── Night Light
```

### Dependencies

**Add to `plugins/darkman/Cargo.toml`:**
```toml
serde_yaml = "0.9"  # For YAML parsing
```

---

## Data Flow

### Initial Load (Plugin Startup)

```
1. Plugin starts
   ↓
2. Parse ~/.config/darkman/config.yaml
   ↓
3. Build schema metadata (all 5 fields available for darkman)
   ↓
4. Create DarkModeAutomationConfig entity
   - Data: lat, lng, usegeoclue, dbusserver, portal from YAML
   - Schema: field metadata with constraints
   ↓
5. Emit entity to daemon
   ↓
6. Daemon routes to subscribed apps (waft-settings)
   ↓
7. Settings UI receives entity
   ↓
8. UI reconciles widgets based on schema:
   - Renders SpinRow for lat/lng (if available)
   - Renders SwitchRow for bools (if available)
   - Sets sensitive() based on field_state
   - Shows help_text as subtitle
```

### User Edits Field (Example: Change Latitude)

```
1. User changes latitude SpinRow from 50.08 to 52.52
   ↓
2. Widget emits value-changed signal
   ↓
3. Section callback checks updating guard (not set)
   ↓
4. Section emits action:
   TriggerAction(
     urn: "darkman/dark-mode-automation-config/default",
     action: "update_field",
     params: {"field": "latitude", "value": 52.52}
   )
   ↓
5. Daemon routes action to darkman plugin
   ↓
6. Plugin handles action:
   a. Lock yaml_config state
   b. Validate value (check constraints: -90 to 90)
   c. Update: yaml_config.lat = Some(52.52)
   d. Backup: cp config.yaml config.yaml.backup
   e. Write: serialize yaml_config to config.yaml
   f. Restart: systemctl --user restart darkman.service
      - If fails: log warning, continue (config saved)
   g. Re-read: parse config.yaml to verify
   h. Build new entity with updated values
   i. Notify daemon
   ↓
7. Daemon sends EntityUpdated to settings app
   ↓
8. Settings UI receives entity update
   ↓
9. UI reconciles:
   - Set updating guard = true
   - Update latitude SpinRow value to 52.52
   - Set updating guard = false
```

### Settings UI Subscription Pattern

```rust
// Subscribe to config entity
entity_store.subscribe_type(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE, move || {
    let configs: Vec<(Urn, DarkModeAutomationConfig)> =
        store.get_entities_typed(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE);

    if let Some((urn, config)) = configs.first() {
        Self::reconcile(&state, &config, &urn_ref, &widgets);
    } else {
        // No entity - hide section
        group_ref.set_visible(false);
    }
});

// Initial reconciliation with cached data
gtk::glib::idle_add_local_once(move || {
    let configs = store.get_entities_typed(DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE);
    if let Some((urn, config)) = configs.first() {
        Self::reconcile(&state, &config, &urn_ref, &widgets);
    }
});
```

---

## Error Handling

### Config Parsing Errors

**Error:** `~/.config/darkman/config.yaml` doesn't exist, is malformed, or has invalid values

**Handling:**
```rust
fn parse_darkman_config() -> Result<DarkmanYamlConfig> {
    let config_path = dirs::config_dir()
        .ok_or("No config directory")?
        .join("darkman/config.yaml");

    if !config_path.exists() {
        log::warn!("[darkman] Config file not found, using defaults");
        return Ok(DarkmanYamlConfig::default());
    }

    let yaml_str = std::fs::read_to_string(&config_path)
        .context("Failed to read config file")?;

    match serde_yaml::from_str(&yaml_str) {
        Ok(config) => Ok(config),
        Err(e) => {
            log::error!("[darkman] Failed to parse config.yaml: {}", e);
            Err(anyhow::anyhow!("Config file has syntax errors: {}", e))
        }
    }
}
```

**User experience:**
- If file missing: Plugin emits entity with default/empty values, all fields editable
- If parse fails: Plugin logs error, doesn't emit config entity, settings section stays hidden
- Settings page gracefully handles missing entity (section not visible)

### Config Write Errors

**Errors:**
- Permission denied
- Disk full
- Invalid YAML serialization

**Handling:**
```rust
async fn update_config_field(&self, field: &str, value: serde_json::Value) -> Result<()> {
    let mut yaml_config = self.lock_yaml_config()?;

    // Validate and update field
    self.validate_field(field, &value)?;
    self.apply_field_update(&mut yaml_config, field, value)?;

    // Backup before writing
    let config_path = get_config_path()?;
    let backup_path = format!("{}.backup", config_path.display());
    if config_path.exists() {
        std::fs::copy(&config_path, &backup_path)
            .context("Failed to create backup")?;
    }

    // Write config
    match write_darkman_config(&yaml_config) {
        Ok(()) => {
            log::info!("[darkman] Config updated: {} = {:?}", field, value);
        }
        Err(e) => {
            log::error!("[darkman] Failed to write config: {}", e);
            // Restore from backup
            if let Err(restore_err) = std::fs::copy(&backup_path, &config_path) {
                log::error!("[darkman] Failed to restore backup: {}", restore_err);
            }
            return Err(anyhow::anyhow!("Failed to save configuration: {}", e));
        }
    }

    // Attempt restart (best-effort)
    if let Err(e) = restart_darkman_service().await {
        log::warn!("[darkman] Failed to restart service: {}. Config saved, manual restart needed.", e);
    }

    Ok(())
}
```

**User experience:**
- Settings UI receives `ActionError` via daemon
- Shows toast: "Failed to save configuration: {error}"
- Widget values revert to previous state (no optimistic update)

### Service Restart Errors

**Error:** `systemctl --user restart darkman.service` fails

**Handling:**
```rust
async fn restart_darkman_service() -> Result<()> {
    let output = tokio::process::Command::new("systemctl")
        .args(["--user", "restart", "darkman.service"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            log::info!("[darkman] Service restarted successfully");
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

**User experience:**
- Warning logged, but action succeeds (config was saved)
- Optional: Show info toast: "Configuration saved. Restart darkman service to apply changes."
- Plugin doesn't fail the action - config persistence is what matters

### Field Validation Errors

**Errors:**
- Latitude out of range (-90 to 90)
- Longitude out of range (-180 to 180)
- Invalid type (string for bool field)

**Handling:**
```rust
fn validate_field(&self, field: &str, value: &serde_json::Value) -> Result<()> {
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
```

**User experience:**
- SpinRow widgets enforce min/max constraints (prevents invalid input)
- If somehow bypassed: ActionError returned, toast shows validation message
- Widget constraints from schema prevent most validation errors

### Logging Strategy

**Error levels:**
- `error!()` - Parse failures, write failures requiring rollback
- `warn!()` - Service restart failures (non-critical), backup issues
- `info!()` - Successful config updates, service restarts
- `debug!()` - Field validations, action handling

---

## Testing

### Unit Tests (Plugin Layer)

**Test module: `plugins/darkman/src/config.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_config() {
        let yaml = r#"
lat: 50.08
lng: 14.42
usegeoclue: true
dbusserver: true
portal: true
"#;
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.lat, Some(50.08));
        assert_eq!(config.usegeoclue, Some(true));
    }

    #[test]
    fn parse_partial_config() {
        let yaml = r#"
lat: 52.52
lng: 13.40
"#;
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.lat, Some(52.52));
        assert_eq!(config.usegeoclue, None);  // Missing field
    }

    #[test]
    fn parse_empty_config() {
        let yaml = "";
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config, DarkmanYamlConfig::default());
    }

    #[test]
    fn build_schema_contains_all_fields() {
        let schema = build_schema();
        assert!(schema.fields.contains_key("latitude"));
        assert!(schema.fields.contains_key("longitude"));
        assert!(schema.fields.contains_key("auto_location"));
        assert!(schema.fields.contains_key("dbus_api"));
        assert!(schema.fields.contains_key("portal_api"));
    }

    #[test]
    fn latitude_field_schema_has_constraints() {
        let schema = build_schema();
        let lat_schema = &schema.fields["latitude"];
        assert_eq!(lat_schema.field_type, FieldType::Float { decimals: 2 });
        assert_eq!(lat_schema.constraints.as_ref().unwrap().min, Some(-90.0));
        assert_eq!(lat_schema.constraints.as_ref().unwrap().max, Some(90.0));
    }

    #[test]
    fn validate_latitude_in_range() {
        let value = serde_json::json!(52.52);
        assert!(validate_field("latitude", &value).is_ok());
    }

    #[test]
    fn validate_latitude_out_of_range() {
        let value = serde_json::json!(100.0);
        assert!(validate_field("latitude", &value).is_err());
    }

    #[test]
    fn validate_bool_fields() {
        let value = serde_json::json!(true);
        assert!(validate_field("auto_location", &value).is_ok());
        assert!(validate_field("dbus_api", &value).is_ok());
        assert!(validate_field("portal_api", &value).is_ok());
    }
}
```

### Integration Tests (Entity Flow)

**Test: Full action flow with mocked config file**

```rust
#[tokio::test]
async fn test_update_latitude_action() {
    // Create temp config file
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    // Write initial config
    let initial_yaml = "lat: 50.08\nlng: 14.42\n";
    std::fs::write(&config_path, initial_yaml).unwrap();

    // Create plugin (inject temp config path)
    let plugin = DarkmanPlugin::new_with_config_path(config_path.clone()).await.unwrap();

    // Trigger action
    let params = serde_json::json!({
        "field": "latitude",
        "value": 52.52
    });
    plugin.handle_action(urn, "update_field".into(), params).await.unwrap();

    // Verify config file updated
    let yaml_str = std::fs::read_to_string(&config_path).unwrap();
    let config: DarkmanYamlConfig = serde_yaml::from_str(&yaml_str).unwrap();
    assert_eq!(config.lat, Some(52.52));

    // Verify entity updated
    let entities = plugin.get_entities();
    let config_entity = entities.iter()
        .find(|e| e.entity_type == DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE)
        .unwrap();
    let data: DarkModeAutomationConfig = serde_json::from_value(config_entity.data.clone()).unwrap();
    assert_eq!(data.latitude, Some(52.52));
}
```

### Settings UI Tests

**Test: Widget reconciliation based on schema**

```rust
#[gtk::test]
fn test_reconciliation_shows_available_fields() {
    let entity = DarkModeAutomationConfig {
        latitude: Some(50.08),
        longitude: Some(14.42),
        auto_location: Some(true),
        dbus_api: Some(true),
        portal_api: Some(true),
        schema: build_test_schema(),
    };

    let widgets = create_test_widgets();
    reconcile(&entity, &widgets);

    // Verify widgets visible and populated
    assert!(widgets.latitude_row.is_visible());
    assert_eq!(widgets.latitude_row.value(), 50.08);
    assert!(widgets.auto_location_row.is_active());
}

#[gtk::test]
fn test_reconciliation_hides_unavailable_fields() {
    let mut schema = build_test_schema();
    schema.fields.get_mut("latitude").unwrap().available = false;

    let entity = DarkModeAutomationConfig {
        latitude: None,
        schema,
        ..Default::default()
    };

    let widgets = create_test_widgets();
    reconcile(&entity, &widgets);

    // Latitude should be hidden
    assert!(!widgets.latitude_row.is_visible());
}
```

### Manual Testing Checklist

**Happy path:**
- [ ] Open waft-settings → Display page shows "Dark Mode Automation" section
- [ ] Section shows all 5 fields with current values from darkman config
- [ ] Change latitude → Config file updated, darkman service restarted
- [ ] Change longitude → Config file updated, darkman service restarted
- [ ] Toggle auto-location → Config file updated
- [ ] Toggle D-Bus API → Config file updated
- [ ] Toggle portal → Config file updated
- [ ] All changes persist across settings app restart

**Edge cases:**
- [ ] No darkman config file → Section shows with default/empty values, all editable
- [ ] Malformed config YAML → Section hidden (parse error)
- [ ] Config file read-only → Error toast on save attempt
- [ ] Systemd not available → Warning logged, config still saved
- [ ] Invalid latitude (> 90) → Prevented by SpinRow constraints
- [ ] Invalid longitude (> 180) → Prevented by SpinRow constraints

**Schema-driven rendering:**
- [ ] All available fields visible (all 5 for darkman)
- [ ] Help text shown as subtitle on relevant widgets
- [ ] Constraints enforced (lat: -90 to 90, lng: -180 to 180)
- [ ] Editable fields respond to changes
- [ ] Read-only fields (if any) are insensitive

**Multi-tool support (future):**
- [ ] Design supports adding Yin-Yang plugin with different schema
- [ ] Entity type name is tool-agnostic
- [ ] Field names are generalized (not darkman-specific)

---

## Comparison with Similar Applications

Our generalized entity structure aligns well with existing Linux dark mode switching tools:

### Tool Comparison

| Tool | Location Support | Auto-Location | D-Bus API | Portal Support | Config Format |
|------|-----------------|---------------|-----------|----------------|---------------|
| **darkman** | lat/lng | geoclue | Yes | Yes | YAML |
| **Yin-Yang** | lat/lng | Manual detection | No | No | Config file |
| **Blueblack** | lat/lng | Manual only (no geoclue) | No | No | Config file |
| **Night Theme Switcher** (GNOME) | Via GNOME location | GNOME geoclue | No | Implicit (GNOME) | dconf |
| **AutomaThemely** | Via internet API | Internet-based | No | No | Config file |

### Field Mapping Across Tools

Our generalized fields map to different tools as follows:

**`latitude` / `longitude`:**
- darkman: `lat`, `lng` (YAML)
- Yin-Yang: Location coordinates (config)
- Blueblack: Manual lat/lng (config)
- Night Theme Switcher: Uses GNOME location services
- AutomaThemely: Fetched from internet, not user-configured

**`auto_location`:**
- darkman: `usegeoclue` (bool)
- Yin-Yang: Manual trigger to detect
- Blueblack: Not supported (manual only)
- Night Theme Switcher: Always auto via GNOME
- AutomaThemely: Always auto via internet

**`dbus_api`:**
- darkman: `dbusserver` (bool) - enables `nl.whynothugo.darkman` D-Bus service
- Yin-Yang: No D-Bus API
- Blueblack: No D-Bus API
- Night Theme Switcher: No separate API (uses GNOME's)
- AutomaThemely: No D-Bus API

**`portal_api`:**
- darkman: `portal` (bool) - implements XDG settings portal dark mode standard
- Yin-Yang: No portal support
- Blueblack: No portal support
- Night Theme Switcher: Implicit via GNOME
- AutomaThemely: No portal support

### Why Our Design is Generalizable

1. **Optional fields** - Tools that don't support a feature (e.g., Blueblack's no geoclue) can set `available: false` in schema
2. **Schema-driven UI** - Settings UI adapts to tool capabilities automatically
3. **Common concepts** - All tools deal with location-based timing and dark mode switching
4. **Tool-agnostic naming** - Field names (`auto_location`, not `usegeoclue`) work across implementations

### Darkman's Unique Features

Darkman is one of the most feature-complete dark mode switchers:
- **Standards-compliant** - Implements XDG portal spec
- **Integration-friendly** - D-Bus API for programmatic control
- **Flexible location** - Both manual and automatic geoclue
- **Script support** - Custom hooks for app-specific switching

Our entity design captures all of darkman's configuration while remaining extensible for simpler tools.

---

## Implementation Notes

1. **Entity type constant**: Add `DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE = "dark-mode-automation-config"` to `waft-protocol`
2. **Plugin manifest**: Update `handle_provides()` to include both `dark-mode` and `dark-mode-automation-config`
3. **Settings module**: Create `crates/settings/src/display/dark_mode_automation_section.rs`
4. **Schema builder**: Centralize in plugin helper function `build_schema() -> ConfigSchema`
5. **Config path**: Use `dirs::config_dir()` to locate `~/.config/darkman/config.yaml`
6. **Backup strategy**: Always create `.backup` file before writing config
7. **Service restart**: Log warnings on failure, don't fail the action (config saved is success)

## Future Enhancements

- **Multiple tool support** - Add Yin-Yang, Blueblack plugins emitting same entity type with different schemas
- **Geoclue helper** - Button to auto-detect and populate lat/lng from geoclue
- **Manual mode toggle** - Disable automatic switching, manual dark/light only
- **Schedule preview** - Show calculated sunrise/sunset times based on current location
- **Import from system** - Detect and import settings from GNOME, KDE if available
