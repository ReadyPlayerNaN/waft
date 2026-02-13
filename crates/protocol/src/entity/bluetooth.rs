use serde::{Deserialize, Serialize};

/// A Bluetooth adapter (e.g. hci0).
///
/// URN: `blueman/bluetooth-adapter/{adapter-id}`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothAdapter {
    pub name: String,
    pub powered: bool,
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
/// URN: `blueman/bluetooth-adapter/{adapter-id}/bluetooth-device/{mac-address}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BluetoothDevice {
    pub name: String,
    pub device_type: String,
    pub connection_state: ConnectionState,
    pub battery_percentage: Option<u8>,
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
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: BluetoothAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn device_serde_roundtrip() {
        let device = BluetoothDevice {
            name: "WH-1000XM4".to_string(),
            device_type: "audio-headphones".to_string(),
            connection_state: ConnectionState::Connected,
            battery_percentage: Some(85),
        };
        let json = serde_json::to_value(&device).unwrap();
        let decoded: BluetoothDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device, decoded);
    }

    #[test]
    fn device_without_battery() {
        let device = BluetoothDevice {
            name: "Wireless Mouse".to_string(),
            device_type: "input-mouse".to_string(),
            connection_state: ConnectionState::Disconnected,
            battery_percentage: None,
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
        };
        assert!(connected.connected());

        let disconnected = BluetoothDevice {
            name: "Mouse".to_string(),
            device_type: "input-mouse".to_string(),
            connection_state: ConnectionState::Disconnected,
            battery_percentage: None,
        };
        assert!(!disconnected.connected());

        let connecting = BluetoothDevice {
            name: "Keyboard".to_string(),
            device_type: "input-keyboard".to_string(),
            connection_state: ConnectionState::Connecting,
            battery_percentage: None,
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
        };
        assert!(connecting.transitioning());

        let disconnecting = BluetoothDevice {
            name: "Test".to_string(),
            device_type: "phone".to_string(),
            connection_state: ConnectionState::Disconnecting,
            battery_percentage: None,
        };
        assert!(disconnecting.transitioning());

        let connected = BluetoothDevice {
            name: "Test".to_string(),
            device_type: "phone".to_string(),
            connection_state: ConnectionState::Connected,
            battery_percentage: None,
        };
        assert!(!connected.transitioning());

        let disconnected = BluetoothDevice {
            name: "Test".to_string(),
            device_type: "phone".to_string(),
            connection_state: ConnectionState::Disconnected,
            battery_percentage: None,
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
