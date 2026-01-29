## Why

The UI currently has inconsistent menu item designs across different plugins. The slider control menu uses a clean, reusable menu item component that isn't being leveraged by bluetooth and wifi menus. Additionally, the networkmanager wired feature toggle doesn't follow the pattern of other toggles, lacks connection detail display, has broken connection status, and contains a hardcoded untranslated "Disabled" label. Unifying these components will improve consistency, user experience, and maintainability.

## What Changes

- Extract the menu item component from slider controls into a reusable component
- Update bluetooth menu to use the extracted menu item component with clickable primary actions
- Update wifi menu to use the extracted menu item component with clickable primary actions
- Refactor networkmanager wired plugin to use the expandable feature toggle component
- Add connection details display (IP address, subnet mask, default gateway) to wired connection when expanded
- Fix connection status display in networkmanager wired plugin
- Replace hardcoded "Disabled" label with translated i18n string

## Capabilities

### New Capabilities
- `menu-item-component`: Extractable, reusable menu item component for plugin menus with clickable primary actions
- `network-connection-details`: Display of network connection information (IP, mask, gateway) in expandable UI
- `network-wired-ui`: Wired network UI using expandable feature toggle with connection status and details

### Modified Capabilities

## Impact

**Affected code:**
- UI components: slider controls menu item extraction
- Bluetooth plugin: menu widget implementation
- WiFi menu: menu widget implementation
- NetworkManager plugin: wired feature toggle and display logic
- i18n: translation strings for networkmanager labels

**Systems:**
- No API changes
- No dependency changes
- Pure UI/UX refactoring with component extraction and reuse
