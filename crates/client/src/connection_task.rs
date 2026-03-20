//! Long-running connection lifecycle manager for the waft daemon.
//!
//! Connects to the daemon, subscribes to entity types, and forwards
//! notifications through a flume channel. Handles reconnection on disconnect.

use std::sync::Arc;

use waft_protocol::AppNotification;

use crate::connection::{
    DAEMON_DBUS_NAME, RECONNECT_INTERVAL, WaftClient, request_dbus_activation,
};

/// Events delivered to the app's event loop.
///
/// Wraps daemon notifications with connection lifecycle signals so the UI
/// can react to daemon crashes and reconnections.
pub enum ClientEvent {
    /// A notification forwarded from the daemon.
    Notification(AppNotification),
    /// Successfully connected (or reconnected) to the daemon.
    Connected,
    /// Lost connection to the daemon.
    Disconnected,
}

/// Long-running tokio task that manages the daemon connection lifecycle.
///
/// Connects to the daemon, subscribes to the given entity types, and forwards
/// notifications through `event_tx`. On disconnect, sends `Disconnected`,
/// clears the client handle, and retries every second.
///
/// The task exits when the `event_tx` receiver is dropped (app closed).
pub async fn daemon_connection_task(
    event_tx: flume::Sender<ClientEvent>,
    client_handle: Arc<std::sync::Mutex<Option<WaftClient>>>,
    entity_types: &[&str],
) {
    let mut activation_requested = false;

    loop {
        // Request D-Bus activation on first attempt to auto-start the daemon
        if !activation_requested {
            activation_requested = true;
            if let Err(e) = request_dbus_activation().await {
                log::warn!("[waft-client] D-Bus activation failed: {e}");
            } else {
                log::info!("[waft-client] requested D-Bus activation for {DAEMON_DBUS_NAME}");
            }
        }

        match WaftClient::connect().await {
            Ok((client, notification_rx)) => {
                log::info!("[waft-client] connected to daemon");

                // Subscribe to all entity types and request cached state
                for et in entity_types {
                    client.subscribe(et);
                }
                for et in entity_types {
                    client.request_status(et);
                }
                log::info!(
                    "[waft-client] subscribed to {} entity types",
                    entity_types.len()
                );

                // Store client for write path (actions from GTK thread)
                *lock_or_recover(&client_handle) = Some(client);

                // Signal connected
                if event_tx.send(ClientEvent::Connected).is_err() {
                    log::debug!("[waft-client] app closed, stopping connection task");
                    return;
                }

                // Forward notifications until disconnect
                // Reset activation flag so we re-request on next reconnect cycle
                activation_requested = false;

                while let Ok(notification) = notification_rx.recv_async().await {
                    if event_tx
                        .send(ClientEvent::Notification(notification))
                        .is_err()
                    {
                        log::debug!("[waft-client] app closed, stopping connection task");
                        return;
                    }
                }

                // Notification channel closed = daemon disconnected
                log::info!("[waft-client] daemon disconnected, will retry");

                // Clear write path so actions are dropped during disconnect
                *lock_or_recover(&client_handle) = None;

                // Signal disconnected
                if event_tx.send(ClientEvent::Disconnected).is_err() {
                    log::debug!("[waft-client] app closed, stopping connection task");
                    return;
                }
            }
            Err(e) => {
                log::debug!("[waft-client] connection attempt failed: {e}");
            }
        }

        tokio::time::sleep(RECONNECT_INTERVAL).await;
    }
}

/// Lock a mutex, recovering from poison with a warning log.
fn lock_or_recover<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("[waft-client] mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    }
}
