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

/// A Bluetooth device paired or visible to an adapter.
///
/// URN: `blueman/bluetooth-adapter/{adapter-id}/bluetooth-device/{mac-address}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BluetoothDevice {
    pub name: String,
    pub device_type: String,
    pub connected: bool,
    pub battery_percentage: Option<u8>,
}

impl BluetoothDevice {
    /// Entity type identifier for Bluetooth devices.
    pub const ENTITY_TYPE: &str = "bluetooth-device";
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
            connected: true,
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
            connected: false,
            battery_percentage: None,
        };
        let json = serde_json::to_value(&device).unwrap();
        let decoded: BluetoothDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device, decoded);
    }
}
