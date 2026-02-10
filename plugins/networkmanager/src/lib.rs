//! NetworkManager plugin shared helpers.
//!
//! Provides reusable utility functions for the networkmanager daemon.

/// Check if a network interface name is a virtual interface.
pub fn is_virtual_interface(name: &str) -> bool {
    let virtual_prefixes = ["docker", "veth", "br-", "virbr", "vnet"];
    virtual_prefixes
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Convert a CIDR prefix length to a dotted-decimal subnet mask.
pub fn prefix_to_subnet_mask(prefix: u32) -> String {
    if prefix == 0 {
        return "0.0.0.0".to_string();
    }
    if prefix > 32 {
        return "255.255.255.255".to_string();
    }
    let mask: u32 = !0u32 << (32 - prefix);
    format!(
        "{}.{}.{}.{}",
        (mask >> 24) & 0xFF,
        (mask >> 16) & 0xFF,
        (mask >> 8) & 0xFF,
        mask & 0xFF
    )
}

/// Get WiFi icon based on signal strength, enabled state, and connection status.
pub fn get_wifi_icon(strength: Option<u8>, enabled: bool, connected: bool) -> &'static str {
    if !enabled || !connected {
        return "network-wireless-symbolic";
    }
    match strength {
        Some(s) if s > 75 => "network-wireless-signal-excellent-symbolic",
        Some(s) if s > 50 => "network-wireless-signal-good-symbolic",
        Some(s) if s > 25 => "network-wireless-signal-ok-symbolic",
        Some(_) => "network-wireless-signal-weak-symbolic",
        None => "network-wireless-symbolic",
    }
}

/// WiFi access point information for security checks.
#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub path: String,
    pub ssid: String,
    pub strength: u8,
    pub flags: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}

