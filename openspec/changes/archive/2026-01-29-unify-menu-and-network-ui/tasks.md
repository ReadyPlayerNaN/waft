# Implementation Tasks

## 1. Extract MenuItemWidget Component

- [x] 1.1 Create `src/ui/menu_item.rs` file with MenuItemWidget struct
- [x] 1.2 Implement `MenuItemWidget::new(child: impl IsA<gtk::Widget>, on_click: impl Fn())` API
- [x] 1.3 Add `menu-item` CSS class to the container widget
- [x] 1.4 Implement click event handling for the entire row area
- [x] 1.5 Add visual hover and active/pressed states for click feedback
- [x] 1.6 Export MenuItemWidget from `src/ui/mod.rs`
- [x] 1.7 Add unit tests for MenuItemWidget component

## 2. Migrate Bluetooth Menu to MenuItemWidget

- [x] 2.1 Update `src/features/bluetooth/device_menu.rs` to import MenuItemWidget
- [x] 2.2 Build content structure (icon, name label, spinner, switch) in gtk::Box
- [x] 2.3 Replace DeviceRow implementation with MenuItemWidget wrapper
- [x] 2.4 Implement click handler to invoke connect/disconnect action
- [x] 2.5 Ensure event propagation doesn't cause double-triggering with switch widget
- [ ] 2.6 Test bluetooth device discovery, connection, and disconnection flows
- [ ] 2.7 Verify hover and click visual feedback works correctly

## 3. Migrate WiFi Menu to MenuItemWidget

- [x] 3.1 Update WiFi menu widget implementation to import MenuItemWidget
- [x] 3.2 Build network item content structure (icon, SSID label, signal strength, lock icon)
- [x] 3.3 Replace current row implementation with MenuItemWidget wrapper
- [x] 3.4 Implement click handler for network selection action
- [ ] 3.5 Test WiFi network scanning, selection, and connection
- [ ] 3.6 Verify all WiFi menu interactions work correctly

## 4. Add i18n Translation Keys for Network Status

- [x] 4.1 Add `"network-disabled"` translation key to translations.json for all languages
- [x] 4.2 Add `"network-connected"` translation key to translations.json
- [x] 4.3 Add `"network-disconnected"` translation key to translations.json
- [x] 4.4 Add translation keys for connection detail labels (IP, mask, gateway, link speed)
- [x] 4.5 Verify translation system can resolve all new keys

## 5. Create EthernetMenuWidget for Connection Details

- [x] 5.1 Create `src/features/networkmanager/ethernet_menu.rs` file
- [x] 5.2 Implement EthernetMenuWidget struct and initialization
- [x] 5.3 Create vertical layout for label-value pairs (link speed, IPv4, IPv6, mask, gateway)
- [x] 5.4 Implement NetworkManager DBus query for IP4Config (IPv4 address, subnet mask, gateway)
- [x] 5.5 Implement NetworkManager DBus query for IP6Config (IPv6 address, gateway)
- [x] 5.6 Implement NetworkManager DBus query for device link speed property
- [x] 5.7 Add logic to show/hide fields based on availability (only show if data exists)
- [x] 5.8 Implement connection details refresh on connection state change
- [x] 5.9 Implement connection details clearing on disconnection
- [x] 5.10 Add empty state or "Not connected" message when no active connection

## 6. Replace EthernetToggleWidget with FeatureToggleExpandableWidget

- [x] 6.1 Update `src/features/networkmanager/mod.rs` to remove EthernetToggleWidget import
- [x] 6.2 Import FeatureToggleExpandableWidget component
- [x] 6.3 Iterate over all wired ethernet devices (like WiFi adapter iteration)
- [x] 6.4 Create one FeatureToggleExpandableWidget per wired adapter
- [x] 6.5 Set title to "Wired ({interface_name})" for each adapter
- [x] 6.6 Implement connection status display logic (Connected/Disconnected/Disabled)
- [x] 6.7 Replace hardcoded "Disabled" string with `i18n::t("network-disabled")`
- [x] 6.8 Set icon based on connection state (network-wired-symbolic, network-wired-disconnected-symbolic, network-wired-offline-symbolic)
- [x] 6.9 Set details text based on connection state (connection name, translated status)
- [x] 6.10 Wire expand callback to show/hide EthernetMenuWidget
- [x] 6.11 Pass EthernetMenuWidget as expandable content to FeatureToggleExpandableWidget
- [x] 6.12 Remove `src/features/networkmanager/ethernet_toggle.rs` file (no longer needed)

## 7. Test Complete Wired Network Functionality

- [ ] 7.1 Test single wired adapter detection and display
- [ ] 7.2 Test multiple wired adapters (built-in + USB ethernet) display
- [ ] 7.3 Test connection status updates (cable plugged/unplugged)
- [ ] 7.4 Test enable/disable wired adapter toggle
- [ ] 7.5 Test expand/collapse wired toggle menu
- [ ] 7.6 Test connection details display when connected (link speed, IP, mask, gateway)
- [ ] 7.7 Test connection details hide when disconnected
- [ ] 7.8 Test IPv4 and IPv6 address display (if available)
- [ ] 7.9 Verify all status labels display translated text
- [ ] 7.10 Verify icons update correctly based on connection state

## 8. Integration Testing and Cleanup

- [ ] 8.1 Test MenuItemWidget usage across bluetooth, wifi, and wired menus
- [ ] 8.2 Verify consistent styling and behavior across all menu items
- [ ] 8.3 Test click interactions don't interfere with right-side widget controls
- [ ] 8.4 Verify hover and active states work consistently
- [ ] 8.5 Test all plugins together (bluetooth, wifi, wired) for any conflicts
- [ ] 8.6 Review and remove any unused imports or dead code
- [ ] 8.7 Run full test suite to ensure no regressions
- [ ] 8.8 Verify UI responsiveness and performance with connection details queries
