## 1. Create WiredToggleWidget (presentational component)

- [x] 1.1 Create `src/features/networkmanager/wired_toggle_widget.rs`
- [x] 1.2 Define `WiredToggleWidget` struct with Rc<WiredToggleWidgetInner> pattern
- [x] 1.3 Implement `new(interface_name: String)` constructor
- [x] 1.4 Create GTK UI: Box with icon, labels (title + details), and switch
- [x] 1.5 Implement `widget()` method returning gtk::Widget
- [x] 1.6 Implement `set_enabled(bool)` to update state and UI
- [x] 1.7 Implement `set_carrier(bool)` to update state and UI
- [x] 1.8 Implement `set_device_state(u32)` to update state and UI
- [x] 1.9 Implement private `get_signal_icon()` returning icon name based on state
- [x] 1.10 Implement private `update_ui()` to sync UI with internal state
- [x] 1.11 Add to `src/features/networkmanager/mod.rs` with `mod wired_toggle_widget;`
- [x] 1.12 Mark as `#[allow(dead_code)]` since it's unused for now

## 2. Create WiredAdapterWidget (coordinator widget)

- [x] 2.1 Create `src/features/networkmanager/wired_adapter_widget.rs`
- [x] 2.2 Define `WiredAdapterWidget` struct with path, store, nm, dbus, toggle, menu, expand_callback fields
- [x] 2.3 Implement `new()` constructor taking adapter state, store, nm, dbus, menu_store
- [x] 2.4 In `new()`: create FeatureToggleExpandableWidget with initial state
- [x] 2.5 In `new()`: create EthernetMenuWidget
- [x] 2.6 In `new()`: create expand_callback Rc<RefCell<Option<Box<dyn Fn(bool)>>>>
- [x] 2.7 In `new()`: call `setup_toggle_handlers()`
- [x] 2.8 In `new()`: call `setup_expand_callback()`
- [x] 2.9 Implement `widget()` method returning Arc<WidgetFeatureToggle>
- [x] 2.10 Implement `setup_toggle_handlers()` connecting toggle output to `handle_toggle_event()`
- [x] 2.11 Implement `handle_toggle_event()` with Activate/Deactivate match arms
- [x] 2.12 In `handle_toggle_event()`: spawn thread with tokio runtime for D-Bus operations
- [x] 2.13 In `handle_toggle_event()`: call `dbus::connect_wired_nmrs()` or `dbus::disconnect_nmrs()`
- [x] 2.14 In `handle_toggle_event()`: poll results with glib::timeout_add_local
- [x] 2.15 Implement `setup_expand_callback()` creating closure that calls `handle_expand()`
- [x] 2.16 Implement `handle_expand()` fetching connection details when expanded=true
- [x] 2.17 In `handle_expand()`: spawn thread to call `dbus::get_link_speed()`
- [x] 2.18 In `handle_expand()`: update EthernetMenuWidget with ConnectionDetails
- [x] 2.19 In `handle_expand()`: clear menu when expanded=false
- [x] 2.20 Implement `sync_state()` method updating toggle properties from EthernetAdapterState
- [x] 2.21 In `sync_state()`: update toggle active, icon, and details based on state
- [x] 2.22 Add to `src/features/networkmanager/mod.rs` with `mod wired_adapter_widget;`

## 3. Create WiFiAdapterWidget (coordinator widget)

