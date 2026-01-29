## Why

The main window collects widgets from plugins once during `MainWindow::build_content()`. Plugins cannot dynamically show/hide widgets at runtime (e.g., when a USB network adapter is plugged in or a wired cable is connected). This prevents responsive UI updates without passing main window references to plugins, which would create tight coupling.

## What Changes

- Introduce a reactive widget registry that plugins can update at runtime
- Main window subscribes to registry changes and rebuilds affected UI sections
- Plugins emit widget changes through the existing store/channel patterns instead of exposing static widget lists
- **BREAKING**: `Plugin::get_widgets()` and `Plugin::get_feature_toggles()` return types may change to support dynamic updates

## Capabilities

### New Capabilities

- `reactive-widget-registry`: A subscription-based registry that notifies the main window when plugins add, remove, or modify their published widgets and feature toggles

### Modified Capabilities

- `network-adapter-separation`: Will need to publish/unpublish adapter widgets dynamically based on hardware state (already exists in openspec/specs/)

## Impact

- `src/plugin_registry.rs` - Add subscription mechanism for widget changes
- `src/plugin.rs` - Modify Plugin trait for dynamic widget registration
- `src/ui/main_window.rs` - Subscribe to registry and rebuild UI sections on change
- `src/ui/feature_grid.rs` - Support dynamic toggle addition/removal
- All existing plugins - Adapt to new registration pattern (audio, brightness, networkmanager, power)
