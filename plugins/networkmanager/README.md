# NetworkManager Plugin

Manages WiFi, Ethernet, VPN, and Bluetooth tethering connections via NetworkManager D-Bus interfaces. Monitors device state changes, connection events, and WiFi access points in real time.

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `network-adapter` | `networkmanager/network-adapter/{interface}` | WiFi, Ethernet, or tethering adapter with enabled/connected state |
| `wifi-network` | `networkmanager/network-adapter/{interface}/wifi-network/{ssid}` | Visible WiFi network with signal strength, security, and connection settings |
| `ethernet-connection` | `networkmanager/network-adapter/{interface}/ethernet-connection/{uuid}` | Saved Ethernet connection profile |
| `vpn` | `networkmanager/vpn/{name}` | VPN connection with state (disconnected/connecting/connected/disconnecting) |
| `tethering-connection` | `networkmanager/network-adapter/tethering/tethering-connection/{uuid}` | Bluetooth tethering profile |

Tethering entities are only exposed when a BlueZ paired device matching a tethering profile is connected (or a tethering connection is already active).

### `wifi-network` Entity Fields

| Field | Type | Description |
|---|---|---|
| `ssid` | `String` | Network name |
| `strength` | `u8` | Signal strength (0-100) |
| `secure` | `bool` | Whether the network requires authentication |
| `known` | `bool` | Whether a saved connection profile exists |
| `connected` | `bool` | Whether currently connected to this network |
| `security_type` | `SecurityType` | `Open`, `Wep`, `Wpa`, `Wpa2`, `Wpa3`, or `Enterprise` |
| `connecting` | `bool` | Whether a connection attempt is in progress |
| `autoconnect` | `Option<bool>` | Auto-connect setting from NM profile (known networks only) |
| `metered` | `Option<MeteredState>` | Metered connection state: `Unknown`, `Yes`, `No`, `GuessYes`, `GuessNo` |
| `dns_servers` | `Option<Vec<String>>` | Configured DNS servers from NM profile |
| `ip_method` | `Option<IpMethod>` | IP configuration method: `Auto`, `Manual`, `LinkLocal`, `Disabled` |

The optional settings fields (`autoconnect`, `metered`, `dns_servers`, `ip_method`) are populated only for known networks by reading the NM connection profile via `GetSettings` D-Bus call.

## Actions

### network-adapter

| Action | Params | Description |
|---|---|---|
| `activate` | | Enable WiFi, connect Ethernet, or activate first available tethering profile |
| `deactivate` | | Disable WiFi, disconnect Ethernet, or disconnect all tethering |
| `scan` | | Trigger a WiFi access point scan |
| `connect` | `ssid: string` | Connect to a specific WiFi network |
| `disconnect` | | Disconnect the WiFi adapter |

### wifi-network

| Action | Params | Description |
|---|---|---|
| `connect` | | Connect to this WiFi network (requires saved connection profile) |
| `disconnect` | | Disconnect from this WiFi network |
| `forget` | | Delete saved connection profile(s) for this network. Disconnects first if currently connected. |
| `update-settings` | `{ "autoconnect": bool, "metered": i64, "ip_method": "auto", "dns_servers": ["8.8.8.8"] }` | Update connection profile settings. All fields optional. |
| `share` | | Returns WiFi QR code string in `ActionSuccess.data` as `{ "qr_string": "WIFI:T:WPA;S:...;P:...;;" }`. Retrieves PSK via NM `GetSecrets`. |

### ethernet-connection

| Action | Description |
|---|---|
| `activate` | Activate this Ethernet connection profile |
| `deactivate` | Deactivate this Ethernet connection |

### vpn

| Action | Description |
|---|---|
| `connect` | Activate the VPN connection |
| `disconnect` | Deactivate the VPN connection |

### tethering-connection

| Action | Description |
|---|---|
| `connect` | Activate this Bluetooth tethering profile |
| `disconnect` | Deactivate this Bluetooth tethering connection |

## D-Bus Interfaces

| Bus | Service | Path | Usage |
|---|---|---|---|
| System | `org.freedesktop.NetworkManager` | `/org/freedesktop/NetworkManager` | Device discovery, WiFi enable/disable, connection activation |
| System | `org.freedesktop.NetworkManager` | `/org/freedesktop/NetworkManager/Settings` | Connection profile discovery (VPN, Ethernet, tethering) |
| System | `org.bluez` | `/org/bluez/*` | Paired device discovery, connection state monitoring for tethering visibility |

## How It Works

1. **Device discovery**: Enumerates NM devices, filters virtual interfaces (docker, veth, br-, virbr, vnet)
2. **NM signal monitor**: Watches `StateChanged`, `PropertiesChanged`, `AccessPointAdded/Removed` signals on the system bus
3. **BlueZ signal monitor**: Uses a dedicated system bus connection to track paired device connection state (determines tethering visibility)
4. **WiFi scan task**: Background task that performs WiFi scans on demand via D-Bus
5. **IP config**: Reads IPv4 configuration from connected devices, fetches public IP via HTTP

## WiFi Features

### Forget Network

The `forget` action deletes all saved NM connection profiles matching the network's SSID. If the network is currently connected, it is disconnected first. After deletion, the network's `known` flag becomes `false` and settings fields are cleared.

### Per-Network Settings

Known WiFi networks expose connection profile settings (autoconnect, metered, DNS, IP method) via the entity. These are read from NM at scan time and can be modified via the `update-settings` action, which reads the current profile, applies changes, and calls NM's `Update` D-Bus method.

### QR Code Sharing

The `share` action retrieves the WiFi password via NM's `GetSecrets` D-Bus method and returns a WiFi QR code string in the standard `WIFI:T:<security>;S:<ssid>;P:<password>;;` format. Special characters in SSID and password are backslash-escaped per the Wi-Fi QR spec. The QR string is returned in `ActionSuccess.data` rather than stored on the entity, avoiding sensitive data flowing through routine entity updates.

## Configuration

```toml
[[plugins]]
id = "networkmanager"
```

No plugin-specific configuration options.

## Dependencies

- **NetworkManager** running with D-Bus service on system bus
- **BlueZ** for Bluetooth tethering device detection (optional, tethering features degrade gracefully)
