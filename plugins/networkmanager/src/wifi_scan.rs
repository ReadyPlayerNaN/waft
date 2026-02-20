//! WiFi scan background task using pure D-Bus.

use std::sync::{Arc, Mutex as StdMutex};

use log::{debug, error, info, warn};
use waft_plugin::EntityNotifier;
use zbus::Connection;

use crate::state::NmState;
use crate::wifi::scan_wifi_networks;

fn lock_state(state: &StdMutex<NmState>) -> std::sync::MutexGuard<'_, NmState> {
    match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[nm] Mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    }
}

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
            let mut st = lock_state(&state);
            for adapter in &mut st.wifi_adapters {
                adapter.scanning = true;
            }
            st.wifi_adapters.iter().map(|a| a.path.clone()).collect()
        };
        notifier.notify();

        match scan_wifi_networks(&conn, &adapter_paths).await {
            Ok(networks) => {
                info!("[nm] WiFi scan found {} known networks", networks.len());
                let mut st = lock_state(&state);
                for adapter in &mut st.wifi_adapters {
                    adapter.access_points = networks.clone();
                    adapter.scanning = false;
                }
            }
            Err(e) => {
                error!("[nm] WiFi scan failed: {}", e);
                let mut st = lock_state(&state);
                for adapter in &mut st.wifi_adapters {
                    adapter.scanning = false;
                }
            }
        }

        notifier.notify();
    }

    warn!("[nm] WiFi scan task stopped");
}
