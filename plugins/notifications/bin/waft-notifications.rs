//! Waft notifications daemon.
//!
//! Standalone tokio binary that:
//! - Owns `org.freedesktop.Notifications` on the session bus
//! - Translates D-Bus notifications into entities for the waft daemon
//! - Provides `notification` and `dnd` entity types

use std::sync::LazyLock;

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex as StdMutex};

use waft_plugin::{PluginRunner, StateLocker};

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/notifications.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/notifications.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

use waft_plugin_notifications::NotificationsPlugin;
use waft_plugin_notifications::config;
use waft_plugin_notifications::dbus::client::{IngressEvent, OutboundEvent, close_reasons};
use waft_plugin_notifications::dbus::server::NotificationsDbusServer;
use waft_plugin_notifications::filter;
use waft_plugin_notifications::sound::player::SoundPlayer;
use waft_plugin_notifications::sound::policy::{NotificationContext, SoundDecision};
use waft_plugin_notifications::store::{NotificationOp, State, process_op};
use waft_plugin_notifications::ttl;
use waft_protocol::entity::notification::{DND_ENTITY_TYPE, NOTIFICATION_ENTITY_TYPE, RECORDING_ENTITY_TYPE};
use waft_protocol::entity::notification_filter::{
    ACTIVE_PROFILE_ENTITY_TYPE, NOTIFICATION_GROUP_ENTITY_TYPE, NOTIFICATION_PROFILE_ENTITY_TYPE,
    SOUND_CONFIG_ENTITY_TYPE,
};
use waft_protocol::entity::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE;

