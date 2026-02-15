# Keyboard Layout Configuration Design

**Date:** 2026-02-15
**Status:** Approved
**Scope:** Niri compositor only

## Overview

This design introduces keyboard layout configuration in the waft-settings UI, allowing users to add, remove, and reorder keyboard layouts. The feature integrates with Niri's configuration file (`~/.config/niri/config.kdl`) and provides a graphical interface for managing XKB keyboard layouts.

## Goals

1. Allow users to add new keyboard layouts from the system XKB database
2. Enable removal of configured layouts
3. Support reordering layouts (affects cycling order)
4. Persist configuration in Niri's config file
5. Handle external config changes gracefully (detect via event stream)
6. Support multiple configuration modes (layout list, external file, system default)

## Non-Goals

- Multi-compositor support (Sway, Hyprland, etc.) - Niri only for this implementation
- XKB options configuration beyond basic layout switching
- Custom keymap file editing
- Layout variant selection (can be added later)

---

## Architecture

### Entity Model

**Two separate entity types:**

1. **`keyboard-layout`** (existing, read-only)
   - URN: `niri/keyboard-layout/default`
   - Shows current active layout and available layouts
   - Used by overview UI for display/switching
   - Actions: `cycle` (existing)

2. **`keyboard-layout-config`** (new, writable)
   - URN: `niri/keyboard-layout-config/default`
   - Represents the configured layout list in niri config
   - Used by settings UI for configuration
   - Actions: `add`, `remove`, `reorder`, `set-options`
   - Field: `config_mode` - Indicates which configuration method is active

**Rationale:** Separating read-only runtime state (`keyboard-layout`) from writable configuration (`keyboard-layout-config`) keeps the entity model clean and prevents confusion about what can be modified.

### Configuration Modes

The plugin detects and handles four modes:

```rust
enum KeyboardConfigMode {
    LayoutList,        // xkb { layout "us,de,cz" }
    ExternalFile,      // xkb { file "~/.config/keymap.xkb" }
    SystemDefault,     // xkb { } or missing - uses systemd-localed
    Malformed,         // Config exists but can't be parsed
}
```

**Mode-specific behavior:**

1. **LayoutList mode** (fully supported)
   - All actions enabled: add, remove, reorder, set-options
   - Entity shows: `config_mode: "editable"`, layouts list

2. **ExternalFile mode** (read-only with hint)
   - Actions disabled
   - Entity shows: `config_mode: "external-file"`, `file_path: "~/.config/keymap.xkb"`
   - Settings UI displays: "Using custom XKB file. To configure layouts here, remove the 'file' option from niri config."

3. **SystemDefault mode** (can bootstrap)
   - Actions enabled
   - On first `add` action: Create xkb section with layout list
   - Entity shows: `config_mode: "system-default"`
   - Settings UI displays: "Using system defaults. Add a layout to start configuring."

4. **Malformed mode** (read-only with error)
   - Actions disabled
   - Entity shows: `config_mode: "error"`, `error_message: "..."`
   - Settings UI displays error and suggests manual config fix

### Plugin State

```rust
struct NiriState {
    keyboard: KeyboardLayoutState,           // Runtime state (existing)
    keyboard_config: KeyboardLayoutConfig,   // Config file state (new)
    outputs: HashMap<String, OutputState>,   // Display outputs (existing)
}

struct KeyboardLayoutConfig {
    mode: KeyboardConfigMode,
    layouts: Vec<String>,          // Empty if not in LayoutList mode
    variant: Option<String>,
    options: Option<String>,
    file_path: Option<String>,     // Set if ExternalFile mode
    error_message: Option<String>, // Set if Malformed mode
}
```

### Event Stream Integration

The niri plugin already monitors `niri msg -j event-stream`. Add handling for config changes:

