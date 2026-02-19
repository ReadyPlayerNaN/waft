use serde::{Deserialize, Serialize};

/// Entity type identifier for network adapters.
pub const ADAPTER_ENTITY_TYPE: &str = "network-adapter";

/// Entity type identifier for WiFi networks (nested under wifi adapter).
pub const WIFI_NETWORK_ENTITY_TYPE: &str = "wifi-network";

/// Entity type identifier for Ethernet connections (nested under ethernet adapter).
pub const ETHERNET_CONNECTION_ENTITY_TYPE: &str = "ethernet-connection";

/// Entity type identifier for VPN connections.
pub const VPN_ENTITY_TYPE: &str = "vpn";

/// Entity type identifier for tethering connections (nested under tethering adapter).
pub const TETHERING_CONNECTION_ENTITY_TYPE: &str = "tethering-connection";

/// A network adapter (wired or wireless).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkAdapter {
    pub name: String,
    pub enabled: bool,
    pub connected: bool,
    pub ip: Option<IpInfo>,
    pub public_ip: Option<String>,
    pub kind: AdapterKind,
}

/// IP address information for a connected adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpInfo {
    pub address: String,
    pub prefix: u8,
    pub gateway: Option<String>,
}

/// The type of network adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterKind {
    Wired,
    Wireless,
    Tethering,
}

/// A WiFi network (child entity of wireless adapter).
///
/// URN: `networkmanager/network-adapter/{adapter}/wifi-network/{ssid}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WiFiNetwork {
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
    pub known: bool,
    pub connected: bool,
}

impl WiFiNetwork {
    /// Entity type identifier for WiFi networks.
    pub const ENTITY_TYPE: &str = WIFI_NETWORK_ENTITY_TYPE;
}

/// An Ethernet connection profile (child entity of ethernet adapter).
///
/// URN: `networkmanager/network-adapter/{adapter}/ethernet-connection/{uuid}`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthernetConnection {
    pub name: String,
    pub uuid: String,
    pub active: bool,
}

impl EthernetConnection {
    /// Entity type identifier for Ethernet connections.
    pub const ENTITY_TYPE: &str = ETHERNET_CONNECTION_ENTITY_TYPE;
}

/// A VPN connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vpn {
    pub name: String,
    pub state: VpnState,
    #[serde(default)]
    pub vpn_type: VpnType,
}

/// VPN technology type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VpnType {
    #[default]
    Vpn,
    Wireguard,
}

/// VPN connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VpnState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// A tethering connection profile (child entity of tethering adapter).
///
/// URN: `networkmanager/network-adapter/tethering/tethering-connection/{uuid}`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TetheringConnection {
    pub name: String,
    pub uuid: String,
    pub active: bool,
}

impl TetheringConnection {
    /// Entity type identifier for tethering connections.
    pub const ENTITY_TYPE: &str = TETHERING_CONNECTION_ENTITY_TYPE;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_wired() {
        let adapter = NetworkAdapter {
            name: "enp0s31f6".to_string(),
            enabled: true,
            connected: true,
            ip: Some(IpInfo {
                address: "192.168.1.100".to_string(),
                prefix: 24,
                gateway: Some("192.168.1.1".to_string()),
            }),
            public_ip: Some("203.0.113.42".to_string()),
            kind: AdapterKind::Wired,
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: NetworkAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn serde_roundtrip_wireless() {
        let adapter = NetworkAdapter {
            name: "wlan0".to_string(),
            enabled: true,
            connected: false,
            ip: None,
            public_ip: None,
            kind: AdapterKind::Wireless,
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: NetworkAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn serde_roundtrip_wifi_network() {
        let network = WiFiNetwork {
            ssid: "MyWiFi".to_string(),
            strength: 75,
            secure: true,
            known: true,
            connected: false,
        };
        let json = serde_json::to_value(&network).unwrap();
        let decoded: WiFiNetwork = serde_json::from_value(json).unwrap();
        assert_eq!(network, decoded);
    }

    #[test]
    fn serde_roundtrip_ethernet_connection() {
        let connection = EthernetConnection {
            name: "Home".to_string(),
            uuid: "abc-123".to_string(),
            active: true,
        };
        let json = serde_json::to_value(&connection).unwrap();
        let decoded: EthernetConnection = serde_json::from_value(json).unwrap();
        assert_eq!(connection, decoded);
    }

    #[test]
    fn serde_roundtrip_vpn() {
        let vpn = Vpn {
            name: "Work VPN".to_string(),
            state: VpnState::Connected,
            vpn_type: VpnType::Vpn,
        };
        let json = serde_json::to_value(&vpn).unwrap();
        let decoded: Vpn = serde_json::from_value(json).unwrap();
        assert_eq!(vpn, decoded);
    }

    #[test]
    fn serde_roundtrip_vpn_wireguard() {
        let vpn = Vpn {
            name: "WG Tunnel".to_string(),
            state: VpnState::Disconnected,
            vpn_type: VpnType::Wireguard,
        };
        let json = serde_json::to_value(&vpn).unwrap();
        let decoded: Vpn = serde_json::from_value(json).unwrap();
        assert_eq!(vpn, decoded);
    }

    #[test]
    fn serde_vpn_missing_type_defaults_to_vpn() {
        let json = serde_json::json!({
            "name": "Legacy VPN",
            "state": "Connected"
        });
        let decoded: Vpn = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.vpn_type, VpnType::Vpn);
    }

    #[test]
    fn serde_roundtrip_tethering_adapter() {
        let adapter = NetworkAdapter {
            name: "tethering".to_string(),
            enabled: true,
            connected: true,
            ip: None,
            public_ip: None,
            kind: AdapterKind::Tethering,
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: NetworkAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn serde_roundtrip_tethering_connection() {
        let conn = TetheringConnection {
            name: "Nokia 3310 Network".to_string(),
            uuid: "abc-123-def".to_string(),
            active: false,
        };
        let json = serde_json::to_value(&conn).unwrap();
        let decoded: TetheringConnection = serde_json::from_value(json).unwrap();
        assert_eq!(conn, decoded);
    }

    #[test]
    fn serde_roundtrip_vpn_states() {
        let states = [
            VpnState::Disconnected,
            VpnState::Connecting,
            VpnState::Connected,
            VpnState::Disconnecting,
        ];
        for state in states {
            let json = serde_json::to_value(state).unwrap();
            let decoded: VpnState = serde_json::from_value(json).unwrap();
            assert_eq!(state, decoded);
        }
    }
}
