//! D-Bus property access and NetworkManager interface constants.

use anyhow::{Context, Result};
use zbus::zvariant::OwnedValue;
use zbus::Connection;

pub const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
pub const NM_PATH: &str = "/org/freedesktop/NetworkManager";
pub const NM_INTERFACE: &str = "org.freedesktop.NetworkManager";
pub const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
pub const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
pub const NM_SETTINGS_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings";
pub const NM_CONNECTION_ACTIVE_INTERFACE: &str =
    "org.freedesktop.NetworkManager.Connection.Active";
pub const NM_VPN_CONNECTION_INTERFACE: &str =
    "org.freedesktop.NetworkManager.VPN.Connection";

pub const DEVICE_TYPE_ETHERNET: u32 = 1;
pub const DEVICE_TYPE_WIFI: u32 = 2;

pub const NM_WIRELESS_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";

/// Read a single D-Bus property via the org.freedesktop.DBus.Properties interface.
pub async fn get_property<T>(
    conn: &Connection,
    path: &str,
    interface: &str,
    property: &str,
) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::error::Error + Send + Sync + 'static,
{
    let proxy = zbus::Proxy::new(
        conn,
        NM_SERVICE,
        path,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .context("Failed to create Properties proxy")?;

    let (value,): (OwnedValue,) = proxy
        .call("Get", &(interface, property))
        .await
        .with_context(|| format!("Failed to get property {}.{}", interface, property))?;

    T::try_from(value).map_err(|e| anyhow::anyhow!("Failed to convert property: {}", e))
}
