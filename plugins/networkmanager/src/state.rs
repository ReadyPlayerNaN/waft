//! Network state types used throughout the networkmanager plugin.

/// VPN connection state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VpnState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

impl VpnState {
    /// Convert from NM ActiveConnection state code.
    pub fn from_active_state(code: u32) -> Self {
        match code {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Disconnecting,
            _ => Self::Disconnected,
        }
    }
}

/// Information about a visible WiFi access point.
#[derive(Debug, Clone)]
pub struct AccessPointInfo {
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
}

/// Per-adapter WiFi state.
#[derive(Debug, Clone)]
pub struct WiFiAdapterState {
    pub path: String,
    pub interface_name: String,
    pub enabled: bool,
    pub busy: bool,
    pub active_ssid: Option<String>,
    /// Known networks (have saved connection profiles).
    pub access_points: Vec<AccessPointInfo>,
    pub scanning: bool,
}

/// Cached IP configuration for a connected device.
#[derive(Debug, Clone)]
pub struct CachedIpConfig {
    pub address: String,
    pub prefix: u8,
    pub gateway: Option<String>,
}

/// A saved Ethernet connection profile.
#[derive(Debug, Clone)]
pub struct EthernetProfileInfo {
    pub path: String,
    pub uuid: String,
    pub name: String,
}

/// Per-adapter Ethernet state.
#[derive(Debug, Clone)]
pub struct EthernetAdapterState {
    pub path: String,
    pub interface_name: String,
    pub device_state: u32,
    pub ip_config: Option<CachedIpConfig>,
    /// UUID of the currently active connection, if any.
    pub active_connection_uuid: Option<String>,
    /// Available ethernet connection profiles.
    pub profiles: Vec<EthernetProfileInfo>,
}

impl EthernetAdapterState {
    pub fn is_connected(&self) -> bool {
        self.device_state == 100
    }

    pub fn is_enabled(&self) -> bool {
        self.device_state >= 20
    }
}

/// VPN connection profile with runtime state.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VpnConnectionInfo {
    pub path: String,
    pub uuid: String,
    pub name: String,
    pub state: VpnState,
    /// Active connection D-Bus path when connected/connecting.
    pub active_path: Option<String>,
}

/// A saved Bluetooth tethering connection profile.
#[derive(Debug, Clone)]
pub struct TetheringProfileInfo {
    pub path: String,
    pub uuid: String,
    pub name: String,
    /// Bluetooth device address from the NM connection's `bluetooth.bdaddr` setting.
    pub bdaddr: Option<String>,
}

/// Runtime state of a tethering connection.
#[derive(Debug, Clone)]
pub struct TetheringConnectionState {
    pub path: String,
    pub uuid: String,
    pub name: String,
    pub active: bool,
    /// Active connection D-Bus path when connected.
    pub active_path: Option<String>,
    /// Bluetooth device address (e.g. "14:3F:A6:40:FA:0B").
    pub bdaddr: Option<String>,
}

/// Tracked NM bluetooth device (for tethering availability).
#[derive(Debug, Clone)]
pub struct BluetoothDeviceInfo {
    pub path: String,
    pub device_state: u32,
}

impl BluetoothDeviceInfo {
    /// Device is usable for tethering (state >= 40 means preparing or connected).
    pub fn ready(&self) -> bool {
        self.device_state >= 40
    }
}

/// A paired BlueZ device tracked for tethering availability.
///
/// Unlike NM's bluetooth device state (which stays "disconnected" even when the
/// phone is physically connected via Bluetooth), BlueZ's `Device1.Connected`
/// property reflects the actual Bluetooth link state.
#[derive(Debug, Clone)]
pub struct BluezPairedDevice {
    pub path: String,
    pub connected: bool,
}

/// Aggregate network state for all adapters and VPN connections.
#[derive(Debug, Clone, Default)]
pub struct NmState {
    pub wifi_adapters: Vec<WiFiAdapterState>,
    pub ethernet_adapters: Vec<EthernetAdapterState>,
    pub vpn_connections: Vec<VpnConnectionInfo>,
    pub tethering_connections: Vec<TetheringConnectionState>,
    /// NM bluetooth devices — kept for DeviceAdded/DeviceRemoved lifecycle.
    pub bluetooth_devices: Vec<BluetoothDeviceInfo>,
    /// BlueZ paired devices — tethering is shown when at least one is connected.
    pub bluez_paired_devices: Vec<BluezPairedDevice>,
    /// Cached public IP address (shared across all adapters).
    pub public_ip: Option<String>,
}

impl NmState {
    /// Returns true if a BlueZ device matching a tethering profile is connected.
    ///
    /// Only devices that have a corresponding NM bluetooth tethering connection
    /// profile count — other paired devices (headphones, mice, etc.) are ignored.
    pub fn any_tethering_device_connected(&self) -> bool {
        self.tethering_connections.iter().any(|conn| {
            let Some(ref bdaddr) = conn.bdaddr else {
                return false;
            };
            // BlueZ device path ends with dev_XX_XX_XX_XX_XX_XX
            let path_suffix = format!("dev_{}", bdaddr.replace(':', "_"));
            self.bluez_paired_devices
                .iter()
                .any(|d| d.connected && d.path.ends_with(&path_suffix))
        })
    }
}
