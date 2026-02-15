# Niri Plugin

Niri compositor plugin providing keyboard layout and display output management.

## Entity Types

### `keyboard-layout`

Active keyboard layout and available alternatives.

**URN:** `niri/keyboard-layout/default`

**Actions:**
- `cycle` - Switch to the next keyboard layout

### `display-output`

Display outputs with resolution, refresh rate, and VRR configuration.

**URN:** `niri/display-output/<output-name>` (e.g., `niri/display-output/DP-3`)

**Actions:**
- `set-mode` - Change display mode. Params: `{"mode_index": <index>}`
- `toggle-vrr` - Toggle variable refresh rate

## Dependencies

- Niri compositor must be running
- `NIRI_SOCKET` environment variable must be set
- `niri` binary must be in PATH

## Event Stream

The plugin monitors `niri msg --json event-stream` for real-time updates:

- `KeyboardLayoutsChanged` - Full layout info at startup and config reload
- `KeyboardLayoutSwitched` - Layout index change on user switch
- `ConfigLoaded` - Triggers re-query of display outputs

## Migration

Keyboard layout support for Niri was previously provided by the `keyboard-layout` plugin's
Niri backend. That backend has been removed in favor of this dedicated plugin, which also
provides display output management.