```rust
// In event stream handler (existing):
NiriEvent::KeyboardLayoutsChanged { .. } => { /* existing */ }
NiriEvent::KeyboardLayoutSwitched { .. } => { /* existing */ }

// NEW: Handle external config changes
NiriEvent::ConfigReloaded => {
    // Re-parse niri config file
    match parse_niri_config() {
        Ok(config) => {
            let new_mode = detect_config_mode(&config);
            let new_layouts = extract_layouts(&config);

            // Update state if changed
            if state.keyboard_config.mode != new_mode
                || state.keyboard_config.layouts != new_layouts {
                state.keyboard_config = new_config;
                notifier.notify(); // Emit keyboard-layout-config entity
            }
        }
        Err(e) => {
            // Config became malformed
            state.keyboard_config.mode = Malformed;
            state.keyboard_config.error_message = Some(e.to_string());
            notifier.notify();
        }
    }

    // Also re-query display outputs (existing behavior)
    display::query_outputs().await;
}
```

**Why this matters:**
- User edits `~/.config/niri/config.kdl` manually → niri reloads → `ConfigReloaded` event
- Settings UI automatically reflects the external change
- Handles mode transitions (e.g., user adds `file` option → UI switches to read-only)
- Detects if config becomes malformed after manual edit

### Configuration Flow

1. **Settings UI → Daemon → Niri Plugin:** User adds layout "fr" via settings
2. **Plugin reads config:** Parse `~/.config/niri/config.kdl`
3. **Plugin modifies:** Update `input.keyboard.xkb.layout` to "us,de,cz,fr"
4. **Plugin writes:** Save modified KDL back to file with backup
5. **Plugin reloads niri:** Execute `niri msg action reload-config`
6. **Plugin notifies daemon:** Send updated `keyboard-layout-config` entity
7. **Daemon → Settings UI:** Settings UI shows updated layout list

---

## Components

### Protocol Layer (`crates/protocol/`)

**New entity type in `src/entity/keyboard.rs`:**

```rust
/// Keyboard layout configuration entity.
/// Represents the configured layouts in niri's config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayoutConfig {
    pub mode: String,  // "editable", "external-file", "system-default", "error"
    pub layouts: Vec<String>,  // e.g., ["us", "de", "cz"]
    pub variant: Option<String>,
    pub options: Option<String>,
    pub file_path: Option<String>,  // Set if mode == "external-file"
    pub error_message: Option<String>,  // Set if mode == "error"
}

pub const CONFIG_ENTITY_TYPE: &str = "keyboard-layout-config";
```

**Actions:**
- `add` - params: `{ "layout": "fr" }` or `{ "layout": "fr", "variant": "azerty" }`
- `remove` - params: `{ "layout": "de" }`
- `reorder` - params: `{ "layouts": ["cz", "us", "de"] }`
- `set-options` - params: `{ "options": "grp:win_space_toggle,compose:ralt" }`

### Niri Plugin (`plugins/niri/`)

**New module: `src/config.rs`**
- `parse_niri_config() -> Result<NiriConfig>` - Parse `~/.config/niri/config.kdl` using kdl-rs
- `detect_config_mode(&NiriConfig) -> KeyboardConfigMode` - Determine which mode
- `extract_keyboard_config(&NiriConfig) -> KeyboardLayoutConfig` - Extract keyboard section
- `modify_keyboard_layouts(&mut NiriConfig, layouts: Vec<String>)` - Modify layout list
- `write_niri_config(&NiriConfig) -> Result<()>` - Write back to file with backup

**Updated: `src/keyboard.rs`**
- Add `to_config_entity(state: &KeyboardLayoutConfig) -> Entity` - Convert config to entity
- Keep existing `to_entity()` for runtime keyboard-layout entity

**Updated: `bin/waft-niri-daemon.rs`**
- Load keyboard config on startup (in addition to runtime layouts)
- Handle new actions: `add`, `remove`, `reorder`, `set-options`
- Re-parse config on `ConfigReloaded` event
- Emit both `keyboard-layout` and `keyboard-layout-config` entities

**Updated: `src/state.rs`**
```rust
pub struct NiriState {
    pub keyboard: KeyboardLayoutState,        // Runtime (existing)
    pub keyboard_config: KeyboardLayoutConfig, // Config file (new)
    pub outputs: HashMap<String, OutputState>,
}
```

### Settings UI (`crates/settings/`)

**New page: `src/pages/keyboard.rs`** (smart container)
- Subscribe to `keyboard-layout-config` entity
- Reconcile on entity updates
- Create keyboard layout configuration UI
- Emit actions via `EntityActionCallback`

