## Context

The shell overlay currently has audio volume controls (speakers and microphone) using `SliderControlWidget` in the Controls slot. Users want similar brightness controls for their displays. Linux systems offer multiple brightness control mechanisms depending on hardware:

- **Laptop backlights**: Controlled via `/sys/class/backlight/` or the `brightnessctl` CLI tool
- **External monitors**: Controlled via DDC/CI protocol using the `ddcutil` CLI tool (when supported by monitor)

The current system has no `/sys/class/backlight` devices (desktop with external monitor), and the connected monitor (Samsung LS49AG95) does not support DDC/CI. This design must handle graceful degradation when no controllable displays exist.

## Goals / Non-Goals

**Goals:**
- Discover all controllable displays at plugin initialization
- Provide a single master slider that controls overall brightness (average of all displays)
- Include expandable menu with per-display sliders when multiple displays exist
- Position brightness slider after microphone controls (weight 60)
- Support both `brightnessctl` (backlight) and `ddcutil` (DDC/CI) backends
- Gracefully handle systems with no controllable displays (plugin remains silent)

**Non-Goals:**
- Monitor hotplug detection (out of scope for initial implementation)
- Automatic backend installation or prompts to install missing tools
- Night light / color temperature controls
- Per-app or per-window brightness (this is display hardware brightness only)

## Decisions

### 1. Use CLI tools instead of direct sysfs/i2c access

**Decision**: Shell out to `brightnessctl` and `ddcutil` rather than direct filesystem/i2c operations.

**Rationale**:
- Consistent with audio plugin's use of `pactl` CLI
- Avoids permission issues (i2c-dev requires root or udev rules; brightnessctl handles this)
- Tools handle edge cases and hardware quirks we'd otherwise need to implement
- Easier maintenance as tools are updated independently

**Alternatives considered**:
- Direct `/sys/class/backlight` access: Simpler but requires proper permissions and doesn't work for DDC
- Rust i2c crate + DDC protocol: Too complex, ddcutil already handles this well

### 2. Single master slider with expandable per-display menu

**Decision**: Create one master `SliderControlWidget` that controls all displays, with an expandable menu containing individual sliders for fine-tuning.

**Rationale**:
- Reduces visual clutter - one slider instead of N
- Provides quick "adjust everything" for common case
- Still allows per-display control when needed via expandable menu
- Consistent with audio control pattern (slider + expandable menu)

**Alternatives considered**:
- One slider per display in Controls slot: Too much visual noise with multiple displays
- Dropdown to select display + single slider: More clicks, doesn't allow simultaneous view of all displays

### 3. Master slider shows average, scales proportionally

**Decision**: Master slider value = arithmetic average of all display brightness levels. Dragging master scales all displays proportionally.

**Rationale**:
- Average gives meaningful "overall brightness" indication
- Proportional scaling preserves user's relative preferences between displays
- Dragging to 0% guarantees all displays reach 0% (mathematically sound)

**Scaling formula**: `new_value = current_value × (new_master / old_master)`

**Special case**: When all displays are at 0% (old_master = 0), dragging master up sets all displays to the new master value (additive instead of multiplicative).

### 4. Hide expandable menu for single display

**Decision**: When only one controllable display exists, hide the expand button entirely. Master slider directly controls that display.

**Rationale**:
- No value in showing a menu with one item
- Simpler UX for the common laptop case
- Master and individual would show same value anyway

### 5. Backend discovery at init, not on-demand

**Decision**: Detect available backends and enumerate displays during `init()`, cache the result.

**Rationale**:
- Avoids repeated CLI calls during normal operation
- Display configuration rarely changes within a session
- Consistent with how audio plugin loads state at init

### 6. Per-display menu rows: icon + slider + truncated name

**Decision**: Each row in the expandable menu shows: display type icon, brightness slider, truncated display name.

**Rationale**:
- Icon provides visual type distinction (laptop vs external)
- Slider allows direct manipulation
- Truncated name fits limited horizontal space while remaining identifiable

**Ordering**: Backlight devices first, then external monitors, alphabetically within each group.

### 7. Icon and click behavior

**Decision**: Use `display-brightness-symbolic` icon. Icon click has no action.

**Rationale**:
- Generic brightness icon appropriate for master control
- Unlike audio, brightness has no "mute" concept - setting to 0% is the equivalent

## Risks / Trade-offs

**ddcutil is slow** (~100-500ms per operation) → Cache brightness values aggressively; only call ddcutil on user-initiated changes, not polling. Accept slight staleness if brightness changed externally.

**brightnessctl may require permissions** → Document udev rules in README if needed. The tool typically handles this via polkit.

**No controllable displays on some systems** → Plugin registers no widgets and logs a debug message. No user-visible error.

**CLI tools may not be installed** → Check for tool availability at init; skip that backend if missing. Log info message.

**Brightness ranges vary** → Both tools report 0-max range. Normalize to 0.0-1.0 for slider. Handle edge cases (max=0 means device unsupported).

**Proportional scaling edge case** → When all displays at 0%, use additive scaling instead of multiplicative to allow recovery.
