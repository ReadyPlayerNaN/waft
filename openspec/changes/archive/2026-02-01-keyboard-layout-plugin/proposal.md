## Why

Users need quick access to view and switch keyboard layouts from the overlay interface. Currently there's no compositor-agnostic way to display the active keyboard layout or cycle through available layouts, forcing users to use compositor-specific tools or system settings.

## What Changes

- Add a new keyboard layout plugin that displays the current keyboard layout in the overlay header
- Display the active layout as a button in the top-right header area, similar to the systemd actions plugin
- Clicking the button cycles through available keyboard layouts
- Use compositor-agnostic D-Bus interfaces (XKB via systemd-localed or equivalent) to query and switch layouts
- Include comprehensive unit and integration tests for layout detection, switching logic, and UI components
- Provide README documentation for the plugin covering setup, D-Bus dependencies, and compositor compatibility

## Capabilities

### New Capabilities
- `keyboard-layout-widget`: Widget displaying current keyboard layout with click-to-cycle functionality
- `keyboard-layout-dbus`: D-Bus integration for compositor-agnostic layout querying and switching via XKB/localed

### Modified Capabilities
<!-- No existing capabilities are being modified -->

## Impact

**Code:**
- New plugin directory: `src/plugins/keyboard_layout/`
- Integration with reactive widget registry for header slot registration
- New D-Bus client for XKB/systemd-localed interaction

**Dependencies:**
- D-Bus integration with systemd-localed (org.freedesktop.locale1) or similar compositor-agnostic service
- XKB configuration access via D-Bus

**Testing:**
- Unit tests for layout parsing, switching logic, and state management
- Integration tests for D-Bus interaction
- Widget tests for UI component behavior

**Documentation:**
- Plugin README covering D-Bus requirements, compositor compatibility, and configuration
