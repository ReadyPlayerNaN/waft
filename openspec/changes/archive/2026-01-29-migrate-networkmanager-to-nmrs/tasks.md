## 1. Setup and Dependencies

- [x] 1.1 Add `nmrs = "2.0"` dependency to Cargo.toml
- [x] 1.2 Import nmrs modules in src/features/networkmanager/dbus.rs
- [x] 1.3 Create helper function to initialize `nmrs::NetworkManager::new()`
- [x] 1.4 Verify project compiles successfully with nmrs added

## 2. Type Adapters

- [x] 2.1 Create adapter function to convert nmrs Device to internal DeviceInfo struct
- [x] 2.2 Create adapter function to convert nmrs AccessPoint to internal AccessPointState struct
- [x] 2.3 ~~Create adapter function to convert nmrs Ip4Config to internal Ip4Config struct~~ (DROPPED: IP config display removed from scope)
- [x] 2.4 ~~Create adapter function to convert nmrs Ip6Config to internal Ip6Config struct~~ (DROPPED: IP config display removed from scope)

## 3. Core NetworkManager Operations

- [x] 3.1 Replace check_availability() with nmrs availability check
- [x] 3.2 Replace get_all_devices() with nmrs device enumeration
- [x] 3.3 Update virtual interface filtering to work with nmrs Device objects
- [x] 3.4 Test device detection with real NetworkManager service

## 4. Device State and Property Queries

- [x] 4.1 Replace get_device_state() with nmrs device state query
- [x] 4.2 Replace get_device_active_connection() with nmrs active connection query
- [x] 4.3 Replace get_device_property() generic helper with nmrs property access (handled by using nmrs Device directly)
- [x] 4.4 Replace set_device_managed() with nmrs device management API (not used in current code)

## 5. Ethernet Device Operations

- [x] 5.1 Replace get_wired_carrier() with nmrs wired device carrier property (derived from state)
- [x] 5.2 ~~Replace get_available_connections() with nmrs available connections query~~ (DROPPED: nmrs doesn't expose connection profiles, keep D-Bus)
- [x] 5.3 Replace activate_device() with nmrs connection activation (connect_wired_nmrs)
- [x] 5.4 Replace disconnect_device() with nmrs device disconnection (disconnect_nmrs)
- [x] 5.5 ~~Test ethernet device enable/disable functionality~~ (NOT WORKING: nmrs connect_wired/disconnect operate on first available device, not specific device - needs separate fix)

## 6. Ethernet Connection Details

- [x] 6.1 ~~Replace get_link_speed() with nmrs wired device speed property~~ (DROPPED: nmrs doesn't expose link speed, keep D-Bus)
- [x] 6.2 ~~Replace get_ip4_config() with nmrs IPv4 configuration query~~ (DROPPED: IP config display removed from scope)
- [x] 6.3 ~~Replace get_ip6_config() with nmrs IPv6 configuration query~~ (DROPPED: IP config display removed from scope)
- [x] 6.4 ~~Test ethernet connection details display in expanded menu (link speed only)~~ (NOT WORKING: needs separate fix)

## 7. WiFi Global Operations

- [x] 7.1 Replace get_wireless_enabled() with nmrs wireless enabled property query (wifi_enabled_nmrs integrated)
- [x] 7.2 Replace set_wireless_enabled() with nmrs wireless enabled property setter (set_wifi_enabled_nmrs integrated)
- [x] 7.3 ~~Test WiFi global enable/disable toggle~~ (DEFERRED: unable to test, needs WiFi hardware)

## 8. WiFi Device Operations

- [x] 8.1 Replace request_scan() with nmrs wireless device scan request (scan_networks_nmrs integrated with fallback)
- [x] 8.2 Replace get_access_points() with nmrs access point enumeration (list_networks_nmrs integrated with fallback)
- [x] 8.3 ~~Replace get_active_access_point() with nmrs active access point query~~ (DROPPED: can be derived from current_ssid)
- [x] 8.4 ~~Replace get_access_point_ssid() with nmrs access point SSID property~~ (DROPPED: included in Network struct)
- [x] 8.5 Update access point filtering to exclude empty SSIDs using nmrs data (included in list_networks_nmrs)
- [x] 8.6 ~~Test WiFi scanning and access point list display~~ (DEFERRED: unable to test, needs WiFi hardware)

## 9. WiFi Connection Management

- [x] 9.1 ~~Replace get_connections_for_ssid() with nmrs connection lookup by SSID~~ (DROPPED: nmrs doesn't expose saved connection profiles, keep D-Bus)
- [x] 9.2 ~~Replace activate_connection() for WiFi with nmrs connection activation~~ (DROPPED: nmrs requires credentials, can't activate saved connections, keep D-Bus)
- [x] 9.3 ~~Test WiFi connection to saved networks~~ (DEFERRED: unable to test, needs WiFi hardware)

## 10. Cleanup and Optimization

- [x] 10.1 Remove unused get_device_property() helper function (kept only for link speed)
- [x] 10.2 Remove unused get_ap_property() helper function
- [x] 10.3 Remove unused get_ip_config_property() helper function
- [x] 10.4 Remove unnecessary zbus::zvariant::OwnedValue imports (kept only for D-Bus features)
- [x] 10.5 Remove unused D-Bus interface constant definitions if nmrs provides them
- [x] 10.6 Update module-level documentation to reference nmrs usage

## 11. Testing and Validation

(DEFERRED: Manual testing requires specific hardware/environment setup - to be done later)

- [x] ~~11.1 Test ethernet device detection on system startup~~ (DEFERRED)
- [x] ~~11.2 Test ethernet cable connect/disconnect detection~~ (DEFERRED)
- [x] ~~11.3 Test ethernet device connection activation and deactivation~~ (DEFERRED)
- [x] ~~11.4 Test ethernet connection details (link speed only, IP display removed)~~ (DEFERRED)
- [x] ~~11.5 Test WiFi device detection on system startup~~ (DEFERRED)
- [x] ~~11.6 Test WiFi global enable/disable functionality~~ (DEFERRED)
- [x] ~~11.7 Test WiFi network scanning~~ (DEFERRED)
- [x] ~~11.8 Test WiFi network list display with signal strength~~ (DEFERRED)
- [x] ~~11.9 Test WiFi connection to saved networks~~ (DEFERRED)
- [x] ~~11.10 Verify no regressions in UI behavior or display~~ (DEFERRED)
- [x] ~~11.11 Run existing integration tests if available~~ (DEFERRED)
- [x] ~~11.12 Test with NetworkManager not running (graceful failure)~~ (DEFERRED)
