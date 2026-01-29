## Why

The NetworkManager plugin's `mod.rs` has severe maintainability issues: excessive coupling, deep nesting (multiple levels of indentation creating "mountain code"), and inconsistent component patterns. The file mixes UI component creation, business logic, and D-Bus interactions in a single 775-line monolith. This makes the code difficult to read, test, and modify. Refactoring now prevents further technical debt accumulation and establishes consistent patterns for network adapter management.

## What Changes

- Extract adapter-specific UI logic into dedicated widget modules:
  - **NEW**: `WiredAdapterWidget` - manages wired adapter UI lifecycle, connection details, and state synchronization
  - **NEW**: `WiFiAdapterWidget` - manages WiFi adapter UI lifecycle, network scanning, and connection handling
  - **NEW**: `VpnAdapterWidget` - manages VPN UI (if VPN support exists)
- Create consistent toggle components:
  - **NEW**: `WiredToggleWidget` - simple presentational component for wired adapter toggles (matching existing `WiFiToggleWidget` pattern)
  - Evaluate if `WiFiToggleWidget` exists and needs refactoring for consistency
- Separate D-Bus interaction layer:
  - Move async D-Bus operations into focused helper functions or modules
  - Reduce nested callback chains through better async structure
- Simplify `mod.rs`:
  - Reduce to plugin coordinator role only (device discovery, widget registration)
  - Delegate adapter-specific logic to respective widget modules
  - Eliminate deep nesting through extraction and composition

## Capabilities

### New Capabilities
- `wired-adapter-widget`: Widget managing wired ethernet adapter UI, connection state, and user interactions
- `wifi-adapter-widget`: Widget managing WiFi adapter UI, network scanning, and connection workflows
- `network-adapter-separation`: Architectural pattern for separating adapter logic from plugin coordination

### Modified Capabilities
- `network-wired-ui`: Requirements remain the same, but implementation will use the new `WiredAdapterWidget` structure
- `nmrs-integration`: D-Bus interaction patterns will be reorganized but the nmrs integration requirements are unchanged

## Impact

**Files to be created:**
- `src/features/networkmanager/wired_adapter_widget.rs` - wired adapter widget
- `src/features/networkmanager/wired_toggle_widget.rs` - wired toggle component (if needed)
- `src/features/networkmanager/wifi_adapter_widget.rs` - WiFi adapter widget

**Files to be modified:**
- `src/features/networkmanager/mod.rs` - simplified to plugin coordinator role
- `src/features/networkmanager/ethernet_menu.rs` - may need adjustments for new architecture
- `src/features/networkmanager/wifi_menu.rs` - may need adjustments for new architecture
- `src/features/networkmanager/dbus.rs` - may extract additional helper functions

**No breaking changes** - this is an internal refactoring that maintains all existing functionality and public interfaces.
