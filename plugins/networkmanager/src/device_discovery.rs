//! Shared device-info types used by the NetworkManager plugin.
//!
//! Read-side device discovery is now handled by `nmrs_adapter`.

/// Basic information about a network device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub device_type: u32,
    pub interface_name: String,
    pub device_state: u32,
}
