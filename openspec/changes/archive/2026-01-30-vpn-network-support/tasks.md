## 1. D-Bus Functions

- [x] 1.1 Add `get_vpn_connections()` function to list all configured VPN connection profiles from NetworkManager Settings
- [x] 1.2 Add `get_active_vpn_connection()` function to check if any VPN is currently active and get its state
- [x] 1.3 Add `activate_vpn_connection()` function to activate a VPN by connection path
- [x] 1.4 Add `deactivate_vpn_connection()` function to deactivate an active VPN connection
- [x] 1.5 Add `subscribe_vpn_state_changed()` function to subscribe to VPN connection state change signals

## 2. VPN Menu Widget

- [x] 2.1 Define `VpnMenuOutput` enum with `Connect(String)` and `Disconnect(String)` variants
- [x] 2.2 Create `VpnRow` struct with icon, name label, spinner, switch, and connection state tracking
- [x] 2.3 Implement `VpnRow::new()` with signal blocking pattern for switch state updates
- [x] 2.4 Implement `VpnRow::update_connection()` to update row state based on `VpnState`
- [x] 2.5 Create `VpnMenuWidget` struct with root box, rows hashmap, and output callback
- [x] 2.6 Implement `VpnMenuWidget::set_vpn_connections()` to add/update/remove VPN rows
- [x] 2.7 Implement `VpnMenuWidget::set_vpn_state()` to update individual VPN row state
- [x] 2.8 Implement `VpnMenuWidget::connect_output()` for registering output callback

## 3. VPN Toggle Widget

- [x] 3.1 Create `VpnToggleWidget` struct with inner pattern (toggle, output callback, expand callback)
- [x] 3.2 Implement `VpnToggleWidget::new()` with initial "VPN" title and disconnected state
- [x] 3.3 Implement `VpnToggleWidget::update_state()` to set title, details, and icon based on active VPN
- [x] 3.4 Implement `VpnToggleWidget::connect_output()` and `set_expand_callback()` methods
- [x] 3.5 Add icon selection logic (connected vs disconnected VPN icons)

## 4. VPN Widget (Adapter)

- [x] 4.1 Create `VpnWidget` struct with store, dbus handle, toggle, and menu components
- [x] 4.2 Implement `VpnWidget::new()` to initialize toggle and menu with initial VPN list
- [x] 4.3 Implement `VpnWidget::widget()` returning `Arc<WidgetFeatureToggle>` with ID "networkmanager:vpn"
- [x] 4.4 Implement toggle click handler: disconnect if connected, expand menu if disconnected
- [x] 4.5 Implement menu output handler: call D-Bus activate/deactivate based on menu events
- [x] 4.6 Implement expand callback: refresh VPN list when menu is opened
- [x] 4.7 Set up VPN state change subscription to update store and UI on state transitions

## 5. Plugin Integration

- [x] 5.1 Add `vpn_ui: Option<VpnWidget>` field to `NetworkManagerPlugin`
- [x] 5.2 Add VPN initialization in `init()`: fetch VPN connections, populate store with `SetVpnConnections`
- [x] 5.3 Add VPN widget creation in `create_elements()`: create and register VPN feature toggle
- [x] 5.4 Export `vpn_widget` module in `mod.rs`

## 6. Translations

- [x] 6.1 Add VPN translation keys to `locales/en-US/main.ftl`: vpn-title, vpn-connected, vpn-disconnected, vpn-connecting, vpn-disconnecting
- [x] 6.2 Add VPN translation keys to `locales/cs-CZ/main.ftl` with Czech translations

## 7. Testing

- [x] 7.1 Verify VPN toggle displays "VPN" when no VPN connected
- [x] 7.2 Verify VPN toggle displays VPN name when connected
- [x] 7.3 Verify clicking toggle while connected disconnects VPN
- [x] 7.4 Verify clicking toggle while disconnected expands menu
- [x] 7.5 Verify menu lists all configured VPNs with correct states
- [x] 7.6 Verify clicking menu row or switch initiates connect/disconnect
- [x] 7.7 Verify spinner shows during connecting/disconnecting states
- [x] 7.8 Verify real-time state updates when VPN connects/disconnects externally