impl AccessPoint {
    /// Returns true if the access point requires authentication.
    pub fn is_secure(&self) -> bool {
        self.flags != 0 || self.wpa_flags != 0 || self.rsn_flags != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Virtual interface detection tests

    #[test]
    fn test_virtual_interface_detection_docker() {
        assert!(is_virtual_interface("docker0"));
    }

    #[test]
    fn test_virtual_interface_detection_veth() {
        assert!(is_virtual_interface("veth1234"));
    }

    #[test]
    fn test_virtual_interface_detection_bridge() {
        assert!(is_virtual_interface("br-abcd"));
    }

    #[test]
    fn test_virtual_interface_detection_virbr() {
        assert!(is_virtual_interface("virbr0"));
    }

    #[test]
    fn test_virtual_interface_detection_vnet() {
        assert!(is_virtual_interface("vnet0"));
    }

    #[test]
    fn test_real_interface_ethernet() {
        assert!(!is_virtual_interface("eth0"));
    }

    #[test]
    fn test_real_interface_wlan() {
        assert!(!is_virtual_interface("wlan0"));
    }

    #[test]
    fn test_real_interface_usb() {
        assert!(!is_virtual_interface("usb0"));
    }

    #[test]
    fn test_real_interface_rndis() {
        assert!(!is_virtual_interface("rndis0"));
    }

    // Access point security tests

    #[test]
    fn test_access_point_is_secure_with_flags() {
        let ap = AccessPoint {
            path: "/test".to_string(),
            ssid: "TestAP".to_string(),
            strength: 80,
            flags: 1,
            wpa_flags: 0,
            rsn_flags: 0,
        };
        assert!(ap.is_secure());
    }

    #[test]
    fn test_access_point_is_secure_with_wpa_flags() {
        let ap = AccessPoint {
            path: "/test".to_string(),
            ssid: "TestAP".to_string(),
            strength: 80,
            flags: 0,
            wpa_flags: 1,
            rsn_flags: 0,
        };
        assert!(ap.is_secure());
    }

    #[test]
    fn test_access_point_is_secure_with_rsn_flags() {
        let ap = AccessPoint {
            path: "/test".to_string(),
            ssid: "TestAP".to_string(),
            strength: 80,
            flags: 0,
            wpa_flags: 0,
            rsn_flags: 1,
        };
        assert!(ap.is_secure());
    }

    #[test]
    fn test_access_point_is_not_secure_with_no_flags() {
        let ap = AccessPoint {
            path: "/test".to_string(),
            ssid: "TestAP".to_string(),
            strength: 80,
            flags: 0,
            wpa_flags: 0,
            rsn_flags: 0,
        };
        assert!(!ap.is_secure());
    }

    // Subnet mask conversion tests

    #[test]
    fn test_prefix_to_subnet_mask_24() {
        assert_eq!(prefix_to_subnet_mask(24), "255.255.255.0");
    }

    #[test]
    fn test_prefix_to_subnet_mask_16() {
        assert_eq!(prefix_to_subnet_mask(16), "255.255.0.0");
    }

    #[test]
    fn test_prefix_to_subnet_mask_8() {
        assert_eq!(prefix_to_subnet_mask(8), "255.0.0.0");
    }

    #[test]
    fn test_prefix_to_subnet_mask_32() {
        assert_eq!(prefix_to_subnet_mask(32), "255.255.255.255");
    }

    #[test]
    fn test_prefix_to_subnet_mask_0() {
        assert_eq!(prefix_to_subnet_mask(0), "0.0.0.0");
    }

    #[test]
    fn test_prefix_to_subnet_mask_greater_than_32() {
        assert_eq!(prefix_to_subnet_mask(33), "255.255.255.255");
        assert_eq!(prefix_to_subnet_mask(64), "255.255.255.255");
    }

    #[test]
    fn test_prefix_to_subnet_mask_common_values() {
        assert_eq!(prefix_to_subnet_mask(25), "255.255.255.128");
        assert_eq!(prefix_to_subnet_mask(26), "255.255.255.192");
        assert_eq!(prefix_to_subnet_mask(27), "255.255.255.224");
        assert_eq!(prefix_to_subnet_mask(28), "255.255.255.240");
        assert_eq!(prefix_to_subnet_mask(29), "255.255.255.248");
        assert_eq!(prefix_to_subnet_mask(30), "255.255.255.252");
        assert_eq!(prefix_to_subnet_mask(31), "255.255.255.254");
    }

    #[test]
    fn test_prefix_to_subnet_mask_class_boundaries() {
        assert_eq!(prefix_to_subnet_mask(8), "255.0.0.0");
        assert_eq!(prefix_to_subnet_mask(16), "255.255.0.0");
        assert_eq!(prefix_to_subnet_mask(24), "255.255.255.0");
    }

    // WiFi icon tests

    #[test]
    fn test_wifi_icon_excellent_signal() {
        assert_eq!(
            get_wifi_icon(Some(100), true, true),
            "network-wireless-signal-excellent-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(76), true, true),
            "network-wireless-signal-excellent-symbolic"
        );
    }

    #[test]
    fn test_wifi_icon_good_signal() {
        assert_eq!(
            get_wifi_icon(Some(75), true, true),
            "network-wireless-signal-good-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(51), true, true),
            "network-wireless-signal-good-symbolic"
        );
    }

    #[test]
    fn test_wifi_icon_ok_signal() {
        assert_eq!(
            get_wifi_icon(Some(50), true, true),
            "network-wireless-signal-ok-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(26), true, true),
            "network-wireless-signal-ok-symbolic"
        );
    }

    #[test]
    fn test_wifi_icon_weak_signal() {
        assert_eq!(
            get_wifi_icon(Some(25), true, true),
            "network-wireless-signal-weak-symbolic"
        );
        assert_eq!(
            get_wifi_icon(Some(0), true, true),
            "network-wireless-signal-weak-symbolic"
        );
    }

    #[test]
    fn test_wifi_icon_disabled() {
        assert_eq!(
            get_wifi_icon(Some(100), false, true),
            "network-wireless-symbolic"
        );
    }

    #[test]
    fn test_wifi_icon_disconnected() {
        assert_eq!(
            get_wifi_icon(Some(100), true, false),
            "network-wireless-symbolic"
        );
    }

    #[test]
    fn test_wifi_icon_no_strength_data() {
        assert_eq!(get_wifi_icon(None, true, true), "network-wireless-symbolic");
    }
}
