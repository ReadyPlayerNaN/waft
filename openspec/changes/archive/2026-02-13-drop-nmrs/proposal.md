## Why

The `nmrs` crate produces `!Send` futures, forcing the networkmanager plugin to spawn a dedicated OS thread with a single-threaded tokio runtime + `LocalSet` just for WiFi scanning. The rest of the plugin already uses pure `zbus` D-Bus calls. Removing `nmrs` eliminates the threading workaround and simplifies the architecture.

This reverses the earlier nmrs migration (2026-01-29-migrate-networkmanager-to-nmrs) after discovering the `!Send` limitation made the architecture worse, not better.

## What Changes

- **Remove** `nmrs = "2.0"` dependency from Cargo.toml
- **Replace** `nmrs::NetworkManager::list_devices()` with D-Bus `GetDevices()` via zbus
- **Replace** `nmrs` WiFi scanning with D-Bus `RequestScan` + `GetAllAccessPoints` via zbus
- **Remove** dedicated `nm-wifi-scan` OS thread + single-threaded runtime + `LocalSet`
- **Use** `tokio::spawn` for scan task on the main runtime (all futures are now `Send`)

## Capabilities

### Removed Capabilities
- `nmrs-integration`: Removed nmrs dependency entirely

### Restored Capabilities
- Pure zbus D-Bus calls for all NM operations (device discovery, WiFi scanning, connection management)
- All async futures are `Send`, eliminating need for dedicated thread

## Impact

**Code:**
- `plugins/networkmanager/Cargo.toml` - Remove nmrs dependency
- `plugins/networkmanager/src/dbus_property.rs` - Add `NM_WIRELESS_INTERFACE` constant
- `plugins/networkmanager/src/device_discovery.rs` - Rewrite to use D-Bus `GetDevices()`
- `plugins/networkmanager/src/wifi.rs` - Rewrite scanning with D-Bus `RequestScan` + `GetAllAccessPoints`
- `plugins/networkmanager/src/wifi_scan.rs` - Take `Connection` instead of `nmrs::NetworkManager`
- `plugins/networkmanager/bin/waft-networkmanager-daemon.rs` - Remove nmrs init, remove dedicated thread
