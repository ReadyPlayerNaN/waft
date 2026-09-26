//! WiFi scan background task using pure D-Bus.

use std::sync::{Arc, Mutex as StdMutex};

use log::{debug, error, info, warn};
use waft_plugin::{EntityNotifier, lock_or_recover};
use zbus::Connection;

use crate::nmrs_adapter;
use crate::state::NmState;

/// Background task: handles WiFi scanning via D-Bus.
/// Receives scan requests via channel and updates shared state.
pub async fn wifi_scan_task(
    mut scan_rx: tokio::sync::mpsc::Receiver<()>,
    _conn: Connection,
    nm: nmrs::NetworkManager,
    state: Arc<StdMutex<NmState>>,
    notifier: EntityNotifier,
) {
    while let Some(()) = scan_rx.recv().await {
        debug!("[nm] WiFi scan requested");

        // Read adapter paths and set scanning state
        let (interfaces, scan_revision): (Vec<String>, u64) = {
            let mut st = lock_or_recover(&state);
            for adapter in &mut st.wifi_adapters {
                adapter.scanning = true;
            }
            st.wifi_revision = st.wifi_revision.wrapping_add(1);
            (
                st.wifi_adapters
                    .iter()
                    .map(|a| a.interface_name.clone())
                    .collect(),
                st.wifi_revision,
            )
        };
        notifier.notify();

        match nmrs_adapter::scan_wifi_networks(&nm, &interfaces).await {
            Ok(networks) => {
                info!("[nm] WiFi scan found {} networks", networks.len());

                let mut st = lock_or_recover(&state);
                let scan_was_interrupted = st.wifi_revision != scan_revision;
                for adapter in &mut st.wifi_adapters {
                    if scan_was_interrupted {
                        // Keep the authoritative connected AP when a signal or
                        // action arrived while this scan was in flight.
                        let active_ap = adapter.active_ssid.as_deref().and_then(|ssid| {
                            adapter
                                .access_points
                                .iter()
                                .find(|ap| ap.ssid == ssid)
                                .cloned()
                        });
                        adapter.access_points = networks.clone();
                        if let Some(active_ap) = active_ap
                            && !adapter
                                .access_points
                                .iter()
                                .any(|ap| ap.ssid == active_ap.ssid)
                        {
                            adapter.access_points.push(active_ap);
                        }
                    } else {
                        adapter.access_points = networks.clone();
                    }
                    adapter.scanning = false;
                }
                st.wifi_revision = st.wifi_revision.wrapping_add(1);
            }
            Err(e) => {
                error!("[nm] WiFi scan failed: {e}");
                let mut st = lock_or_recover(&state);
                for adapter in &mut st.wifi_adapters {
                    adapter.scanning = false;
                }
                st.wifi_revision = st.wifi_revision.wrapping_add(1);
            }
        }

        notifier.notify();
    }

    warn!("[nm] WiFi scan task stopped");
}
