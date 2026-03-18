# Display/Brightness Unification -- Implementation Plan

## Current State

Two separate entity types represent the same physical displays:

### `display` entity (brightness plugin)

- **Source:** `plugins/brightness/bin/waft-brightness-daemon.rs`
- Provides brightness control via `brightnessctl` (backlight) and `ddcutil` (DDC/CI)
- URN: `brightness/display/{device-id}` where device-id is `backlight:intel_backlight` or `ddc:1`
- Entity: `name`, `brightness`, `kind` (DisplayKind)
- `name` is human-readable: "Built-in Display" for backlight, model string from ddcutil for external
- **No connector name** (e.g., "DP-3", "eDP-1") is captured

### `display-output` entity (niri plugin)

- **Source:** `plugins/niri/bin/waft-niri-daemon.rs`
- Provides resolution, refresh rate, scale, rotation, VRR, enable/disable
- URN: `niri/display-output/{connector}` where connector is `DP-3`, `eDP-1`, etc.
- Entity: `name` (connector), `make`, `model`, `current_mode`, `available_modes`, `vrr_supported`, `vrr_enabled`, `enabled`, `scale`, `transform`, `physical_size`, `connection_type`

### In waft-settings (`crates/settings/src/pages/display.rs`)

- `BrightnessSection` renders one `adw::PreferencesGroup` per `display` entity with a brightness slider
- `OutputSection` renders one `adw::PreferencesGroup` per `display-output` entity with resolution/scale/rotation/VRR and Apply/Reset buffered editing
- These appear as **separate, uncorrelated groups** on the same page

**User's complaint:** "I have brightness controls for 'Built-in Display' and display controls for 'California Institute of Technology 0x1413'. These are the same displays and should manifest as same UI."

---

## How Displays Can Be Correlated

| Aspect | Brightness Plugin | Niri Plugin |
|--------|------------------|-------------|
| ID scheme | `backlight:intel_backlight`, `ddc:1` | `DP-3`, `eDP-1` |
| Name field | "Built-in Display", model string | Connector name |
| Model info | From ddcutil `Model:` line | From niri `model` field |
| Connector | **Not captured** | Primary identifier |

**Correlation strategies:**

1. **Backlight → Internal display:** Backlight devices correspond to internal displays. Niri marks these with `connection_type: "Internal"`. If one backlight + one internal output → same display. Multiple internal displays are extremely rare.

2. **DDC display number → connector:** `ddcutil detect` (non-brief) outputs `DRM connector: card0-DP-3` which maps directly to niri's connector name. The brightness plugin currently uses `--brief` which omits this.

3. **Model name matching:** ddcutil `Model:` line often matches niri's `model` field (both from EDID). But exact strings may differ. Fragile.

4. **Add `connector` field to `Display` entity:** Most robust. Brightness plugin extracts connector info, UI matches trivially.

---

## Design Options

### Option A: Merge in UI Layer Only

Heuristic matching (connection_type + model name) in the settings app. No protocol/plugin changes.

**Pros:** Fastest. **Cons:** Fragile for multiple external monitors of same model.

### Option B: Add Connector Field to Display Entity (Recommended)

Extend brightness plugin to extract DRM connector name. Add optional `connector` field to `Display` protocol entity. Settings app uses exact match.

**Pros:** Exact matching. Clean separation. Compatible with existing architecture.
**Cons:** Requires protocol + plugin changes. `ddcutil detect` (non-brief) is slower.

### Option C: Merge Entity Types in Protocol

Single unified entity from one plugin.

**Cons:** Violates architecture. Would require orchestrator plugin combining data from two backends.

### Option D: Merge in the Daemon

Daemon-level entity correlation mechanism.

**Cons:** Massive architectural change. Over-engineered.

---

## Recommended Approach: Option B

Follows "plugins provide entities, apps render UI" while adding just enough information for the UI.

---

## Implementation Steps

### Phase 1: Protocol -- Add connector field

`crates/protocol/src/entity/display.rs`:

```rust
pub struct Display {
    pub name: String,
    pub brightness: f64,
    pub kind: DisplayKind,
    #[serde(default)]
    pub connector: Option<String>,
}
```

`#[serde(default)]` ensures backward compatibility.

### Phase 2: Plugin -- Extract connector in brightness plugin

`plugins/brightness/bin/waft-brightness-daemon.rs`:

**For backlight devices:**
- Read `/sys/class/backlight/{device}/device` symlink to find PCI device
- Enumerate `/sys/class/drm/card*-*/enabled` to find connector name
- Alternatively, pragmatically assume backlight maps to first `eDP-*` connector

**For DDC monitors:**
- Run `ddcutil detect` (without `--brief`) and parse `DRM connector: card0-DP-3` line
- Extract connector name (strip `card0-` prefix)
- Add to `Display` struct's `connector` field

### Phase 3: Settings -- Unify display page

`crates/settings/src/pages/display.rs`:

Replace separate `BrightnessSection` + `OutputSection` with unified section:

1. Subscribe to both entity types (can use existing `subscribe_dual_entities` pattern)
2. Group entities by connector into `HashMap<String, UnifiedDisplayInfo>`
3. For each group, render a single `adw::PreferencesGroup` containing:
   - Brightness slider (if `display` entity exists for that connector)
   - Output controls (if `display-output` entity exists)
4. Handle cases where only one entity type is available

**Correlation logic:**
```
fn correlate(displays, outputs) -> Vec<UnifiedDisplayInfo> {
    // 1. Match by connector field (exact match when available)
    // 2. For displays without connector: match backlight to Internal connection_type
    // 3. Unmatched entities appear as standalone groups
}
```

**Important:** Preserve the Apply/Reset buffered editing pattern from `OutputSection`. Brightness changes remain immediate, output changes stay buffered. These two interaction models coexist within the same group.

---

## Questions Requiring User Input

1. **Backlight-to-connector mapping:** Assume all backlights map to internal displays (`connection_type: "Internal"`), or invest in sysfs path traversal? Most laptops have one backlight + one internal display.

2. **ddcutil performance:** `ddcutil detect` without `--brief` is slower. Extract connector at startup only and cache? Current discovery runs only at startup anyway.

3. **Connector extraction approach for DDC:**
   - (a) Run `ddcutil detect` without `--brief` and parse `DRM connector:` line
   - (b) Run `--brief` first for speed, then second call for connector
   - (c) Use `--terse` which may include connector

4. **Apply/Reset in unified view:** Brightness is immediate, output changes are buffered. Both in same group -- natural or confusing?

5. **Fallback when connector unavailable:** If brightness plugin can't determine connector, fall back to heuristic matching or show separate uncorrelated groups?

6. **Naming priority:** When both entities correlated, what should the group title show? Suggestion: use niri's `make + model` as primary, connector in description.

---

## Critical Files

- `crates/protocol/src/entity/display.rs` - Add `connector` field to `Display`
- `plugins/brightness/bin/waft-brightness-daemon.rs` - Extract connector from sysfs/ddcutil
- `crates/settings/src/pages/display.rs` - Rewrite to unified section
- `crates/settings/src/display/output_section.rs` - Refactor to accept brightness data
- `crates/settings/src/display/brightness_section.rs` - Extract helpers or merge into unified section
