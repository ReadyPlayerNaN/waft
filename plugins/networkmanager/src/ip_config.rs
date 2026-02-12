//! IP configuration reading from NetworkManager D-Bus interfaces.

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::debug;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::Connection;

use crate::dbus_property::{get_property, NM_DEVICE_INTERFACE};

const NM_IP4CONFIG_INTERFACE: &str = "org.freedesktop.NetworkManager.IP4Config";

/// IP configuration for a connected device.
#[derive(Debug, Clone)]
pub struct DeviceIpConfig {
    pub address: String,
    pub prefix: u8,
    pub gateway: Option<String>,
}

/// Read IP4 configuration from a connected NM device.
///
/// Returns `None` if the device has no IP4Config (disconnected or IPv6-only).
pub async fn get_device_ip4_config(
    conn: &Connection,
    device_path: &str,
) -> Result<Option<DeviceIpConfig>> {
    // Get Ip4Config object path from the device
    let ip4_config_path: OwnedObjectPath =
        match get_property(conn, device_path, NM_DEVICE_INTERFACE, "Ip4Config").await {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

    let path_str = ip4_config_path.as_str();
    if path_str == "/" {
        return Ok(None);
    }

    // Read AddressData - array of dicts with "address" (string) and "prefix" (u32)
    let address_data: Vec<HashMap<String, OwnedValue>> = match get_property(
        conn,
        path_str,
        NM_IP4CONFIG_INTERFACE,
        "AddressData",
    )
    .await
    {
        Ok(data) => data,
        Err(e) => {
            debug!("[nm] Failed to read AddressData from {}: {}", path_str, e);
            return Ok(None);
        }
    };

    let first = match address_data.first() {
        Some(entry) => entry,
        None => return Ok(None),
    };

    let address = match first.get("address") {
        Some(v) => String::try_from(v.clone())
            .context("Failed to parse address from AddressData")?,
        None => return Ok(None),
    };

    let prefix = match first.get("prefix") {
        Some(v) => u32::try_from(v.clone())
            .map(|p| p as u8)
            .unwrap_or(24),
        None => 24,
    };

    // Read Gateway
    let gateway: Option<String> =
        match get_property::<String>(conn, path_str, NM_IP4CONFIG_INTERFACE, "Gateway").await {
            Ok(gw) if !gw.is_empty() => Some(gw),
            _ => None,
        };

    Ok(Some(DeviceIpConfig {
        address,
        prefix,
        gateway,
    }))
}

/// Fetch public IP address from an external service.
///
/// Uses a lightweight HTTP GET to determine the public-facing IP.
/// Returns `None` on failure (network issues, timeout, etc.).
pub async fn fetch_public_ip() -> Option<String> {
    // Use a simple TCP connection to avoid adding reqwest dependency.
    // Connect to ifconfig.me on port 80 with a plain HTTP request.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        fetch_public_ip_inner(),
    )
    .await;

    match result {
        Ok(Some(ip)) => Some(ip),
        Ok(None) => None,
        Err(_) => {
            debug!("[nm] Public IP fetch timed out");
            None
        }
    }
}

async fn fetch_public_ip_inner() -> Option<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = match TcpStream::connect("ifconfig.me:80").await {
        Ok(s) => s,
        Err(e) => {
            debug!("[nm] Failed to connect to ifconfig.me: {}", e);
            return None;
        }
    };

    let request = "GET / HTTP/1.1\r\nHost: ifconfig.me\r\nUser-Agent: curl/8.0\r\nAccept: */*\r\nConnection: close\r\n\r\n";
    if let Err(e) = stream.write_all(request.as_bytes()).await {
        debug!("[nm] Failed to send HTTP request: {}", e);
        return None;
    }

    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response).await {
        debug!("[nm] Failed to read HTTP response: {}", e);
        return None;
    }

    let response_str = String::from_utf8_lossy(&response);

    // Parse HTTP response - body is after \r\n\r\n
    let body = response_str.split("\r\n\r\n").nth(1)?;
    let ip = body.trim().to_string();

    // Basic validation: should look like an IP address
    if ip.contains('.') || ip.contains(':') {
        Some(ip)
    } else {
        debug!("[nm] Unexpected public IP response: {}", ip);
        None
    }
}
