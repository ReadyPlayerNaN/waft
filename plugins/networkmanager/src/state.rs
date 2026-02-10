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

/// Per-adapter Ethernet state.
#[derive(Debug, Clone)]
pub struct EthernetAdapterState {
    pub path: String,
    pub interface_name: String,
    pub device_state: u32,
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

/// Aggregate network state for all adapters and VPN connections.
#[derive(Debug, Clone, Default)]
pub struct NmState {
    pub wifi_adapters: Vec<WiFiAdapterState>,
    pub ethernet_adapters: Vec<EthernetAdapterState>,
    pub vpn_connections: Vec<VpnConnectionInfo>,
}
