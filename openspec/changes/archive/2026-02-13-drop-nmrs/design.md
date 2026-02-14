# Drop nmrs in favor of pure D-Bus for NetworkManager

## Problem

The `nmrs` crate produces `!Send` futures, forcing the networkmanager plugin to spawn a dedicated OS thread with a single-threaded tokio runtime + `LocalSet` just for WiFi scanning. The rest of the plugin already uses pure `zbus` D-Bus calls. Removing `nmrs` eliminates the threading workaround and simplifies the architecture.

## nmrs usage surface (3 call sites)

1. `device_discovery.rs`: `nm.list_devices()` — enumerate devices at startup
2. `wifi.rs`: `nm.scan_networks()` + `nm.list_networks()` — WiFi scanning
3. `bin/waft-networkmanager-daemon.rs`: `nmrs::NetworkManager::new()` — client init, passed to scan thread

## Replacement strategy

### Device discovery

Replace `nmrs::NetworkManager::list_devices()` with D-Bus `GetDevices()` on `org.freedesktop.NetworkManager`. Returns `Vec<OwnedObjectPath>`. Read each device's properties via the existing `get_property()` helper. The existing `get_device_info_dbus()` already does per-device property reads — reuse it.

### WiFi scanning

Replace `nm.scan_networks()` with `RequestScan` on `org.freedesktop.NetworkManager.Device.Wireless` (per-adapter). Replace `nm.list_networks()` with `GetAllAccessPoints` on the same interface, then read each AP's `Ssid`, `Strength`, `Flags`, `WpaFlags`, `RsnFlags`. The `AccessPoint` struct in `lib.rs` already has these fields.

### Thread elimination

Remove the dedicated `nm-wifi-scan` OS thread + single-threaded runtime + `LocalSet`. The scan task takes `Connection` instead of `nmrs::NetworkManager` — all futures are `Send`. Spawn with `tokio::spawn` on the main runtime.

## File changes

| File | Change |
|------|--------|
| `Cargo.toml` | Remove `nmrs = "2.0"` |
| `dbus_property.rs` | Add `NM_WIRELESS_INTERFACE` constant |
| `device_discovery.rs` | Rewrite `discover_devices()` to use D-Bus `GetDevices()` + `get_device_info_dbus()` |
| `wifi.rs` | Rewrite `scan_and_list_known_networks()` to use D-Bus `RequestScan` + `GetAllAccessPoints` |
| `wifi_scan.rs` | Change signature: take `Connection` instead of `nmrs::NetworkManager` |
| `bin/waft-networkmanager-daemon.rs` | Remove nmrs init, remove dedicated thread, use `tokio::spawn` for scan task |

## What stays the same

- All action handlers (connect, disconnect, activate) — already pure zbus
- Signal monitoring — already pure zbus
- Ethernet, VPN, IP config modules — already pure zbus
- State types, entity building, all tests
