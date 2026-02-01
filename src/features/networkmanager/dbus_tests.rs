use super::*;

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

// Integration tests for NetworkManager DBus functionality require a mock
// NetworkManager service or access to the system NetworkManager.
//
// The networkmanager module uses complex NetworkManager-specific interfaces:
// - org.freedesktop.NetworkManager for device discovery and activation
// - org.freedesktop.NetworkManager.Device for device properties
// - org.freedesktop.NetworkManager.Device.Wireless for WiFi operations
// - org.freedesktop.NetworkManager.AccessPoint for access point properties
// - org.freedesktop.NetworkManager.Settings for connection management
//
// Future work: Add integration tests using mock NetworkManager service.
//
// Test scenarios to add:
//
// Device discovery:
// - check_availability() returns true when NetworkManager is available
// - check_availability() returns false when NetworkManager is not available
// - get_all_devices() returns empty list when no devices present
// - get_all_devices() finds ethernet and WiFi devices
// - get_all_devices() filters out non-ethernet/WiFi devices
// - get_all_devices() filters out virtual interfaces
// - get_all_devices() filters out unmanaged devices
// - get_all_devices() filters out non-real devices
//
// Device properties:
// - get_device_state() returns correct state value
// - get_device_active_connection() returns None when no connection active
// - get_device_active_connection() returns connection path when active
//
// WiFi operations:
// - get_wireless_enabled() returns current wireless enabled state
// - set_wireless_enabled() successfully enables wireless
// - set_wireless_enabled() successfully disables wireless
// - request_scan() successfully triggers a scan
// - get_access_points() returns list of available access points
// - get_access_points() filters out hidden networks (empty SSID)
// - get_access_points() correctly parses SSID, strength, and security flags
// - get_active_access_point() returns None when not connected
// - get_active_access_point() returns access point path when connected
// - get_access_point_ssid() correctly decodes SSID from bytes
//
// Connection management:
// - activate_connection() successfully activates a connection
// - get_connections_for_ssid() finds matching WiFi connections
// - get_connections_for_ssid() returns empty list when no matches
// - get_connections_for_ssid() handles SSID byte array comparison

// Tests for prefix_to_subnet_mask conversion

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
    // Edge case: prefix > 32 should saturate to /32
    assert_eq!(prefix_to_subnet_mask(33), "255.255.255.255");
    assert_eq!(prefix_to_subnet_mask(64), "255.255.255.255");
}

#[test]
fn test_prefix_to_subnet_mask_common_values() {
    // /25 through /31 for fine-grained subnetting
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
    // Class A, B, C default masks
    assert_eq!(prefix_to_subnet_mask(8), "255.0.0.0"); // Class A
    assert_eq!(prefix_to_subnet_mask(16), "255.255.0.0"); // Class B
    assert_eq!(prefix_to_subnet_mask(24), "255.255.255.0"); // Class C
}
