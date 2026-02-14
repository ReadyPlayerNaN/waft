# Bluetooth Plugin

Bluetooth adapter power toggle and paired device management via BlueZ.

## Purpose

Manages Bluetooth adapters and their paired devices. Exposes adapter power toggles and device connect/disconnect actions. Monitors BlueZ D-Bus signals for real-time state updates including connection state transitions (connecting, disconnecting).

## Entity Types

### `bluetooth-adapter`

One entity per Bluetooth adapter (e.g. `hci0`).

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Adapter alias or name |
| `powered` | `bool` | Whether the adapter is powered on |

#### Actions

| Action | Params | Description |
|--------|--------|-------------|
| `toggle-power` | none | Toggle adapter power on/off |

### `bluetooth-device`

One entity per paired device, nested under its adapter.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Device alias or name |
| `device_type` | `String` | BlueZ icon/type (e.g. `audio-headphones`, `input-keyboard`) |
| `connection_state` | `ConnectionState` | `Connected`, `Disconnected`, `Connecting`, `Disconnecting` |
| `battery_percentage` | `Option<u8>` | Device battery level if reported |

#### Actions

| Action | Params | Description |
|--------|--------|-------------|
| `toggle-connect` | none | Connect or disconnect the device |

### URN Format

```
bluez/bluetooth-adapter/{adapter-id}
bluez/bluetooth-adapter/{adapter-id}/bluetooth-device/{mac-address}
```

Examples:
```
bluez/bluetooth-adapter/hci0
bluez/bluetooth-adapter/hci0/bluetooth-device/AA:BB:CC:DD:EE:FF
```

## D-Bus Interfaces

| Bus | Destination | Path | Interface | Usage |
|-----|-------------|------|-----------|-------|
| System | `org.bluez` | `/` | `org.freedesktop.DBus.ObjectManager` | Enumerate adapters and devices |
| System | `org.bluez` | `/org/bluez/{adapter}` | `org.bluez.Adapter1` | Read/write adapter properties (Powered) |
| System | `org.bluez` | `/org/bluez/{adapter}/dev_{mac}` | `org.bluez.Device1` | Read device properties, Connect/Disconnect |
| System | `org.bluez` | (all paths) | `org.freedesktop.DBus.Properties` | PropertiesChanged signals for live updates |

## Dependencies

- **BlueZ** -- Linux Bluetooth stack (provides the `org.bluez` D-Bus service)

## Configuration

```toml
[[plugins]]
id = "bluez"
```

No plugin-specific configuration options. Only paired devices are shown.