fn main() -> Result<()> {
    PluginRunner::new("notifications", &[
        NOTIFICATION_ENTITY_TYPE,
        DND_ENTITY_TYPE,
        RECORDING_ENTITY_TYPE,
        NOTIFICATION_GROUP_ENTITY_TYPE,
        NOTIFICATION_PROFILE_ENTITY_TYPE,
        ACTIVE_PROFILE_ENTITY_TYPE,
        SOUND_CONFIG_ENTITY_TYPE,
        NOTIFICATION_SOUND_ENTITY_TYPE,
    ])
    .i18n(i18n(), "plugin-name", "plugin-description")
    .run(|notifier| async move {
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

        // Load sound configuration
        let sound_config = config::load_sound_config();

        // Load recording configuration
        let recording_config = config::load_recording_config();

        // Create the plugin with filter, sound, and recording config
        let plugin = NotificationsPlugin::new(
            state.clone(),
            outbound_tx.clone(),
            groups,
            profiles,
            active_profile_id,
            sound_config,
            recording_config.recording,
            i18n(),
        );

        // Get handles BEFORE passing the plugin to the runtime
        // (the runtime consumes the plugin, so we extract shared state first)
        let filter_handle = plugin.filter_handle();
        let sound_policy_handle = plugin.sound_policy_handle();
        let ingress_recorder = plugin.recorder();

        // Start D-Bus server
        let mut dbus_server = NotificationsDbusServer::connect()
            .await
            .context("failed to create D-Bus server")?;

        dbus_server
            .start(ingress_tx, outbound_rx)
            .await
            .context("failed to start D-Bus server")?;

        // Sound player
        let sound_player = Arc::new(SoundPlayer::new());

        // Spawn ingress monitor: D-Bus Notify/Close -> plugin state -> entity notify
        let ingress_state = state.clone();
        let ingress_notifier = notifier.clone();
        let ingress_outbound_tx = outbound_tx;
        let ingress_sound_policy = sound_policy_handle;
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

                        // 4. Evaluate sound policy (check no_sound filter action, per-group sound)
                        let sound_decision = if filter_actions.no_sound {
                            SoundDecision::Silent
                        } else if let Some(ref custom_sound) = filter_actions.sound {
                            SoundDecision::Play(custom_sound.clone())
                        } else {
                            let guard = ingress_state.lock_or_recover();
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
                            let policy = ingress_sound_policy.lock_or_recover();
                            policy.evaluate(&ctx)
                        };

                        // 5. Record notification (if recording is active)
                        {
                            let urn_str = format!(
                                "notifications/{}/{}",
                                NOTIFICATION_ENTITY_TYPE, notification.id
                            );
                            let proto_notif = waft_protocol::entity::notification::Notification {
                                title: notification.title.to_string(),
                                description: notification.description.to_string(),
                                app_name: notification
                                    .app_name
                                    .as_ref()
                                    .map(|s| s.to_string()),
                                app_id: notification
                                    .hints
                                    .desktop_entry
                                    .as_ref()
                                    .map(|s| s.to_string()),
                                urgency: match notification.hints.urgency {
                                    waft_plugin_notifications::types::NotificationUrgency::Low => {
                                        waft_protocol::entity::notification::NotificationUrgency::Low
                                    }
                                    waft_plugin_notifications::types::NotificationUrgency::Normal => {
                                        waft_protocol::entity::notification::NotificationUrgency::Normal
                                    }
                                    waft_plugin_notifications::types::NotificationUrgency::Critical => {
                                        waft_protocol::entity::notification::NotificationUrgency::Critical
                                    }
                                },
                                actions: notification
                                    .actions
                                    .chunks(2)
                                    .filter_map(|chunk| {
                                        if chunk.len() == 2 {
                                            Some(waft_protocol::entity::notification::NotificationAction {
                                                key: chunk[0].to_string(),
                                                label: chunk[1].to_string(),
                                            })
                                        } else {
                                            None
                                        }
                                    })
                                    .collect(),
                                icon_hints: {
                                    let mut hints = Vec::new();
                                    if let Some(ref data) = notification.hints.image_data {
                                        hints.push(
                                            waft_protocol::entity::notification::NotificationIconHint::Bytes(
                                                data.clone(),
                                            ),
                                        );
                                    }
                                    if let Some(ref path) = notification.hints.image_path {
                                        hints.push(
                                            waft_protocol::entity::notification::NotificationIconHint::FilePath(
                                                path.to_string(),
                                            ),
                                        );
                                    }
                                    if let Some(ref icon) = notification.icon {
                                        hints.push(
                                            waft_protocol::entity::notification::NotificationIconHint::Themed(
                                                icon.to_string(),
                                            ),
                                        );
                                    }
                                    hints
                                },
                                created_at_ms: notification
                                    .created_at
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as i64)
                                    .unwrap_or(0),
                                resident: notification.hints.resident,
                                workspace: None,
                                suppress_toast: filter_actions.no_toast,
                                ttl: notification.ttl,
                            };
                            ingress_recorder.record(&proto_notif, &urn_str);
                        }

                        // 6. Mutate state and apply suppress_toast
                        let notif_id = notification.id;
                        {
                            let mut guard = ingress_state.lock_or_recover();
                            process_op(&mut guard, NotificationOp::Ingress(notification), i18n());
                            if filter_actions.no_toast
                                && let Some(stored) = guard.notifications.get_mut(&notif_id)
                            {
                                stored.suppress_toast = true;
                            }
                        }

                        // 6. Play sound AFTER state mutation (non-blocking)
                        if let SoundDecision::Play(sound_id) = sound_decision {
                            let resolved = waft_plugin_notifications::sound::gallery::resolve_sound_reference(&sound_id);
                            let player = ingress_sound_player.clone();
                            tokio::spawn(async move {
                                player.play(&resolved).await;
                            });
                        }

                        ingress_notifier.notify();
                        // Wake TTL timer in case this notification has an earlier deadline
                        ttl_wake_for_ingress.notify_one();
                    }
                    IngressEvent::CloseNotification { id } => {
                        {
                            let mut guard = ingress_state.lock_or_recover();
                            process_op(&mut guard, NotificationOp::NotificationRetract(id as u64), i18n());
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
            ttl::run_ttl_expiration(ttl_state, ttl_notifier, ttl_wake, i18n()).await;
            log::warn!("[notifications] TTL expiration loop exited");
        });

        Ok(plugin)
    })
}
