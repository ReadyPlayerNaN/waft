## Why

Users need to control display brightness from the shell overlay without switching to external applications. The system may have multiple controllable displays (laptop backlights via sysfs/brightnessctl, external monitors via DDC/CI), and users want both quick overall control and fine-grained per-display adjustment using a familiar slider interface consistent with audio controls.

## What Changes

- Add a new brightness plugin that discovers controllable displays at startup
- Create a single master slider control that adjusts all displays proportionally
- Include an expandable menu (when 2+ displays) with per-display sliders for fine-tuning
- Place brightness slider in the Controls slot, positioned after microphone controls
- Support multiple brightness backends:
  - `brightnessctl` for backlight devices (laptops, internal displays)
  - `ddcutil` for DDC/CI-capable external monitors
- Master slider shows average brightness; dragging scales all displays proportionally
- Dragging master to 0% sets all displays to 0%

## Capabilities

### New Capabilities
- `brightness-control`: Discovers controllable displays and provides master + per-display brightness sliders using brightnessctl and ddcutil backends

### Modified Capabilities
<!-- None - this is a new plugin that doesn't modify existing specs -->

## Impact

- **New files**: `src/features/brightness/` module with store, dbus (for backend commands), and widget components
- **Dependencies**: Requires `brightnessctl` and/or `ddcutil` CLI tools to be installed (graceful degradation if missing)
- **Plugin registration**: New plugin added to `src/features/mod.rs`
- **Config**: Optional `[brightness]` section in config for enabling/disabling backends
- **UI**: Single brightness slider in Controls slot with expandable per-display menu