- [x] 3.1 Create `src/features/networkmanager/wifi_adapter_widget.rs`
- [x] 3.2 Define `WiFiAdapterWidget` struct with path, store, nm, dbus, toggle, menu, expand_callback fields
- [x] 3.3 Implement `new()` constructor taking adapter state, store, nm, dbus, menu_store
- [x] 3.4 In `new()`: create FeatureToggleExpandableWidget with initial state
- [x] 3.5 In `new()`: create WiFiMenuWidget
- [x] 3.6 In `new()`: create expand_callback Rc<RefCell<Option<Box<dyn Fn(bool)>>>>
- [x] 3.7 In `new()`: call `setup_toggle_handlers()`
- [x] 3.8 In `new()`: call `setup_expand_callback()`
- [x] 3.9 In `new()`: call `setup_menu_handlers()`
- [x] 3.10 Implement `widget()` method returning Arc<WidgetFeatureToggle>
- [x] 3.11 Implement `setup_toggle_handlers()` connecting toggle output to `handle_toggle_event()`
- [x] 3.12 Implement `handle_toggle_event()` with Activate/Deactivate match arms
- [x] 3.13 In `handle_toggle_event()`: emit SetWiFiBusy(true) to store
- [x] 3.14 In `handle_toggle_event()`: spawn thread calling `dbus::set_wifi_enabled_nmrs()`
- [x] 3.15 In `handle_toggle_event()`: poll results and emit SetWiFiEnabled + SetWiFiBusy(false)
- [x] 3.16 Implement `setup_expand_callback()` creating closure that calls `handle_expand()`
- [x] 3.17 Implement `handle_expand()` triggering auto-scan when expanded=true
- [x] 3.18 In `handle_expand()`: spawn thread calling `dbus::scan_networks_nmrs()`
- [x] 3.19 In `handle_expand()`: wait 3 seconds, then call `dbus::list_networks_nmrs()`
- [x] 3.20 In `handle_expand()`: filter networks with saved profiles using `dbus::get_connections_for_ssid()`
- [x] 3.21 In `handle_expand()`: deduplicate by SSID keeping strongest signal
- [x] 3.22 In `handle_expand()`: emit SetWiFiAccessPoints to store
- [x] 3.23 In `handle_expand()`: update WiFiMenuWidget with networks and active SSID
- [x] 3.24 Implement `setup_menu_handlers()` connecting menu output to `handle_menu_event()`
- [x] 3.25 Implement `handle_menu_event()` with Connect/Disconnect match arms
- [x] 3.26 In `handle_menu_event()`: set network as "connecting" in menu
- [x] 3.27 In `handle_menu_event()`: spawn thread calling `dbus::get_connections_for_ssid()`
- [x] 3.28 In `handle_menu_event()`: call `dbus::activate_connection()` with connection path
- [x] 3.29 In `handle_menu_event()`: emit SetActiveWiFiConnection on success
- [x] 3.30 In `handle_menu_event()`: update toggle details and menu, clear connecting state
- [x] 3.31 Implement `sync_state()` method updating toggle properties from WiFiAdapterState
- [x] 3.32 In `sync_state()`: update toggle enabled, busy, details based on state
- [x] 3.33 In `sync_state()`: update menu with access points and active SSID
- [x] 3.34 Add to `src/features/networkmanager/mod.rs` with `mod wifi_adapter_widget;`

## 4. Refactor mod.rs to use adapter widgets

- [x] 4.1 Import `WiredAdapterWidget` and `WiFiAdapterWidget` in mod.rs
- [x] 4.2 Replace `EthernetAdapterUI` struct with HashMap<String, WiredAdapterWidget>
- [x] 4.3 Replace `WiFiAdapterUI` struct with HashMap<String, WiFiAdapterWidget>
- [x] 4.4 In `create_elements()`: iterate over ethernet_adapters from state
- [x] 4.5 For each ethernet adapter: create WiredAdapterWidget instance
- [x] 4.6 For each ethernet adapter: insert widget into ethernet_uis HashMap
- [x] 4.7 Remove all inline ethernet toggle event handler code from mod.rs
- [x] 4.8 Remove all inline ethernet expand callback code from mod.rs
- [x] 4.9 In `create_elements()`: iterate over wifi_adapters from state
- [x] 4.10 For each WiFi adapter: create WiFiAdapterWidget instance
- [x] 4.11 For each WiFi adapter: insert widget into wifi_uis HashMap
- [x] 4.12 Remove all inline WiFi toggle event handler code from mod.rs
- [x] 4.13 Remove all inline WiFi menu event handler code from mod.rs
- [x] 4.14 Remove all inline WiFi expand callback code from mod.rs
- [x] 4.15 Update `get_feature_toggles()` to call widget() on adapter widgets
- [x] 4.16 Remove EthernetAdapterUI and WiFiAdapterUI struct definitions

## 5. Cleanup and verification

- [x] 5.1 Run `cargo check` to verify compilation
- [x] 5.2 Fix any compilation errors or warnings
- [x] 5.3 Verify mod.rs is under 200 lines
- [ ] 5.4 Run the application and test wired adapter toggle (enable/disable) - USER TESTING REQUIRED
- [ ] 5.5 Test wired adapter menu expansion and connection details display - USER TESTING REQUIRED
- [ ] 5.6 Run the application and test WiFi adapter toggle (enable/disable) - USER TESTING REQUIRED
- [ ] 5.7 Test WiFi adapter menu expansion and network scanning - USER TESTING REQUIRED
- [ ] 5.8 Test WiFi network connection from menu - USER TESTING REQUIRED
- [ ] 5.9 Verify all state synchronization works (toggle updates from store) - USER TESTING REQUIRED
- [ ] 5.10 Verify no regressions in existing functionality - USER TESTING REQUIRED
- [x] 5.11 Check for any remaining deeply nested code (>3 levels)
- [x] 5.12 Check that methods are under 50 lines and have single responsibility
