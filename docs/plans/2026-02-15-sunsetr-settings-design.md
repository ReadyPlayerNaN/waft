# Sunsetr Settings UI Design

**Date:** 2026-02-15
**Status:** Approved
**Target:** waft-settings Display → Night Light section

## Overview

Add comprehensive configuration UI for sunsetr night light settings in waft-settings. The UI will expose all ~18 sunsetr configuration fields, organized into logical groups, with full preset management support.

## Requirements

1. **Grouped by category** - Settings organized into Colors, Timing, Location, and Advanced sections
2. **Separate config entity** - New `NightLightConfig` entity type separate from runtime `NightLight` entity
3. **Full preset management** - Support creating, editing, and deleting presets
4. **Mode-aware UI** - Show all fields but disable irrelevant ones based on `transition_mode`

## Architecture

### Entity Structure

Create `NightLightConfig` entity in `waft-protocol/src/entity/display.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NightLightConfig {
    pub target: String,              // "default" or preset name

    // Backend & Mode
    pub backend: String,             // "auto", "hyprland", "wayland"
    pub transition_mode: String,     // "geo", "static", "center", "finish_by", "start_at"

    // Colors (as strings to match sunsetr JSON)
    pub night_temp: String,          // "1000"-"10000"
    pub night_gamma: String,         // "10"-"200"
    pub day_temp: String,            // "1000"-"10000"
    pub day_gamma: String,           // "10"-"200"
    pub static_temp: String,         // "1000"-"10000"
    pub static_gamma: String,        // "10"-"200"

    // Timing
    pub sunset: String,              // "HH:MM:SS"
    pub sunrise: String,             // "HH:MM:SS"
    pub transition_duration: String, // minutes as string

    // Location
    pub latitude: String,            // decimal string
    pub longitude: String,           // decimal string

    // Advanced
    pub smoothing: String,           // "true"/"false"
    pub startup_duration: String,    // float seconds as string
    pub shutdown_duration: String,   // float seconds as string
    pub adaptive_interval: String,   // milliseconds as string
    pub update_interval: String,     // seconds as string

    // Field availability metadata
    pub field_state: HashMap<String, FieldState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldState {
    /// Field is editable and relevant to current mode
    Editable,
    /// Field is read-only (e.g., sunrise/sunset in geo mode)
    ReadOnly,
    /// Field exists but is not relevant to current mode (grayed out)
    Disabled,
}
```

**URN format:** `sunsetr/night-light-config/{target}` where target is "default" or preset name.

**Field state rules** based on `transition_mode`:

- **geo mode**:
  - Editable: `latitude`, `longitude`, `day_temp`, `day_gamma`, `night_temp`, `night_gamma`, `transition_duration`
  - ReadOnly: `sunrise`, `sunset` (calculated from location)
  - Disabled: `static_temp`, `static_gamma`

- **static mode**:
  - Editable: `static_temp`, `static_gamma`
  - Disabled: `day_temp`, `day_gamma`, `night_temp`, `night_gamma`, `sunrise`, `sunset`, `latitude`, `longitude`, `transition_duration`

- **center/finish_by/start_at modes**:
  - Editable: `sunrise`, `sunset`, `day_temp`, `day_gamma`, `night_temp`, `night_gamma`, `transition_duration`
  - Disabled: `latitude`, `longitude`, `static_temp`, `static_gamma`

- **Always editable**: `backend`, `transition_mode`, `smoothing`, `startup_duration`, `shutdown_duration`, `adaptive_interval`, `update_interval`

### Plugin Responsibilities

The sunsetr plugin (`plugins/sunsetr/`) will:

1. **Query config** via `sunsetr get --target {name} --json all` on startup and when target changes
2. **Provide entities** for both `NightLight` (runtime state) and `NightLightConfig` (configuration)
3. **Compute field states** based on `transition_mode` before emitting entity
4. **Handle actions**:
   - `update_config(field, value)` → calls `sunsetr set --target {target} {field}={value}`, re-queries, emits update
   - `create_preset(name)` → calls `sunsetr preset {name}`, queries new preset, emits entity
   - `delete_preset(name)` → removes `~/.config/sunsetr/presets/{name}.toml`, switches to "default"
   - `load_preset(name)` → queries that preset's config, emits entity with new target
5. **Emit entity updates** after successful config changes

### Settings UI Structure

Extend `NightLightSection` in `crates/settings/src/display/night_light_section.rs` to include configuration UI.

### Communication Flow

1. **Load:** Settings subscribes to `night-light-config` entity type, receives config for current target
2. **Edit:** User changes field → action sent to plugin → plugin calls `sunsetr set` → plugin emits updated entity
3. **Switch preset:** User selects preset → `load_preset` action → plugin queries preset config → emits entity
4. **Create preset:** User clicks "+" → dialog → `create_preset` action → plugin creates → emits entity
5. **Delete preset:** User clicks trash → confirmation → `delete_preset` action → plugin deletes → emits "default"

## Components

### UI Widget Hierarchy

