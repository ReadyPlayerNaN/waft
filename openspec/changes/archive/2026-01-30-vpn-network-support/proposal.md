## Why

The networkmanager plugin has placeholder components for VPN (`vpn_menu.rs`, `vpn_toggle.rs`) that are currently empty. Users need a way to connect to and manage their configured VPN connections through the same panel interface used for WiFi and wired networks.

## What Changes

- Implement `VpnToggleWidget` as an expandable feature toggle displaying "VPN" when disconnected or the VPN name when connected
- Implement `VpnMenuWidget` following the Bluetooth device menu pattern - name on left, switch on right, full row clickable
- Add `VpnWidget` adapter component (similar to `WiredAdapterWidget`) to coordinate toggle, menu, and D-Bus operations
- Add D-Bus functions to list configured VPN connections, activate VPN, deactivate VPN, and subscribe to VPN state changes
- Register VPN widget in the plugin's `create_elements()` lifecycle
- Add translation keys for VPN-related labels and states

## Capabilities

### New Capabilities
- `vpn-widget`: VPN toggle and menu widget implementation following adapter widget patterns

### Modified Capabilities
- None (store already has VPN state management via `NetworkOp::SetVpnConnections` and `NetworkOp::SetVpnState`)

## Impact

- **Files to create/modify:**
  - `src/features/networkmanager/vpn_toggle.rs` - Replace placeholder with full implementation
  - `src/features/networkmanager/vpn_menu.rs` - Replace placeholder with full implementation
  - `src/features/networkmanager/vpn_widget.rs` - New adapter widget (like `wired_adapter_widget.rs`)
  - `src/features/networkmanager/dbus.rs` - Add VPN D-Bus functions
  - `src/features/networkmanager/mod.rs` - Register VPN widget, add VPN UI tracking
  - `locales/en-US/main.ftl` - Add VPN translation keys
  - `locales/cs-CZ/main.ftl` - Add VPN translation keys (Czech)

- **D-Bus operations needed:**
  - List VPN connections from NetworkManager Settings
  - Get active VPN connection state
  - Activate VPN connection
  - Deactivate VPN connection
  - Subscribe to VPN state change signals

- **No breaking changes** - Adds new functionality without modifying existing behavior
