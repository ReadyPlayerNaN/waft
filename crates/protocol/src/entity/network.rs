use serde::{Deserialize, Serialize};

/// Entity type identifier for network adapters.
pub const ADAPTER_ENTITY_TYPE: &str = "network-adapter";

/// Entity type identifier for VPN connections.
pub const VPN_ENTITY_TYPE: &str = "vpn";

/// A network adapter (wired or wireless).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkAdapter {
    pub name: String,
    pub active: bool,
    pub ip: Option<IpInfo>,
    pub kind: AdapterKind,
}

/// IP address information for a connected adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpInfo {
    pub address: String,
    pub prefix: u8,
    pub gateway: Option<String>,
}

/// The type of network adapter with type-specific data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AdapterKind {
    Wired {
        profiles: Vec<Profile>,
        current_profile: Option<String>,
    },
    Wireless {
        networks: Vec<Network>,
        known_networks: Vec<Network>,
        connected: Option<Network>,
    },
}

/// A saved network connection profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub id: String,
}

/// A wireless network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
}

/// A VPN connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vpn {
    pub name: String,
    pub state: VpnState,
}

/// VPN connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VpnState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_wired() {
        let adapter = NetworkAdapter {
            name: "enp0s31f6".to_string(),
            active: true,
            ip: Some(IpInfo {
                address: "192.168.1.100".to_string(),
                prefix: 24,
                gateway: Some("192.168.1.1".to_string()),
            }),
            kind: AdapterKind::Wired {
                profiles: vec![Profile {
                    name: "Home".to_string(),
                    id: "home-uuid".to_string(),
                }],
                current_profile: Some("Home".to_string()),
            },
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: NetworkAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn serde_roundtrip_wireless() {
        let adapter = NetworkAdapter {
            name: "wlan0".to_string(),
            active: true,
            ip: None,
            kind: AdapterKind::Wireless {
                networks: vec![Network {
                    ssid: "MyWiFi".to_string(),
                    strength: 75,
                    secure: true,
                }],
                known_networks: vec![],
                connected: Some(Network {
                    ssid: "MyWiFi".to_string(),
                    strength: 75,
                    secure: true,
                }),
            },
        };
        let json = serde_json::to_value(&adapter).unwrap();
        let decoded: NetworkAdapter = serde_json::from_value(json).unwrap();
        assert_eq!(adapter, decoded);
    }

    #[test]
    fn serde_roundtrip_vpn() {
        let vpn = Vpn {
            name: "Work VPN".to_string(),
            state: VpnState::Connected,
        };
        let json = serde_json::to_value(&vpn).unwrap();
        let decoded: Vpn = serde_json::from_value(json).unwrap();
        assert_eq!(vpn, decoded);
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
