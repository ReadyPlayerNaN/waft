//! WiFi scan background task using pure D-Bus.

use std::sync::{Arc, Mutex as StdMutex};

use log::{debug, error, info, warn};
use waft_plugin::{EntityNotifier, lock_or_recover};
use zbus::Connection;

use crate::state::{CachedConnectionSettings, NmState};
use crate::wifi::{get_connection_settings, get_connections_for_ssid, scan_wifi_networks};

/// Background task: handles WiFi scanning via D-Bus.
/// Receives scan requests via channel and updates shared state.
pub async fn wifi_scan_task(
    mut scan_rx: tokio::sync::mpsc::Receiver<()>,
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    notifier: EntityNotifier,
) {
    while let Some(()) = scan_rx.recv().await {
        debug!("[nm] WiFi scan requested");

        // Read adapter paths and set scanning state
        let adapter_paths: Vec<String> = {
            let mut st = lock_or_recover(&state);
            for adapter in &mut st.wifi_adapters {
                adapter.scanning = true;
            }
            st.wifi_adapters.iter().map(|a| a.path.clone()).collect()
        };
        notifier.notify();

        match scan_wifi_networks(&conn, &adapter_paths).await {
            Ok(mut networks) => {
                info!("[nm] WiFi scan found {} networks", networks.len());

                // Read connection settings for known networks
                for network in &mut networks {
                    if !network.known {
                        continue;
                    }
                    match get_connections_for_ssid(&conn, &network.ssid).await {
                        Ok(paths) if !paths.is_empty() => {
                            match get_connection_settings(&conn, &paths[0]).await {
                                Ok(settings) => {
                                    network.cached_settings = Some(CachedConnectionSettings {
                                        autoconnect: settings.autoconnect,
                                        metered: settings.metered,
                                        ip_method: settings.ip_method,
                                        dns_servers: settings.dns_servers,
                                    });
                                }
                                Err(e) => {
                                    debug!(
                                        "[nm] Failed to read settings for {}: {e}",
                                        network.ssid
                                    );
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            debug!(
                                "[nm] Failed to find connections for {}: {e}",
                                network.ssid
                            );
                        }
                    }
                }

                let mut st = lock_or_recover(&state);
                for adapter in &mut st.wifi_adapters {
                    adapter.access_points = networks.clone();
                    adapter.scanning = false;
                }
            }
            Err(e) => {
                error!("[nm] WiFi scan failed: {e}");
                let mut st = lock_or_recover(&state);
                for adapter in &mut st.wifi_adapters {
                    adapter.scanning = false;
                }
            }
        }

        notifier.notify();
    }

    warn!("[nm] WiFi scan task stopped");
}
