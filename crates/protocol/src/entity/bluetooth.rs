use serde::{Deserialize, Serialize};

/// A Bluetooth adapter (e.g. hci0).
///
/// URN: `bluez/bluetooth-adapter/{adapter-id}`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothAdapter {
    pub name: String,
    pub powered: bool,
    #[serde(default)]
    pub discoverable: bool,
    #[serde(default)]
    pub discovering: bool,
}

impl BluetoothAdapter {
    /// Entity type identifier for Bluetooth adapters.
    pub const ENTITY_TYPE: &str = "bluetooth-adapter";
}

/// Connection lifecycle state for a Bluetooth device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// A Bluetooth device paired or visible to an adapter.
///
/// URN: `bluez/bluetooth-adapter/{adapter-id}/bluetooth-device/{mac-address}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BluetoothDevice {
    pub name: String,
    pub device_type: String,
    pub connection_state: ConnectionState,
    pub battery_percentage: Option<u8>,
    #[serde(default)]
    pub paired: bool,
    #[serde(default)]
    pub trusted: bool,
    #[serde(default)]
    pub rssi: Option<i16>,
}

impl BluetoothDevice {
    /// Entity type identifier for Bluetooth devices.
    pub const ENTITY_TYPE: &str = "bluetooth-device";

    /// Whether this device is currently connected.
    pub fn connected(&self) -> bool {
        self.connection_state == ConnectionState::Connected
    }

    /// Whether this device is in a transitional state (connecting or disconnecting).
    pub fn transitioning(&self) -> bool {
        matches!(
            self.connection_state,
            ConnectionState::Connecting | ConnectionState::Disconnecting
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_serde_roundtrip() {
        let adapter = BluetoothAdapter {
            name: "hci0".to_string(),
            powered: true,
            discoverable: false,
            discovering: false,
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: BluetoothAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn adapter_serde_roundtrip_with_new_fields() {
        let adapter = BluetoothAdapter {
            name: "hci0".to_string(),
            powered: true,
            discoverable: true,
            discovering: true,
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: BluetoothAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn adapter_backwards_compat_without_new_fields() {
        let json = serde_json::json!({
            "name": "hci0",
            "powered": true
        });
        let adapter: BluetoothAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter.name, "hci0");
        assert!(adapter.powered);
        assert!(!adapter.discoverable);
        assert!(!adapter.discovering);
    }

    #[test]
    fn device_serde_roundtrip() {
        let device = BluetoothDevice {
            name: "WH-1000XM4".to_string(),
            device_type: "audio-headphones".to_string(),
            connection_state: ConnectionState::Connected,
            battery_percentage: Some(85),
            paired: false,
            trusted: false,
            rssi: None,
        };
        let json = serde_json::to_value(&device).unwrap();
        let decoded: BluetoothDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device, decoded);
    }

    #[test]
    fn device_serde_roundtrip_with_new_fields() {
        let device = BluetoothDevice {
            name: "WH-1000XM4".to_string(),
            device_type: "audio-headphones".to_string(),
            connection_state: ConnectionState::Connected,
            battery_percentage: Some(85),
            paired: true,
            trusted: true,
            rssi: Some(-42),
        };
        let json = serde_json::to_value(&device).unwrap();
        let decoded: BluetoothDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device, decoded);
    }

    #[test]
    fn device_backwards_compat_without_new_fields() {
        let json = serde_json::json!({
            "name": "WH-1000XM4",
            "device_type": "audio-headphones",
            "connection_state": "Connected",
            "battery_percentage": 85
        });
        let device: BluetoothDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device.name, "WH-1000XM4");
        assert_eq!(device.device_type, "audio-headphones");
        assert_eq!(device.connection_state, ConnectionState::Connected);
        assert_eq!(device.battery_percentage, Some(85));
        assert!(!device.paired);
        assert!(!device.trusted);
        assert_eq!(device.rssi, None);
    }

    #[test]
    fn device_without_battery() {
        let device = BluetoothDevice {
            name: "Wireless Mouse".to_string(),
            device_type: "input-mouse".to_string(),
            connection_state: ConnectionState::Disconnected,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        let json = serde_json::to_value(&device).unwrap();
        let decoded: BluetoothDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device, decoded);
    }

    #[test]
    fn device_connected_method() {
        let connected = BluetoothDevice {
            name: "Headphones".to_string(),
            device_type: "audio-headphones".to_string(),
            connection_state: ConnectionState::Connected,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        assert!(connected.connected());

        let disconnected = BluetoothDevice {
            name: "Mouse".to_string(),
            device_type: "input-mouse".to_string(),
            connection_state: ConnectionState::Disconnected,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        assert!(!disconnected.connected());

        let connecting = BluetoothDevice {
            name: "Keyboard".to_string(),
            device_type: "input-keyboard".to_string(),
            connection_state: ConnectionState::Connecting,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        assert!(!connecting.connected());
    }

    #[test]
    fn device_transitioning_method() {
        let connecting = BluetoothDevice {
            name: "Test".to_string(),
            device_type: "phone".to_string(),
            connection_state: ConnectionState::Connecting,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        assert!(connecting.transitioning());

        let disconnecting = BluetoothDevice {
            name: "Test".to_string(),
            device_type: "phone".to_string(),
            connection_state: ConnectionState::Disconnecting,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        assert!(disconnecting.transitioning());

        let connected = BluetoothDevice {
            name: "Test".to_string(),
            device_type: "phone".to_string(),
            connection_state: ConnectionState::Connected,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        assert!(!connected.transitioning());

        let disconnected = BluetoothDevice {
            name: "Test".to_string(),
            device_type: "phone".to_string(),
            connection_state: ConnectionState::Disconnected,
            battery_percentage: None,
            paired: false,
            trusted: false,
            rssi: None,
        };
        assert!(!disconnected.transitioning());
    }

    #[test]
    fn connection_state_serde_all_variants() {
        for state in [
            ConnectionState::Disconnected,
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Disconnecting,
        ] {
            let json = serde_json::to_value(state).unwrap();
            let decoded: ConnectionState = serde_json::from_value(json).unwrap();
            assert_eq!(state, decoded);
        }
    }
}
