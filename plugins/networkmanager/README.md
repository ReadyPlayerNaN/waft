# NetworkManager Plugin

Manages WiFi, Ethernet, VPN, and Bluetooth tethering connections via NetworkManager D-Bus interfaces. Monitors device state changes, connection events, and WiFi access points in real time.

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `network-adapter` | `networkmanager/network-adapter/{interface}` | WiFi, Ethernet, or tethering adapter with enabled/connected state |
| `wifi-network` | `networkmanager/network-adapter/{interface}/wifi-network/{ssid}` | Visible WiFi network with signal strength and security info |
| `ethernet-connection` | `networkmanager/network-adapter/{interface}/ethernet-connection/{uuid}` | Saved Ethernet connection profile |
| `vpn` | `networkmanager/vpn/{name}` | VPN connection with state (disconnected/connecting/connected/disconnecting) |
| `tethering-connection` | `networkmanager/network-adapter/tethering/tethering-connection/{uuid}` | Bluetooth tethering profile |

Tethering entities are only exposed when a BlueZ paired device matching a tethering profile is connected (or a tethering connection is already active).

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

| Action | Description |
|---|---|
| `connect` | Connect to this WiFi network (requires saved connection profile) |
| `disconnect` | Disconnect from this WiFi network |

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

## Configuration

```toml
[[plugins]]
id = "networkmanager"
```

No plugin-specific configuration options.

## Dependencies

- **NetworkManager** running with D-Bus service on system bus
- **BlueZ** for Bluetooth tethering device detection (optional, tethering features degrade gracefully)
