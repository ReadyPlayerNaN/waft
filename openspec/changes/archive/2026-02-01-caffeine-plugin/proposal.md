## Why

Users need a way to temporarily prevent screen lock/screensaver activation (caffeine mode). This is useful during presentations, watching videos, or any activity where the screen shouldn't blank despite no keyboard/mouse input.

## What Changes

- Add a new `caffeine` plugin that provides a feature toggle to inhibit screen lock
- Plugin uses compositor-agnostic D-Bus interfaces with fallback chain
- Plugin only appears if a supported inhibit interface is detected during initialization

## Capabilities

### New Capabilities

- `screen-inhibit`: Compositor-agnostic screen lock/screensaver inhibition via D-Bus interfaces (portal and legacy ScreenSaver)

### Modified Capabilities

None - this is a standalone new plugin.

## Impact

- **New files**: `src/features/caffeine/` module (mod.rs, dbus.rs, store.rs)
- **Modified files**: `src/features/mod.rs` (module export), `src/app.rs` (plugin registration)
- **Dependencies**: Uses existing `zbus` crate for D-Bus communication
- **Config**: New plugin ID `plugin::caffeine` for enabling in config.toml
