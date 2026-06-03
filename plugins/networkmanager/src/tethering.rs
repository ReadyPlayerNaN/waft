//! Bluetooth tethering connection profile discovery and state management.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::Connection;
use zbus::zvariant::ObjectPath;

use crate::dbus_property::{NM_INTERFACE, NM_PATH, NM_SERVICE};
use crate::state::{NmState, TetheringConnectionState, TetheringProfileInfo};

/// List all saved bluetooth connection profiles from NetworkManager.
pub async fn get_tethering_profiles(nm: &nmrs::NetworkManager) -> Result<Vec<TetheringProfileInfo>> {
    let saved = nm.list_saved_connections().await?;
    let mut profiles = Vec::new();

    for conn in saved {
        if conn.connection_type != "bluetooth" {
            continue;
        }

        let bdaddr = match &conn.summary {
            nmrs::models::SettingsSummary::Bluetooth { bdaddr, .. } => Some(bdaddr.clone()),
            _ => None,
        };
        if bdaddr.is_none() {
            warn!(
                "[nm] Tethering profile {} missing bdaddr, cannot match to BlueZ device",
                conn.path
            );
        }

        profiles.push(TetheringProfileInfo {
            path: conn.path.to_string(),
            uuid: conn.uuid,
            name: conn.id,
            bdaddr,
        });
    }

    Ok(profiles)
}

/// Get active bluetooth tethering states keyed by BDADDR.
pub async fn get_active_tethering_connections(
    nm: &nmrs::NetworkManager,
) -> Result<HashMap<String, bool>> {
    let devices = nm.list_bluetooth_devices().await?;
    Ok(devices
        .into_iter()
        .map(|device| {
            let active = matches!(device.state, nmrs::models::DeviceState::Activated);
            (device.bdaddr, active)
        })
        .collect())
}

/// Deactivate a tethering connection by its active connection path.
pub async fn deactivate_tethering(conn: &Connection, active_connection_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, NM_SERVICE, NM_PATH, NM_INTERFACE)
        .await
        .context("Failed to create NM proxy")?;

    let active_obj = ObjectPath::try_from(active_connection_path)?;
    let _: () = proxy
        .call("DeactivateConnection", &(active_obj,))
        .await
        .context("Failed to deactivate tethering connection")?;

    Ok(())
}

/// Refresh tethering connection states from D-Bus.
pub async fn refresh_tethering_states(
    _conn: &Connection,
    nm: &nmrs::NetworkManager,
    state: &Arc<StdMutex<NmState>>,
) -> Result<()> {
    let profiles = get_tethering_profiles(nm).await?;
    let active = get_active_tethering_connections(nm).await.unwrap_or_default();

    let new_connections: Vec<TetheringConnectionState> = profiles
        .into_iter()
        .map(|profile| {
            let is_active = profile
                .bdaddr
                .as_ref()
                .and_then(|bdaddr| active.get(bdaddr).map(|v| *v))
                .unwrap_or(false);

            TetheringConnectionState {
                path: profile.path,
                uuid: profile.uuid,
                name: profile.name,
                active: is_active,
                active_path: None,
                bdaddr: profile.bdaddr,
            }
        })
        .collect();

    let mut st = match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[nm] Mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };
    st.tethering_connections = new_connections;

    debug!(
        "[nm] Refreshed tethering state: {} profiles",
        st.tethering_connections.len()
    );

    Ok(())
}