**New module: `src/keyboard/` (dumb widgets)**
- `layout_list.rs` - Reorderable list of configured layouts with drag-handle icons
- `layout_row.rs` - Single layout row (abbreviation, full name, remove button)
- `add_layout_dialog.rs` - Dialog to add new layout (search/filter available layouts)
- `mode_banner.rs` - Info banner explaining current config mode

**New: `src/keyboard/available_layouts.rs`**
- Parse `/usr/share/X11/xkb/rules/base.lst` for available XKB layouts
- Returns: `Vec<(code: String, name: String, variants: Vec<String>)>`
- Example: `("us", "English (US)", vec!["dvorak", "colemak", ...])`

**Updated: `src/sidebar.rs`**
- Add "Keyboard" row with `input-keyboard-symbolic` icon

**Updated: `src/window.rs`**
- Add "Keyboard" case to page stack switching

### Dependencies

**New crates:**
- `kdl` (v6.0+) - KDL parser/serializer for niri config
- Add to `plugins/niri/Cargo.toml`

**No GTK4 changes needed** - Use existing widgets (`gtk::ListBox`, `adw::ActionRow`, `gtk::DragSource`/`gtk::DropTarget` for reordering)

---

## Data Flow

### Initial Load (Plugin Startup)

```
1. Plugin starts
   ↓
2. Parse ~/.config/niri/config.kdl
   ↓
3. Detect config mode (LayoutList/ExternalFile/SystemDefault/Malformed)
   ↓
4. Extract keyboard config (layouts, variant, options)
   ↓
5. Emit keyboard-layout-config entity to daemon
   ↓
6. Settings UI subscribes → receives entity → renders page
```

**Entity flow:**
```
Niri Plugin → Daemon (EntityUpdated) → Settings App → EntityStore → Page reconciliation → UI update
```

### User Adds Layout

```
1. User clicks "Add Layout" button in settings
   ↓
2. Dialog shows available layouts (parsed from /usr/share/X11/xkb/rules/base.lst)
   ↓
3. User selects "French (fr)"
   ↓
4. Settings UI emits action: TriggerAction("niri/keyboard-layout-config/default", "add", {"layout": "fr"})
   ↓
5. Daemon routes action to niri plugin
   ↓
6. Plugin handles action:
   a. Check mode == LayoutList (reject if not)
   b. Parse config.kdl
   c. Backup to config.kdl.backup
   d. Modify: layout "us,de,cz" → "us,de,cz,fr"
   e. Write config.kdl
   f. Execute: niri msg action reload-config
   g. Re-parse config to verify
   h. Update state.keyboard_config.layouts
   i. Notify daemon
   ↓
7. Daemon sends EntityUpdated(keyboard-layout-config) to settings app
   ↓
8. Settings UI reconciles → shows "fr" in layout list
```

**Action flow:**
```
Settings UI → Daemon (TriggerAction) → Niri Plugin → Config file → niri reload → Plugin notify → Daemon → Settings UI
```

### User Reorders Layouts

```
1. User drags "cz" layout above "us" in the list
   ↓
2. On drop: Settings UI emits action: TriggerAction(..., "reorder", {"layouts": ["cz", "us", "de", "fr"]})
   ↓
3. Plugin modifies config: layout "us,de,cz,fr" → "cz,us,de,fr"
   ↓
4. Same write/reload/notify flow as add
   ↓
5. Settings UI updates to show new order
```

### External Config Change

```
1. User manually edits ~/.config/niri/config.kdl (changes layout list or adds "file" option)
   ↓
2. User runs: niri msg action reload-config (or niri auto-reloads)
   ↓
3. Niri emits ConfigReloaded event in event stream
   ↓
4. Plugin event handler catches ConfigReloaded:
   a. Re-parse config.kdl
   b. Detect new mode (e.g., ExternalFile if user added "file" option)
   c. Extract new layouts
   d. Update state.keyboard_config
   e. Notify daemon
   ↓
5. Daemon sends EntityUpdated(keyboard-layout-config)
   ↓
6. Settings UI reconciles:
   - If mode changed to ExternalFile: Shows info banner, disables actions
   - If layouts changed: Updates list to match new config
```

### Error Scenarios

