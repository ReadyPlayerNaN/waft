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
        let interfaces: Vec<String> = {
            let mut st = lock_or_recover(&state);
            for adapter in &mut st.wifi_adapters {
                adapter.scanning = true;
            }
            st.wifi_adapters
                .iter()
                .map(|a| a.interface_name.clone())
                .collect()
        };
        notifier.notify();

        match nmrs_adapter::scan_wifi_networks(&nm, &interfaces).await {
            Ok(networks) => {
                info!("[nm] WiFi scan found {} networks", networks.len());

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
