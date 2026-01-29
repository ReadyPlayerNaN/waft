use anyhow::Result;
use zbus::zvariant::OwnedValue;
use zbus::Connection;

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_INTERFACE: &str = "org.freedesktop.NetworkManager";
const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";

const DEVICE_TYPE_ETHERNET: u32 = 1;
const DEVICE_TYPE_WIFI: u32 = 2;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub device_type: u32,
    pub interface_name: String,
    pub managed: bool,
    pub real: bool,
}

pub async fn check_availability() -> bool {
    let conn = match Connection::system().await {
        Ok(conn) => conn,
        Err(_) => return false,
    };

    conn.call_method(Some(NM_SERVICE), NM_PATH, Some(NM_INTERFACE), "GetDevices", &())
        .await
        .is_ok()
}

pub async fn get_all_devices() -> Result<Vec<DeviceInfo>> {
    let conn = Connection::system().await?;

    let device_paths: Vec<zbus::zvariant::OwnedObjectPath> = conn
        .call_method(Some(NM_SERVICE), NM_PATH, Some(NM_INTERFACE), "GetDevices", &())
        .await?
        .body()
        .deserialize()?;

    let mut devices = Vec::new();

    for device_path in device_paths {
        let path_str = device_path.to_string();

        let device_type = get_device_property::<u32>(&conn, &path_str, "DeviceType").await?;

        if device_type != DEVICE_TYPE_ETHERNET && device_type != DEVICE_TYPE_WIFI {
            continue;
        }

        let interface_name = get_device_property::<String>(&conn, &path_str, "Interface").await?;

        if is_virtual_interface(&interface_name) {
            continue;
        }

        let managed = get_device_property::<bool>(&conn, &path_str, "Managed")
            .await
            .unwrap_or(false);
        let real = get_device_property::<bool>(&conn, &path_str, "Real")
            .await
            .unwrap_or(true);

        if !managed || !real {
            continue;
        }

        devices.push(DeviceInfo {
            path: path_str,
            device_type,
            interface_name,
            managed,
            real,
        });
    }

    Ok(devices)
}

async fn get_device_property<T>(conn: &Connection, path: &str, property: &str) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let value: OwnedValue = conn
        .call_method(
            Some(NM_SERVICE),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_DEVICE_INTERFACE, property),
        )
        .await?
        .body()
        .deserialize::<(OwnedValue,)>()?
        .0;

    value
        .try_into()
        .map_err(|e: T::Error| anyhow::anyhow!("Failed to convert property: {}", e))
}

fn is_virtual_interface(name: &str) -> bool {
    let virtual_prefixes = ["docker", "veth", "br-", "virbr", "vnet"];
    virtual_prefixes.iter().any(|prefix| name.starts_with(prefix))
}

pub async fn get_device_state(path: &str) -> Result<u32> {
    let conn = Connection::system().await?;
    get_device_property(&conn, path, "State").await
}

pub async fn get_device_active_connection(path: &str) -> Result<Option<String>> {
    let conn = Connection::system().await?;
    let active_conn_path: String =
        get_device_property(&conn, path, "ActiveConnection").await?;

    if active_conn_path == "/" {
        Ok(None)
    } else {
        Ok(Some(active_conn_path))
    }
}

// WiFi-specific operations

const NM_WIRELESS_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const NM_ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";

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
    pub fn is_secure(&self) -> bool {
        self.flags != 0 || self.wpa_flags != 0 || self.rsn_flags != 0
    }
}

pub async fn get_wireless_enabled(device_path: &str) -> Result<bool> {
    let conn = Connection::system().await?;

    let enabled: u32 = conn
        .call_method(
            Some(NM_SERVICE),
            NM_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_INTERFACE, "WirelessEnabled"),
        )
        .await?
        .body()
        .deserialize::<(OwnedValue,)>()?
        .0
        .try_into()?;

    Ok(enabled != 0)
}

pub async fn set_wireless_enabled(device_path: &str, enabled: bool) -> Result<()> {
    let conn = Connection::system().await?;

    conn.call_method(
        Some(NM_SERVICE),
        NM_PATH,
        Some("org.freedesktop.DBus.Properties"),
        "Set",
        &(NM_INTERFACE, "WirelessEnabled", OwnedValue::from(enabled)),
    )
    .await?;

    Ok(())
}

pub async fn request_scan(device_path: &str) -> Result<()> {
    let conn = Connection::system().await?;

    let options: std::collections::HashMap<&str, OwnedValue> = std::collections::HashMap::new();

    conn.call_method(
        Some(NM_SERVICE),
        device_path,
        Some(NM_WIRELESS_INTERFACE),
        "RequestScan",
        &(options,),
    )
    .await?;

    Ok(())
}