**Scenario A: Malformed config**
```
1. Plugin tries to parse config.kdl → KDL parse error
   ↓
2. Set mode = Malformed, error_message = "..."
   ↓
3. Emit entity with mode="error"
   ↓
4. Settings UI shows error banner: "Config file has errors. Please fix manually."
   ↓
5. All action buttons disabled
```

**Scenario B: Write failure**
```
1. User adds layout "fr"
   ↓
2. Plugin writes config.kdl → Permission denied
   ↓
3. Restore from .backup file
   ↓
4. Return error to daemon → ActionError sent to settings
   ↓
5. Settings UI shows toast: "Failed to save configuration: Permission denied"
   ↓
6. Layout list unchanged (rollback)
```

**Scenario C: Niri reload failure**
```
1. User adds layout "fr"
   ↓
2. Config written successfully
   ↓
3. Execute niri msg action reload-config → Command fails
   ↓
4. Log warning (don't fail the action - config is saved, user can reload manually)
   ↓
5. Return success to settings (config was updated)
   ↓
6. Settings UI shows layout added
```

---

## Error Handling

### Principles

1. **Fail gracefully** - Never crash the plugin; degrade to read-only mode on errors
2. **Preserve user data** - Always backup before modifying config
3. **Clear feedback** - Show actionable error messages in the UI
4. **Auto-recovery** - Re-detect config mode on every ConfigReloaded event

### Config Parsing Errors

**Error:** KDL parsing fails (syntax error, invalid structure)

**Handling:**
```rust
match parse_niri_config() {
    Ok(config) => { /* normal flow */ }
    Err(e) => {
        log::error!("[niri] Failed to parse config.kdl: {}", e);
        state.keyboard_config = KeyboardLayoutConfig {
            mode: "error",
            layouts: vec![],
            error_message: Some(format!("Config file has syntax errors: {}", e)),
            ..Default::default()
        };
        notifier.notify(); // Emit entity with error state
    }
}
```

