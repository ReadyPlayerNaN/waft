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
use waft_plugin_notifications::config;
use waft_plugin_notifications::dbus::client::{IngressEvent, OutboundEvent, close_reasons};
use waft_plugin_notifications::dbus::server::NotificationsDbusServer;
use waft_plugin_notifications::filter;
use waft_plugin_notifications::sound::player::SoundPlayer;
use waft_plugin_notifications::sound::policy::{NotificationContext, SoundDecision, SoundPolicy};
use waft_plugin_notifications::store::{NotificationOp, State, process_op};
use waft_plugin_notifications::ttl;
use waft_protocol::entity::notification::{DND_ENTITY_TYPE, NOTIFICATION_ENTITY_TYPE};
use waft_protocol::entity::notification_filter::{
    ACTIVE_PROFILE_ENTITY_TYPE, NOTIFICATION_GROUP_ENTITY_TYPE, NOTIFICATION_PROFILE_ENTITY_TYPE,
};

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides(&[
        NOTIFICATION_ENTITY_TYPE,
        DND_ENTITY_TYPE,
        NOTIFICATION_GROUP_ENTITY_TYPE,
        NOTIFICATION_PROFILE_ENTITY_TYPE,
        ACTIVE_PROFILE_ENTITY_TYPE,
    ]) {
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

        // Load filter configuration
        let (groups, profiles) = config::load_filter_config();

        // Determine active profile
        let active_profile_id = filter::load_active_profile()
            .or_else(|| profiles.first().map(|p| p.id.clone()))
            .unwrap_or_else(|| "default".to_string());

        log::info!(
            "[notifications] loaded {} groups, {} profiles, active: {}",
            groups.len(),
            profiles.len(),
            active_profile_id
        );

        // Create the plugin with filter config
        let plugin = NotificationsPlugin::new(
            state.clone(),
            outbound_tx.clone(),
            groups,
            profiles,
            active_profile_id,
        );

        // Get a filter handle BEFORE passing the plugin to the runtime
        // (the runtime consumes the plugin, so we extract shared state first)
        let filter_handle = plugin.filter_handle();

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

        // Sound infrastructure (immutable after load)
        let sound_config = config::load_sound_config();
        let sound_policy = Arc::new(SoundPolicy::new(sound_config));
        let sound_player = Arc::new(SoundPlayer::new());

        // Spawn ingress monitor: D-Bus Notify/Close -> plugin state -> entity notify
        let ingress_state = state.clone();
        let ingress_notifier = notifier.clone();
        let ingress_outbound_tx = outbound_tx;
        let ingress_sound_policy = sound_policy.clone();
        let ingress_sound_player = sound_player.clone();
        let ttl_wake = Arc::new(tokio::sync::Notify::new());
        let ttl_wake_for_ingress = ttl_wake.clone();
        tokio::spawn(async move {
            while let Ok(event) = ingress_rx.recv_async().await {
                match event {
                    IngressEvent::Notify { notification } => {
                        // 1. Match notification against filter groups
                        let matched_group = filter_handle.match_notification(&notification);

                        // 2. Get filter actions for the matched group
                        let filter_actions =
                            filter_handle.get_filter_actions(matched_group.as_deref());

                        // 3. Apply hide filter (drop notification entirely)
                        if filter_actions.hide {
                            log::debug!(
                                "[notifications] hiding notification from {:?} (group: {:?})",
                                notification.app_name,
                                matched_group
                            );
                            continue;
                        }

                        // 4. Evaluate sound policy (check no_sound filter action)
                        let sound_decision = if filter_actions.no_sound {
                            SoundDecision::Silent
                        } else {
                            let guard = match ingress_state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    log::warn!(
                                        "[notifications/ingress] mutex poisoned, recovering: {e}"
                                    );
                                    e.into_inner()
                                }
                            };
                            let ctx = NotificationContext {
                                app_name: notification
                                    .app_name
                                    .as_ref()
                                    .map(|s| s.as_ref()),
                                urgency: notification.hints.urgency,
                                suppress_sound: notification.hints.suppress_sound,
                                sound_file: notification
                                    .hints
                                    .sound_file
                                    .as_ref()
                                    .map(|s| s.as_ref()),
                                sound_name: notification
                                    .hints
                                    .sound_name
                                    .as_ref()
                                    .map(|s| s.as_ref()),
                                category: notification
                                    .hints
                                    .category_raw
                                    .as_ref()
                                    .map(|s| s.as_ref()),
                                dnd_active: guard.dnd,
                            };
                            ingress_sound_policy.evaluate(&ctx)
                        };

                        // 5. Mutate state and apply suppress_toast
                        let notif_id = notification.id;
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
                            if filter_actions.no_toast {
                                if let Some(stored) = guard.notifications.get_mut(&notif_id) {
                                    stored.suppress_toast = true;
                                }
                            }
                        }

                        // 6. Play sound AFTER state mutation (non-blocking)
                        if let SoundDecision::Play(sound_id) = sound_decision {
                            let player = ingress_sound_player.clone();
                            tokio::spawn(async move {
                                player.play(&sound_id).await;
                            });
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
