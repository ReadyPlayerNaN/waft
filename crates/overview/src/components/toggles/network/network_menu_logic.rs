//! Pure decision logic for WiFi toggle menus — extracted for testability.

use waft_protocol::entity::network::WiFiNetwork;
use waft_protocol::urn::Urn;

/// Returns true if the WiFi feature toggle should have an expandable menu.
///
/// The menu is expandable when:
/// - At least one wifi-network entity is available, OR
/// - waft-settings is running (so a "Open settings" link can be shown)
pub fn should_be_expandable(network_count: usize, has_settings: bool) -> bool {
    network_count > 0 || has_settings
}

/// Returns the details/subtitle text for the WiFi feature toggle.
///
/// Priority:
/// 1. If a network is currently connected, show its SSID.
/// 2. If multiple networks are visible, show the count.
/// 3. Otherwise return None.
pub fn details_text(networks: &[(Urn, WiFiNetwork)]) -> Option<String> {
    if let Some((_, net)) = networks.iter().find(|(_, n)| n.connected) {
        return Some(net.ssid.clone());
    }
    if !networks.is_empty() {
        return Some(format!("{} networks", networks.len()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity::network::WiFiNetwork;
    use waft_protocol::urn::Urn;

    fn make_urn(ssid: &str) -> Urn {
        Urn::new("networkmanager", "network-adapter", "wlan0")
            .child("wifi-network", ssid)
    }

    fn make_network(ssid: &str, connected: bool, known: bool) -> WiFiNetwork {
        WiFiNetwork {
            ssid: ssid.to_string(),
            strength: 80,
            secure: true,
            known,
            connected,
        }
    }

    // --- should_be_expandable ---

    #[test]
    fn not_expandable_when_no_networks_and_no_settings() {
        assert!(!should_be_expandable(0, false));
    }

    #[test]
    fn expandable_when_settings_available_even_with_no_networks() {
        // This is the critical regression guard:
        // the settings link must always appear when waft-settings is running
        assert!(should_be_expandable(0, true));
    }

    #[test]
    fn expandable_when_networks_present_without_settings() {
        assert!(should_be_expandable(1, false));
        assert!(should_be_expandable(3, false));
    }

    #[test]
    fn expandable_when_both_networks_and_settings() {
        assert!(should_be_expandable(2, true));
    }

    // --- details_text ---

    #[test]
    fn details_none_when_no_networks() {
        let networks: Vec<(Urn, WiFiNetwork)> = vec![];
        assert_eq!(details_text(&networks), None);
    }

    #[test]
    fn details_shows_connected_ssid() {
        let networks = vec![
            (make_urn("HomeNet"), make_network("HomeNet", true, true)),
        ];
        assert_eq!(details_text(&networks), Some("HomeNet".to_string()));
    }

    #[test]
    fn details_shows_connected_ssid_among_multiple_networks() {
        let networks = vec![
            (make_urn("Other"), make_network("Other", false, false)),
            (make_urn("HomeNet"), make_network("HomeNet", true, true)),
            (make_urn("Neighbor"), make_network("Neighbor", false, false)),
        ];
        assert_eq!(details_text(&networks), Some("HomeNet".to_string()));
    }

    #[test]
    fn details_shows_count_when_multiple_unconnected_networks() {
        let networks = vec![
            (make_urn("Net1"), make_network("Net1", false, false)),
            (make_urn("Net2"), make_network("Net2", false, false)),
        ];
        assert_eq!(details_text(&networks), Some("2 networks".to_string()));
    }

    #[test]
    fn details_shows_count_for_single_unconnected_network() {
        let networks = vec![
            (make_urn("SomeNet"), make_network("SomeNet", false, false)),
        ];
        assert_eq!(details_text(&networks), Some("1 networks".to_string()));
    }
}