**User experience:**
- Settings page shows error banner: "Config file has syntax errors. Please check ~/.config/niri/config.kdl"
- All action buttons disabled
- Entity still emitted (UI doesn't break)

### Config Write Errors

**Errors:**
- Permission denied
- Disk full
- File locked by another process

**Handling:**
```rust
// Backup first
std::fs::copy(config_path, format!("{}.backup", config_path))?;

match write_niri_config(&modified_config) {
    Ok(()) => { /* proceed with reload */ }
    Err(e) => {
        log::error!("[niri] Failed to write config: {}", e);
        // Restore from backup
        if let Err(restore_err) = std::fs::copy(backup_path, config_path) {
            log::error!("[niri] Failed to restore backup: {}", restore_err);
        }
        return Err(format!("Failed to save configuration: {}", e).into());
    }
}
```

**User experience:**
- Settings UI receives ActionError
- Shows toast notification: "Failed to save configuration: Permission denied"
- Layout list unchanged (optimistic UI update rolled back)

### Niri Reload Errors

**Error:** `niri msg action reload-config` command fails or times out

**Handling:**
```rust
match niri_reload_config().await {
    Ok(()) => log::info!("[niri] Config reloaded successfully"),
    Err(e) => {
        log::warn!("[niri] Config reload failed (config saved but not applied): {}", e);
        // Don't fail the action - config file was updated successfully
        // User can reload manually or niri will pick it up on next restart
    }
}
// Continue with success response
```

**User experience:**
- Action succeeds (config was saved)
- Optional: Show info toast: "Configuration saved. Run 'niri msg action reload-config' to apply."
- Layout list updates to show new config

**Rationale:** Config file modification succeeded, which is persistent. Reload is a "best effort" step.

### Unsupported Config Mode Errors

**Error:** User tries to add layout while in ExternalFile or Malformed mode

**Handling:**
```rust
async fn handle_action(...) -> Result<()> {
    let config = self.lock_state().keyboard_config.clone();

    if config.mode != "editable" {
        return Err(format!(
            "Cannot modify layouts in '{}' mode. {}",
            config.mode,
            mode_help_text(&config.mode)
        ).into());
    }

    // Proceed with action...
}

fn mode_help_text(mode: &str) -> &'static str {
    match mode {
        "external-file" => "Remove the 'file' option from niri config to enable editing.",
        "system-default" => "This shouldn't happen - system-default allows adding layouts.",
        "error" => "Fix config file errors first.",
        _ => "",
    }
}
```

**User experience:**
- Action buttons disabled in UI for non-editable modes
- If user somehow triggers action anyway: ActionError with helpful message
- Info banner explains why editing is disabled

### XKB Layout Database Errors

**Error:** Cannot parse `/usr/share/X11/xkb/rules/base.lst` (file missing or malformed)

**Handling:**
```rust
fn load_available_layouts() -> Vec<AvailableLayout> {
    match parse_xkb_layouts() {
        Ok(layouts) => layouts,
        Err(e) => {
            log::warn!("[keyboard-page] Failed to parse XKB database: {}", e);
            // Fallback to hardcoded common layouts
            vec![
                AvailableLayout { code: "us".into(), name: "English (US)".into(), variants: vec![] },
                AvailableLayout { code: "gb".into(), name: "English (UK)".into(), variants: vec![] },
                AvailableLayout { code: "de".into(), name: "German".into(), variants: vec![] },
                AvailableLayout { code: "fr".into(), name: "French".into(), variants: vec![] },
                // ... more common layouts
            ]
        }
    }
}
```

**User experience:**
- Add layout dialog still works with fallback list
- User can type custom layout code if their desired layout isn't in fallback list

### Logging Strategy

**Error levels:**
- `error!()` - Config parsing failures, write failures requiring rollback
- `warn!()` - Reload failures (non-critical), backup restoration failures
- `info!()` - Successful config modifications, mode transitions
- `debug!()` - Action handling, entity emissions

**All errors logged before returning** to ensure visibility in plugin logs.

---

## Testing

### Unit Tests (Plugin Layer)

**Test module: `plugins/niri/src/config.rs`**

```rust
#[cfg(test)]
mod tests {
    // Config parsing tests
    #[test]
    fn parse_config_with_layout_list();

    #[test]
    fn parse_config_with_external_file();

    #[test]
    fn parse_config_missing_keyboard_section();

    #[test]
    fn parse_malformed_kdl();

    // Config modification tests
    #[test]
    fn add_layout_to_list();

    #[test]
    fn remove_layout_from_list();

    #[test]
    fn reorder_layouts();

    #[test]
    fn preserve_other_config_sections();
}
```

### Integration Tests (Entity Actions)

**Test: Full action flow with mocked config file**

```rust
#[tokio::test]
async fn test_add_layout_action();

#[tokio::test]
async fn test_reject_action_in_external_file_mode();
```

### Settings UI Tests

**Test: EntityStore subscription and reconciliation**

```rust
#[gtk::test]
fn test_keyboard_page_reconciliation();
```

### Manual Testing Checklist

**Happy path:**
- [ ] Open settings → Keyboard page shows current layouts
- [ ] Add layout "fr" → Appears in list, config file updated
- [ ] Reorder layouts via drag-and-drop → Order changes in list and config
- [ ] Remove layout → Disappears from list, config updated
- [ ] External config edit → Settings UI updates automatically on reload

**Edge cases:**
- [ ] Config with `file` option → UI shows info banner, actions disabled
- [ ] Config with empty `xkb {}` section → Can add first layout (bootstrap)
- [ ] Malformed config → UI shows error banner, actions disabled
- [ ] No write permission on config → Error toast shown, changes rolled back
- [ ] XKB database missing → Add dialog uses fallback layout list

**Error recovery:**
- [ ] Break config manually → UI shows error → Fix config → UI recovers on reload
- [ ] Switch from `layout` to `file` in config → UI switches to read-only mode
- [ ] Switch from `file` back to `layout` → UI switches to editable mode

**Multi-entity:**
- [ ] Add layout in settings → Overview keyboard widget updates immediately
- [ ] Cycle layout in overview → Settings page shows new active layout

---

## Future Enhancements

1. **Layout variant selection** - Allow choosing variants like "dvorak", "colemak" for each layout
2. **XKB options editor** - GUI for configuring options like compose key, caps lock behavior
3. **Multi-compositor support** - Extend to Sway, Hyprland with backend abstraction
4. **Layout preview** - Show visual keyboard layout preview in add dialog
5. **Import/export** - Share layout configurations between systems
