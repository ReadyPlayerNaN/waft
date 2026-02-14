//! Waft notifications daemon.
//!
//! Standalone tokio binary that:
//! - Owns `org.freedesktop.Notifications` on the session bus
//! - Translates D-Bus notifications into entities for the waft daemon
//! - Provides `notification` and `dnd` entity types

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex as StdMutex};

use waft_plugin::PluginRuntime;
use waft_plugin_notifications::NotificationsPlugin;
use waft_plugin_notifications::dbus::client::{IngressEvent, OutboundEvent, close_reasons};
use waft_plugin_notifications::dbus::server::NotificationsDbusServer;
use waft_plugin_notifications::store::{NotificationOp, State, process_op};
use waft_plugin_notifications::ttl;
use waft_protocol::entity::notification::{DND_ENTITY_TYPE, NOTIFICATION_ENTITY_TYPE};

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[NOTIFICATION_ENTITY_TYPE, DND_ENTITY_TYPE]) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting notifications plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        // Shared state between plugin, D-Bus server, and TTL timer
        let state = Arc::new(StdMutex::new(State::new()));

        // Channels for D-Bus communication
        let (ingress_tx, ingress_rx) = flume::unbounded::<IngressEvent>();
        let (outbound_tx, outbound_rx) = flume::unbounded();

        // Create the plugin
        let plugin = NotificationsPlugin::new(state.clone(), outbound_tx.clone());

        // Create the plugin runtime (connects to waft daemon)
        let (runtime, notifier) = PluginRuntime::new("notifications", plugin);

        // Start D-Bus server
        let mut dbus_server = NotificationsDbusServer::connect()
            .await
            .context("failed to create D-Bus server")?;

        dbus_server
            .start(ingress_tx, outbound_rx)
            .await
            .context("failed to start D-Bus server")?;

        // Spawn ingress monitor: D-Bus Notify/Close -> plugin state -> entity notify
        let ingress_state = state.clone();
        let ingress_notifier = notifier.clone();
        let ingress_outbound_tx = outbound_tx;
        let ttl_wake = Arc::new(tokio::sync::Notify::new());
        let ttl_wake_for_ingress = ttl_wake.clone();
        tokio::spawn(async move {
            while let Ok(event) = ingress_rx.recv_async().await {
                match event {
                    IngressEvent::Notify { notification } => {
                        {
                            let mut guard = match ingress_state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    log::warn!(
                                        "[notifications/ingress] mutex poisoned, recovering: {e}"
                                    );
                                    e.into_inner()
                                }
                            };
                            process_op(&mut guard, NotificationOp::Ingress(notification));
                        }
                        ingress_notifier.notify();
                        // Wake TTL timer in case this notification has an earlier deadline
                        ttl_wake_for_ingress.notify_one();
                    }
                    IngressEvent::CloseNotification { id } => {
                        {
                            let mut guard = match ingress_state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    log::warn!(
                                        "[notifications/ingress] mutex poisoned, recovering: {e}"
                                    );
                                    e.into_inner()
                                }
                            };
                            process_op(&mut guard, NotificationOp::NotificationRetract(id as u64));
                        }
                        if ingress_outbound_tx
                            .send(OutboundEvent::NotificationClosed {
                                id,
                                reason: close_reasons::CLOSED_BY_CALL,
                            })
                            .is_err()
                        {
                            log::warn!("[notifications/ingress] outbound channel closed");
                            break;
                        }
                        ingress_notifier.notify();
                    }
                }
            }
            log::warn!("[notifications] ingress receiver loop exited");
        });

        // Spawn TTL expiration timer (sleep-to-deadline, no polling)
        let ttl_state = state.clone();
        let ttl_notifier = notifier.clone();
        tokio::spawn(async move {
            ttl::run_ttl_expiration(ttl_state, ttl_notifier, ttl_wake).await;
            log::warn!("[notifications] TTL expiration loop exited");
        });

        // Run the plugin runtime (blocks until shutdown)
        runtime.run().await?;
        Ok(())
    })
}