```
NightLightSection (PreferencesGroup)
├── Preset Management Row (ActionRow with HBox)
│   ├── Preset ComboRow (shows available presets)
│   ├── Create Button ("+")
│   └── Delete Button (trash icon)
│
├── Colors Group (PreferencesGroup)
│   ├── Night Temperature (SpinRow, 1000-10000)
│   ├── Night Gamma (SpinRow, 10-200)
│   ├── Day Temperature (SpinRow, 1000-10000)
│   ├── Day Gamma (SpinRow, 10-200)
│   ├── Static Temperature (SpinRow, 1000-10000)
│   └── Static Gamma (SpinRow, 10-200)
│
├── Timing Group (PreferencesGroup)
│   ├── Transition Mode (ComboRow: geo, static, center, finish_by, start_at)
│   ├── Sunrise Time (EntryRow, "HH:MM:SS")
│   ├── Sunset Time (EntryRow, "HH:MM:SS")
│   └── Transition Duration (SpinRow, minutes)
│
├── Location Group (PreferencesGroup)
│   ├── Latitude (EntryRow, -90 to 90)
│   └── Longitude (EntryRow, -180 to 180)
│
└── Advanced Group (PreferencesGroup, collapsed by default)
    ├── Backend (ComboRow: auto, hyprland, wayland)
    ├── Smoothing (SwitchRow)
    ├── Startup Duration (SpinRow, seconds, 0.1 step)
    ├── Shutdown Duration (SpinRow, seconds, 0.1 step)
    ├── Adaptive Interval (SpinRow, milliseconds)
    └── Update Interval (SpinRow, seconds)
```

### Widget Rendering Pattern

Each config field renders based on `field_state`:

- `FieldState::Editable` → `sensitive(true)`, normal appearance
- `FieldState::ReadOnly` → `sensitive(false)`, `subtitle("Calculated automatically")`
- `FieldState::Disabled` → `sensitive(false)`, `subtitle("Not used in this mode")`

Widget types by field:
- **SpinRow**: Numeric values (temps, gamma, durations, intervals)
- **EntryRow**: Time strings (HH:MM:SS) and coordinates (lat/lon)
- **ComboRow**: Enums (backend, transition_mode)
- **SwitchRow**: Booleans (smoothing)

### Preset Management Flows

**Create preset:**
1. User clicks "+" button
2. Dialog prompts for preset name (EntryDialog)
3. Validate name: no spaces, not "default", unique
4. Send `create_preset(name)` action
5. Plugin creates via `sunsetr preset {name}` (copies current config)
6. Plugin queries new preset, emits `NightLightConfig` entity
7. UI updates combo row to show new preset

**Delete preset:**
1. User clicks trash button (disabled when preset == "default")
2. Confirmation dialog: "Delete preset '{name}'?"
3. Send `delete_preset(name)` action
4. Plugin removes `~/.config/sunsetr/presets/{name}.toml`
5. Plugin switches to "default", queries config, emits entity
6. UI removes preset from combo, switches to "default"

**Switch preset:**
1. User selects preset from combo row
2. Send `load_preset(name)` action
3. Plugin queries via `sunsetr get --target {name} --json all`
4. Plugin emits `NightLightConfig` with new target and values
5. UI reconciles all widgets with new config

## Data Flow

### Subscription Pattern

```rust
// Settings subscribes to night-light-config entity type
entity_store.subscribe_type(NIGHT_LIGHT_CONFIG_ENTITY_TYPE, move || {
    let configs: Vec<(Urn, NightLightConfig)> =
        store.get_entities_typed(NIGHT_LIGHT_CONFIG_ENTITY_TYPE);

    if let Some((urn, config)) = configs.first() {
        reconcile_ui(&config);
    }
});

// Initial reconciliation via idle_add_local_once
gtk::glib::idle_add_local_once(move || {
    let configs = store.get_entities_typed(NIGHT_LIGHT_CONFIG_ENTITY_TYPE);
    if !configs.is_empty() {
        reconcile_ui(&configs[0].1);
    }
});
```

### Plugin Config Query

```rust
async fn query_config(target: &str) -> Result<NightLightConfig> {
    // Query all fields from sunsetr
    let (code, stdout, _) = run_sunsetr(&["get", "--target", target, "--json", "all"]).await?;

    let values: HashMap<String, String> = serde_json::from_str(&stdout)?;

    // Compute field states based on transition_mode
    let transition_mode = values.get("transition_mode").unwrap();
    let field_state = compute_field_states(transition_mode);

    Ok(NightLightConfig {
        target: target.to_string(),
        backend: values.get("backend").cloned().unwrap_or_default(),
        transition_mode: transition_mode.clone(),
        // ... all other fields ...
        field_state,
    })
}
```

### User Edit Flow

```
User changes field
    ↓
Widget emits value-changed signal (if not updating flag)
    ↓
Settings sends: update_config(field, value)
    ↓
Plugin receives action
    ↓
Plugin: sunsetr set --target {target} {field}={value}
    ↓
Plugin re-queries config (captures side effects)
    ↓
Plugin: EntityUpdated with new NightLightConfig
    ↓
UI receives entity update
    ↓
UI reconciles widgets, applies new field_state
```

## Error Handling

### Plugin-Side Errors

