## Why

The sunsetr plugin has multiple critical issues:
1. **Application hangs** when toggling during daylight - busy-polling caused by running tokio futures in glib context
2. **Incorrect state display** - toggle shows "off" even when sunsetr process is running
3. **Missing localized labels** - should display period-specific text like "Denní režim do {čas}" / "Noční světlo do {čas}"
4. **Missing preset menu** - no way to switch between sunsetr presets when running

Additionally, the codebase has two separate Feature Toggle components that should be unified into a single component with CSS-based variants.

## What Changes

- Fix runtime mixing bug: move tokio work from glib to tokio runtime using channels
- Correct sunsetr state logic: display "on" when process is running, regardless of period
- Add localized period labels with time display
- Make sunsetr toggle expandable when running, with preset menu
- Unify FeatureToggle and FeatureToggleExpandable into single component with CSS variants

## Capabilities

### New Capabilities

- `runtime-safety-sunsetr`: Safe runtime bridging for sunsetr plugin (tokio → glib via channels)

### Modified Capabilities

- `safe-widget-removal`: Extended with unified toggle component architecture

## Impact

- **Code affected**:
  - `src/features/sunsetr/mod.rs` - fix glib/tokio mixing
  - `src/features/sunsetr/ipc.rs` - add preset menu support
  - `src/ui/feature_toggle.rs` - unify with expandable variant
  - `src/ui/feature_toggle_expandable.rs` - merge into base component
- **Breaking changes**: None (internal refactor only)
- **Dependencies**: None added