pub async fn get_access_points(device_path: &str) -> Result<Vec<AccessPoint>> {
    let conn = Connection::system().await?;

    let ap_paths: Vec<zbus::zvariant::OwnedObjectPath> = conn
        .call_method(
            Some(NM_SERVICE),
            device_path,
            Some(NM_WIRELESS_INTERFACE),
            "GetAccessPoints",
            &(),
        )
        .await?
        .body()
        .deserialize()?;

    let mut access_points = Vec::new();

    for ap_path in ap_paths {
        let path_str = ap_path.to_string();

        // Get SSID
        let ssid_bytes: Vec<u8> = get_ap_property(&conn, &path_str, "Ssid").await?;
        let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();

        // Skip hidden networks (empty SSID)
        if ssid.is_empty() {
            continue;
        }

        let strength: u8 = get_ap_property(&conn, &path_str, "Strength").await?;
        let flags: u32 = get_ap_property(&conn, &path_str, "Flags").await?;
        let wpa_flags: u32 = get_ap_property(&conn, &path_str, "WpaFlags").await?;
        let rsn_flags: u32 = get_ap_property(&conn, &path_str, "RsnFlags").await?;

        access_points.push(AccessPoint {
            path: path_str,
            ssid,
            strength,
            flags,
            wpa_flags,
            rsn_flags,
        });
    }

    Ok(access_points)
}

pub async fn get_active_access_point(device_path: &str) -> Result<Option<String>> {
    let conn = Connection::system().await?;
    let ap_path: String = conn
        .call_method(
            Some(NM_SERVICE),
            device_path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_WIRELESS_INTERFACE, "ActiveAccessPoint"),
        )
        .await?
        .body()
        .deserialize::<(OwnedValue,)>()?
        .0
        .try_into()?;

    if ap_path == "/" {
        Ok(None)
    } else {
        Ok(Some(ap_path))
    }
}

pub async fn get_access_point_ssid(ap_path: &str) -> Result<String> {
    let conn = Connection::system().await?;
    let ssid_bytes: Vec<u8> = get_ap_property(&conn, ap_path, "Ssid").await?;
    Ok(String::from_utf8_lossy(&ssid_bytes).to_string())
}

async fn get_ap_property<T>(conn: &Connection, path: &str, property: &str) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let value: OwnedValue = conn
        .call_method(
            Some(NM_SERVICE),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(NM_ACCESS_POINT_INTERFACE, property),
        )
        .await?
        .body()
        .deserialize::<(OwnedValue,)>()?
        .0;

    value
        .try_into()
        .map_err(|e: T::Error| anyhow::anyhow!("Failed to convert property: {}", e))
}

pub async fn activate_connection(
    connection_path: Option<&str>,
    device_path: &str,
    specific_object: Option<&str>,
) -> Result<String> {
    let conn = Connection::system().await?;

    let conn_path = connection_path.unwrap_or("/");
    let specific = specific_object.unwrap_or("/");

    let active_conn_path: zbus::zvariant::OwnedObjectPath = conn
        .call_method(
            Some(NM_SERVICE),
            NM_PATH,
            Some(NM_INTERFACE),
            "ActivateConnection",
            &(conn_path, device_path, specific),
        )
        .await?
        .body()
        .deserialize()?;

    Ok(active_conn_path.to_string())
}

pub async fn get_connections_for_ssid(ssid: &str) -> Result<Vec<String>> {
    let conn = Connection::system().await?;

    // Get all connection paths
    let settings_paths: Vec<zbus::zvariant::OwnedObjectPath> = conn
        .call_method(
            Some(NM_SERVICE),
            "/org/freedesktop/NetworkManager/Settings",
            Some("org.freedesktop.NetworkManager.Settings"),
            "ListConnections",
            &(),
        )
        .await?
        .body()
        .deserialize()?;

    let mut matching_connections = Vec::new();

    for settings_path in settings_paths {
        let path_str = settings_path.as_str();

        // Get connection settings
        let settings: std::collections::HashMap<String, std::collections::HashMap<String, OwnedValue>> = conn
            .call_method(
                Some(NM_SERVICE),
                path_str,
                Some("org.freedesktop.NetworkManager.Settings.Connection"),
                "GetSettings",
                &(),
            )
            .await?
            .body()
            .deserialize()?;

        // Check if this is a WiFi connection with matching SSID
        if let Some(wireless) = settings.get("802-11-wireless") {
            if let Some(ssid_value) = wireless.get("ssid") {
                if let Ok(ssid_bytes) = <Vec<u8>>::try_from(ssid_value.clone()) {
                    let connection_ssid = String::from_utf8_lossy(&ssid_bytes);
                    if connection_ssid == ssid {
                        matching_connections.push(path_str.to_string());
                    }
                }
            }
        }
    }

    Ok(matching_connections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_interface_detection() {
        assert!(is_virtual_interface("docker0"));
        assert!(is_virtual_interface("veth1234"));
        assert!(is_virtual_interface("br-abcd"));
        assert!(is_virtual_interface("virbr0"));
        assert!(is_virtual_interface("vnet0"));

        assert!(!is_virtual_interface("eth0"));
        assert!(!is_virtual_interface("wlan0"));
        assert!(!is_virtual_interface("usb0"));
        assert!(!is_virtual_interface("rndis0"));
    }
}
