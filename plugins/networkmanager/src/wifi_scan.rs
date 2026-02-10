//! WiFi scan background task using nmrs (non-Send).
//!
//! Runs on a dedicated thread with a single-threaded tokio runtime + LocalSet
//! because nmrs futures are !Send and cannot be spawned on the multi-threaded runtime.

use std::sync::{Arc, Mutex as StdMutex};

use log::{debug, error, info, warn};
use waft_plugin_sdk::WidgetNotifier;
use zbus::Connection;

use crate::state::NmState;
use crate::wifi::scan_and_list_known_networks;

/// Background task: handles WiFi scanning using nmrs (non-Send).
/// Receives scan requests via channel and updates shared state.
pub async fn wifi_scan_task(
    mut scan_rx: tokio::sync::mpsc::Receiver<()>,
    nm: nmrs::NetworkManager,
    conn: Connection,
    state: Arc<StdMutex<NmState>>,
    notifier: WidgetNotifier,
) {
    while let Some(()) = scan_rx.recv().await {
        debug!("[nm] WiFi scan requested");

        // Set scanning state
        {
            let mut st = state.lock().unwrap();
            for adapter in &mut st.wifi_adapters {
                adapter.scanning = true;
            }
        }
        notifier.notify();

        match scan_and_list_known_networks(&nm, &conn).await {
            Ok(networks) => {
                info!("[nm] WiFi scan found {} known networks", networks.len());
                let mut st = state.lock().unwrap();
                for adapter in &mut st.wifi_adapters {
                    adapter.access_points = networks.clone();
                    adapter.scanning = false;
                }
            }
            Err(e) => {
                error!("[nm] WiFi scan failed: {}", e);
                let mut st = state.lock().unwrap();
                for adapter in &mut st.wifi_adapters {
                    adapter.scanning = false;
                }
            }
        }

        notifier.notify();
    }

    warn!("[nm] WiFi scan task stopped");
}
