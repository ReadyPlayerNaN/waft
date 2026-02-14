## 1. Add NM_WIRELESS_INTERFACE constant

- [x] 1.1 Add `NM_WIRELESS_INTERFACE` constant to `dbus_property.rs`
- [x] 1.2 Verify build

## 2. Rewrite device discovery to pure D-Bus

- [x] 2.1 Replace `discover_devices` to take `&Connection` and call `GetDevices()` via zbus
- [x] 2.2 Reuse `get_device_info_dbus()` for each returned device path
- [x] 2.3 Verify build

## 3. Rewrite WiFi scanning to pure D-Bus

- [x] 3.1 Replace `scan_and_list_known_networks` to take `&Connection` and adapter paths
- [x] 3.2 Use `RequestScan` on each adapter's Wireless interface
- [x] 3.3 Use `GetAllAccessPoints` to list APs, read properties per AP
- [x] 3.4 Verify build

## 4. Update wifi_scan task signature

- [x] 4.1 Change `wifi_scan_task` to take `Connection` instead of `nmrs::NetworkManager`
- [x] 4.2 Read adapter paths from shared state
- [x] 4.3 Verify build

## 5. Rewire daemon binary

- [x] 5.1 Remove `nmrs::NetworkManager` creation from `NetworkManagerPlugin::new`
- [x] 5.2 Update `main()` to use `tokio::spawn` for scan task (no dedicated thread)
- [x] 5.3 Remove unused nmrs imports
- [x] 5.4 Verify build

## 6. Remove nmrs dependency

- [x] 6.1 Remove `nmrs = "2.0"` from `plugins/networkmanager/Cargo.toml`
- [x] 6.2 Verify full workspace builds
- [x] 6.3 Run all tests — all pass

## 7. Cleanup

- [x] 7.1 Remove completed nmrs TODO section
- [x] 7.2 Commit: `refactor(networkmanager): replace nmrs with pure D-Bus calls`