**Config query failures:**
- `sunsetr get` fails → log error, don't emit entity (UI keeps last state)
- sunsetr binary not found → log warning, plugin exits gracefully
- JSON parse fails → log error with raw output, retry once after 1s

**Config update failures:**
- `sunsetr set` fails → return `ActionError` with stderr message
- Settings shows error toast: "Failed to update {field}: {error}"
- UI doesn't update until successful `EntityUpdated` arrives

**Preset management failures:**
- Duplicate preset name → `ActionError("Preset already exists")`
- Delete "default" → `ActionError("Cannot delete default preset")`
- Filesystem error → `ActionError` with OS error message

### UI Error Handling

**Invalid user input:**
- SpinRow: Min/max constraints enforce ranges
- EntryRow: Validate on focus-out
  - Times: regex `^\d{2}:\d{2}:\d{2}$`
  - Latitude: -90 to 90
  - Longitude: -180 to 180
- Invalid input → error styling, disable action until fixed

**Entity subscription failures:**
- Daemon connection lost → EntityStore handles reconnection
- No entity after 5s → show "Night light settings unavailable"
- Other settings sections remain functional

**Action timeout:**
- No response in 5s → toast "Configuration update timed out"
- UI allows retry

### Validation Rules

| Field | Validation | Error Message |
|-------|-----------|---------------|
| night_temp, day_temp, static_temp | 1000-10000 | "Temperature must be between 1000-10000K" |
| night_gamma, day_gamma, static_gamma | 10-200 | "Gamma must be between 10-200%" |
| sunrise, sunset | HH:MM:SS format | "Time must be in HH:MM:SS format" |
| latitude | -90 to 90 | "Latitude must be between -90 and 90" |
| longitude | -180 to 180 | "Longitude must be between -180 and 180" |
| transition_duration | 1-180 | "Duration must be between 1-180 minutes" |
| preset name | No spaces, not "default", unique | "Invalid preset name" |

## Testing

### Plugin Tests

- Mock `sunsetr get --json all` → verify entity fields parsed
- Test field_state computation for each transition_mode
- Test preset config query with `--target`
- Test error handling (binary missing, non-zero exit)
- Mock `sunsetr set` → verify correct field/value pairs
- Test action error propagation
- Test `create_preset`, `delete_preset` flows
- Test duplicate/invalid preset name rejection

### UI Tests

- Test widget reconciliation with different FieldState values
- Test transition_mode change updates field states
- Test SpinRow min/max enforcement
- Test EntryRow validation (times, coordinates)
- Test preset name validation
- Test action emission on widget changes
- Test updating guard prevents loops

### Integration Tests

1. Load settings → verify subscription, config loaded, widgets populated
2. Change temperature → verify action, plugin calls sunsetr, entity updated
3. Switch transition mode → verify field states update, fields disabled
4. Create preset → verify created, config switches, combo updated
5. Delete preset → verify removed, switches to default
6. Switch preset → verify config loads, widgets update

### Manual Testing Checklist

- [ ] All 18 fields render correctly
- [ ] Grouping (Colors, Timing, Location, Advanced) is clear
- [ ] Advanced section collapses/expands
- [ ] transition_mode change updates field availability immediately
- [ ] SpinRow min/max constraints work
- [ ] Time/coordinate validation works
- [ ] Preset creation validates names
- [ ] Preset deletion shows confirmation
- [ ] Cannot delete "default"
- [ ] Preset switching loads correct config
- [ ] Changes persist across restarts
- [ ] Error toasts appear for failures
- [ ] Read-only fields show explanatory subtitle

## Comparison with Similar Tools

Our entity structure aligns with established Linux night light tools:

- **Redshift/Gammastep**: Use temp-day/temp-night, gamma, brightness, transition boolean, location provider, lat/lon
- **wlsunset**: Uses location, time-range, temp, transition duration, gamma

**sunsetr advantages:**
- More granular gamma control (separate for day/night/static)
- More transition modes (5 vs typically 2)
- Advanced smoothing controls (startup/shutdown/adaptive intervals)
- String-based to match sunsetr's JSON output exactly

## Implementation Notes

1. **Entity type constant**: Add `NIGHT_LIGHT_CONFIG_ENTITY_TYPE = "night-light-config"` to `waft-protocol`
2. **Plugin manifest**: Update `handle_provides()` to include both entity types
3. **Settings module**: Create `crates/settings/src/display/night_light_config_section.rs` or extend existing section
4. **Field state computation**: Centralize in plugin helper function `compute_field_states(transition_mode: &str) -> HashMap<String, FieldState>`
5. **Preset list**: Plugin should query available presets via filesystem scan of `~/.config/sunsetr/presets/` and include in entity or separate action response
6. **Initial target**: Default to "default" preset on startup, persist last selected preset in waft config if desired

## Future Enhancements

- **Preset import/export**: Allow sharing preset files
- **Geo mode helper**: Button to auto-detect location via geoclue
- **Visual preview**: Show color temperature preview swatch
- **Schedule override**: Temporary manual color override without changing config
- **Sync with GNOME/KDE**: Import settings from system night light if available
