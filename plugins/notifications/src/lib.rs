//! Notifications plugin — daemon-based freedesktop.org notification server.
//!
//! Provides `notification` entities (one per active notification) and a `dnd`
//! (Do Not Disturb) entity via the entity-based daemon architecture.
//!
//! Owns `org.freedesktop.Notifications` on the session bus and translates
//! D-Bus notifications into entities for the waft daemon.

use std::sync::{Arc, Mutex as StdMutex};

use log::{debug, info, warn};
use waft_plugin::*;
use waft_protocol::entity::notification as proto;
use waft_protocol::entity::notification_filter::{
    self as filter_proto, ActiveProfile, NotificationGroup, NotificationProfile,
    SoundConfigEntity, SOUND_CONFIG_ENTITY_TYPE,
};
use waft_protocol::entity::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE;

use self::dbus::client::{OutboundEvent, close_reasons};
use self::dbus::ingress::IngressedNotification;
use self::filter::{CompiledGroup, compile_groups};
use self::store::{NotificationOp, State, process_op};
use self::types::{NotificationIcon, NotificationUrgency};

pub mod config;
pub mod dbus;
pub mod filter;
pub mod recording;
pub mod sound;
pub mod store;
pub mod ttl;
pub mod types;

/// Result of evaluating filter rules for a notification.
#[derive(Debug, Default)]
pub struct FilterActions {
    pub hide: bool,
    pub no_toast: bool,
    pub no_sound: bool,
    pub sound: Option<String>,
}

/// Shareable handle for notification filtering in the ingress monitor.
///
/// Holds cloned `Arc` references to the same data as `NotificationsPlugin`,
/// allowing the ingress task to call `match_notification()` and `get_filter_actions()`
/// without requiring a reference to the plugin itself (which is consumed by `PluginRuntime`).
#[derive(Clone)]
pub struct FilterHandle {
    compiled_matchers: Arc<StdMutex<Vec<CompiledGroup>>>,
    profiles: Arc<StdMutex<Vec<NotificationProfile>>>,
    active_profile_id: Arc<StdMutex<String>>,
}

impl FilterHandle {
    /// Match a notification against configured groups. Returns the ID of the first matching group.
    pub fn match_notification(&self, notification: &IngressedNotification) -> Option<String> {
        let compiled = self.compiled_matchers.lock_or_recover();

        for group in compiled.iter() {
            if filter::matches_combinator(&group.matcher, notification, &group.regex_cache) {
                return Some(group.id.clone());
            }
        }

        None
    }

    /// Get filter actions for a notification based on matched group and active profile.
    pub fn get_filter_actions(&self, group_id: Option<&str>) -> FilterActions {
        let Some(group_id) = group_id else {
            return FilterActions::default();
        };

        let active_profile_id = self.active_profile_id.lock_or_recover().clone();

        let profiles = self.profiles.lock_or_recover();

        let profile = profiles.iter().find(|p| p.id == active_profile_id);
        let Some(profile) = profile else {
            return FilterActions::default();
        };

        let rule = profile.rules.get(group_id);
        let Some(rule) = rule else {
            return FilterActions::default();
        };

        FilterActions {
            hide: rule.hide == filter_proto::RuleValue::On,
            no_toast: rule.no_toast == filter_proto::RuleValue::On,
            no_sound: rule.no_sound == filter_proto::RuleValue::On,
            sound: rule.sound.clone(),
        }
    }
}

/// Notifications plugin implementing the entity-based `Plugin` trait.
pub struct NotificationsPlugin {
    state: Arc<StdMutex<State>>,
    outbound_tx: flume::Sender<OutboundEvent>,
    groups: Arc<StdMutex<Vec<NotificationGroup>>>,
    profiles: Arc<StdMutex<Vec<NotificationProfile>>>,
    active_profile_id: Arc<StdMutex<String>>,
    compiled_matchers: Arc<StdMutex<Vec<CompiledGroup>>>,
    sound_config: Arc<StdMutex<config::SoundConfig>>,
    sound_policy: Arc<StdMutex<sound::policy::SoundPolicy>>,
    sound_gallery: Arc<StdMutex<sound::gallery::SoundGallery>>,
    sound_player: Arc<sound::player::SoundPlayer>,
    claim_sender: Arc<StdMutex<Option<waft_plugin::ClaimSender>>>,
    recorder: Arc<recording::NotificationRecorder>,
    i18n: &'static waft_i18n::I18n,
}

impl NotificationsPlugin {
    /// Create a new plugin instance with filter and sound configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state: Arc<StdMutex<State>>,
        outbound_tx: flume::Sender<OutboundEvent>,
        groups: Vec<NotificationGroup>,
        profiles: Vec<NotificationProfile>,
        active_profile_id: String,
        sound_cfg: config::SoundConfig,
        recording: bool,
        i18n: &'static waft_i18n::I18n,
    ) -> Self {
        let compiled_matchers = compile_groups(&groups);
        let policy = sound::policy::SoundPolicy::new(sound_cfg.clone());
        let gallery = sound::gallery::SoundGallery::scan();

        Self {
            state,
            outbound_tx,
            groups: Arc::new(StdMutex::new(groups)),
            profiles: Arc::new(StdMutex::new(profiles)),
            active_profile_id: Arc::new(StdMutex::new(active_profile_id)),
            compiled_matchers: Arc::new(StdMutex::new(compiled_matchers)),
            sound_config: Arc::new(StdMutex::new(sound_cfg)),
            sound_policy: Arc::new(StdMutex::new(policy)),
            sound_gallery: Arc::new(StdMutex::new(gallery)),
            sound_player: Arc::new(sound::player::SoundPlayer::new()),
            claim_sender: Arc::new(StdMutex::new(None)),
            recorder: Arc::new(recording::NotificationRecorder::new(recording)),
            i18n,
        }
    }

    /// Get a shareable sound policy handle for use in the ingress monitor task.
    pub fn sound_policy_handle(&self) -> Arc<StdMutex<sound::policy::SoundPolicy>> {
        self.sound_policy.clone()
    }

    /// Get a shareable filter handle for use in the ingress monitor task.
    ///
    /// Must be called before passing the plugin to `PluginRuntime::new()`,
    /// since the runtime consumes the plugin.
    pub fn filter_handle(&self) -> FilterHandle {
        FilterHandle {
            compiled_matchers: self.compiled_matchers.clone(),
            profiles: self.profiles.clone(),
            active_profile_id: self.active_profile_id.clone(),
        }
    }

    /// Get a shareable recorder handle for use in the ingress monitor task.
    ///
    /// Must be called before passing the plugin to `PluginRuntime::new()`,
    /// since the runtime consumes the plugin.
    pub fn recorder(&self) -> Arc<recording::NotificationRecorder> {
        self.recorder.clone()
    }

    /// Ingest a notification from the D-Bus server into the store.
    ///
    /// Called from the ingress monitor task when a `Notify` D-Bus call arrives.
    pub fn process_ingress(&self, notification: IngressedNotification) {
        let mut guard = self.state.lock_or_recover();
        process_op(&mut guard, NotificationOp::Ingress(Box::new(notification)), self.i18n);
    }

    /// Process a CloseNotification D-Bus call.
    pub fn process_close(&self, id: u32) {
        let mut guard = self.state.lock_or_recover();
        process_op(&mut guard, NotificationOp::NotificationRetract(id as u64), self.i18n);
        // Emit the close signal on the D-Bus side
        if self
            .outbound_tx
            .send(OutboundEvent::NotificationClosed {
                id,
                reason: close_reasons::CLOSED_BY_CALL,
            })
            .is_err()
        {
            warn!("[notifications] outbound channel closed on CloseNotification");
        }
    }

    /// Match a notification against configured groups. Returns the ID of the first matching group.
    pub fn match_notification(&self, notification: &IngressedNotification) -> Option<String> {
        self.filter_handle().match_notification(notification)
    }

    /// Get filter actions for a notification based on matched group and active profile.
    pub fn get_filter_actions(&self, group_id: Option<&str>) -> FilterActions {
        self.filter_handle().get_filter_actions(group_id)
    }

    /// Rebuild compiled matchers from the current groups.
    fn rebuild_matchers(&self) {
        let groups = self.groups.lock_or_recover();
        let compiled = compile_groups(&groups);

        let mut matchers_guard = self.compiled_matchers.lock_or_recover();
        *matchers_guard = compiled;
    }

    /// Write current sound config to TOML config file.
    fn sync_sound_config_to_toml(&self) -> anyhow::Result<()> {
        let sound_cfg = self.sound_config.lock_or_recover().clone();

        filter::toml_sync::write_sound_config(&sound_cfg)
    }

    /// Write current filter config to TOML config file.
    fn sync_config_to_toml(&self) -> anyhow::Result<()> {
        let groups = self.groups.lock_or_recover().clone();
        let profiles = self.profiles.lock_or_recover().clone();

        filter::toml_sync::write_filter_config(&groups, &profiles)
    }
}

#[async_trait::async_trait]
impl Plugin for NotificationsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let guard = self.state.lock_or_recover();

        let mut entities = Vec::new();

        // DND entity
        let dnd = proto::Dnd { active: guard.dnd };
        entities.push(Entity::new(
            Urn::new("notifications", proto::DND_ENTITY_TYPE, "default"),
            proto::DND_ENTITY_TYPE,
            &dnd,
        ));

        // Recording entity
        entities.push(Entity::new(
            Urn::new("notifications", proto::RECORDING_ENTITY_TYPE, "default"),
            proto::RECORDING_ENTITY_TYPE,
            &proto::Recording {
                active: self.recorder.is_active(),
            },
        ));

        // One entity per visible panel notification
        for (id, _lifecycle) in &guard.panel_notifications {
            let Some(notif) = guard.notifications.get(id) else {
                continue;
            };

            let proto_notif = proto::Notification {
                title: notif.title.to_string(),
                description: notif.description.to_string(),
                app_name: notif
                    .app
                    .as_ref()
                    .and_then(|a| a.title.as_ref())
                    .map(|t| t.to_string()),
                app_id: notif.app.as_ref().map(|a| a.ident.to_string()),
                urgency: match notif.urgency {
                    NotificationUrgency::Low => proto::NotificationUrgency::Low,
                    NotificationUrgency::Normal => proto::NotificationUrgency::Normal,
                    NotificationUrgency::Critical => proto::NotificationUrgency::Critical,
                },
                actions: notif
                    .actions
                    .iter()
                    .map(|a| proto::NotificationAction {
                        key: a.key.to_string(),
                        label: a.label.to_string(),
                    })
                    .collect(),
                icon_hints: notif
                    .icon_hints
                    .iter()
                    .map(|h| match h {
                        NotificationIcon::Bytes(b) => proto::NotificationIconHint::Bytes(b.clone()),
                        NotificationIcon::FilePath(p) => {
                            proto::NotificationIconHint::FilePath(p.display().to_string())
                        }
                        NotificationIcon::Themed(name) => {
                            proto::NotificationIconHint::Themed(name.to_string())
                        }
                    })
                    .collect(),
                created_at_ms: notif
                    .created_at
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
                resident: notif.resident,
                workspace: notif.workspace.as_ref().map(|w| w.to_string()),
                suppress_toast: notif.suppress_toast,
                ttl: notif.ttl,
            };

            entities.push(Entity::new(
                Urn::new(
                    "notifications",
                    proto::NOTIFICATION_ENTITY_TYPE,
                    &id.to_string(),
                ),
                proto::NOTIFICATION_ENTITY_TYPE,
                &proto_notif,
            ));
        }

        // Drop the state lock before acquiring filter locks
        drop(guard);

        // Sound config entity
        {
            let sound_cfg = self.sound_config.lock_or_recover();

            entities.push(Entity::new(
                Urn::new("notifications", SOUND_CONFIG_ENTITY_TYPE, "default"),
                SOUND_CONFIG_ENTITY_TYPE,
                &SoundConfigEntity {
                    enabled: sound_cfg.enabled,
                    default_low: sound_cfg.urgency.low.clone(),
                    default_normal: sound_cfg.urgency.normal.clone(),
                    default_critical: sound_cfg.urgency.critical.clone(),
                },
            ));
        }

        // Notification groups
        {
            let groups = self.groups.lock_or_recover();

            for group in groups.iter() {
                entities.push(Entity::new(
                    Urn::new(
                        "notifications",
                        filter_proto::NOTIFICATION_GROUP_ENTITY_TYPE,
                        &group.id,
                    ),
                    filter_proto::NOTIFICATION_GROUP_ENTITY_TYPE,
                    group,
                ));
            }
        }

        // Notification profiles
        {
            let profiles = self.profiles.lock_or_recover();

            for profile in profiles.iter() {
                entities.push(Entity::new(
                    Urn::new(
                        "notifications",
                        filter_proto::NOTIFICATION_PROFILE_ENTITY_TYPE,
                        &profile.id,
                    ),
                    filter_proto::NOTIFICATION_PROFILE_ENTITY_TYPE,
                    profile,
                ));
            }
        }

        // Active profile
        {
            let active_profile_id = self.active_profile_id.lock_or_recover().clone();

            entities.push(Entity::new(
                Urn::new(
                    "notifications",
                    filter_proto::ACTIVE_PROFILE_ENTITY_TYPE,
                    "current",
                ),
                filter_proto::ACTIVE_PROFILE_ENTITY_TYPE,
                &ActiveProfile {
                    profile_id: active_profile_id,
                },
            ));
        }

        // Sound gallery
        {
            let gallery = self.sound_gallery.lock_or_recover();

            for sound in gallery.sounds() {
                entities.push(Entity::new(
                    Urn::new(
                        "notifications",
                        NOTIFICATION_SOUND_ENTITY_TYPE,
                        &sound.filename,
                    ),
                    NOTIFICATION_SOUND_ENTITY_TYPE,
                    sound,
                ));
            }
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let parts: Vec<&str> = urn.as_str().split('/').collect();
        // URN format: notifications/{entity-type}/{id}
        let entity_type = parts.get(1).copied().unwrap_or("");
        let entity_id = parts.get(2).copied().unwrap_or("");

        match (entity_type, action.as_str()) {
            ("dnd", "toggle") => {
                let mut guard = self.state.lock_or_recover();
                let new_dnd = !guard.dnd;
                process_op(&mut guard, NotificationOp::SetDnd(new_dnd), self.i18n);
                info!("[notifications] DND toggled to {new_dnd}");
            }

            ("recording", "toggle") => {
                let new_active = !self.recorder.is_active();
                self.recorder.set_active(new_active);
                info!("[notifications] recording toggled to {new_active}");
            }

            ("notification", "dismiss") => {
                let id: u64 = entity_id.parse().map_err(|e| {
                    anyhow::anyhow!("invalid notification id: {e}")
                })?;

                {
                    let mut guard = self.state.lock_or_recover();
                    process_op(&mut guard, NotificationOp::NotificationDismiss(id), self.i18n);
                }

                if self
                    .outbound_tx
                    .send(OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: close_reasons::DISMISSED_BY_USER,
                    })
                    .is_err()
                {
                    warn!("[notifications] outbound channel closed on dismiss");
                }
            }

            ("notification", "expire") => {
                let id: u64 = entity_id.parse().map_err(|e| {
                    anyhow::anyhow!("invalid notification id: {e}")
                })?;

                // Check if notification still exists before initiating claim
                {
                    let guard = self.state.lock_or_recover();
                    if guard.get_notification(&id).is_none() {
                        debug!(
                            "[notifications] expire: notification {id} already gone, skip claim"
                        );
                        return Ok(serde_json::Value::Null);
                    }
                }

                // Request claim check -- daemon will ask other subscribers if they still want it
                let claim_sender = self.claim_sender.lock_or_recover().clone();

                if let Some(ref sender) = claim_sender {
                    let _claim_id = sender.request(urn.clone()).await;
                    info!(
                        "[notifications] expire: initiated claim check for notification {id}"
                    );
                } else {
                    // No claim sender (runtime not attached), fall back to dismiss
                    warn!(
                        "[notifications] expire: no claim sender, falling back to dismiss for {id}"
                    );
                    let mut guard = self.state.lock_or_recover();
                    process_op(&mut guard, NotificationOp::NotificationDismiss(id), self.i18n);
                    if self
                        .outbound_tx
                        .send(OutboundEvent::NotificationClosed {
                            id: id as u32,
                            reason: close_reasons::EXPIRED,
                        })
                        .is_err()
                    {
                        warn!("[notifications] outbound channel closed in expire fallback");
                    }
                }
            }

            ("notification", "invoke-action") => {
                let id: u64 = entity_id.parse().map_err(|e| {
                    anyhow::anyhow!("invalid notification id: {e}")
                })?;

                let action_key = params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();

                // Remove notification from store
                {
                    let mut guard = self.state.lock_or_recover();
                    process_op(&mut guard, NotificationOp::NotificationDismiss(id), self.i18n);
                }

                // Emit ActionInvoked + NotificationClosed signals
                if self
                    .outbound_tx
                    .send(OutboundEvent::ActionInvoked {
                        id: id as u32,
                        action_key,
                    })
                    .is_err()
                {
                    warn!("[notifications] outbound channel closed on invoke-action");
                }
                if self
                    .outbound_tx
                    .send(OutboundEvent::NotificationClosed {
                        id: id as u32,
                        reason: close_reasons::DISMISSED_BY_USER,
                    })
                    .is_err()
                {
                    warn!("[notifications] outbound channel closed on invoke-action close");
                }
            }

            // --- Sound config actions ---

            ("sound-config", "update-sound-config") => {
                let entity: SoundConfigEntity = serde_json::from_value(params)?;

                let new_config = {
                    let existing = self.sound_config.lock_or_recover();
                    config::SoundConfig {
                        enabled: entity.enabled,
                        urgency: config::UrgencySounds {
                            low: entity.default_low,
                            normal: entity.default_normal,
                            critical: entity.default_critical,
                        },
                        rules: existing.rules.clone(),
                    }
                };

                // Update sound config
                {
                    let mut guard = self.sound_config.lock_or_recover();
                    *guard = new_config.clone();
                }

                // Rebuild sound policy
                {
                    let mut guard = self.sound_policy.lock_or_recover();
                    *guard = sound::policy::SoundPolicy::new(new_config);
                }

                if let Err(e) = self.sync_sound_config_to_toml() {
                    warn!("[notifications] failed to write sound config: {e}");
                }

                info!("[notifications] updated sound config");
            }

            // --- Filter config actions ---

            ("active-profile", "set-profile") => {
                let profile_id = params
                    .get("profile_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing profile_id"))?
                    .to_string();

                {
                    let mut guard = self.active_profile_id.lock_or_recover();
                    *guard = profile_id.clone();
                }

                if let Err(e) = filter::save_active_profile(&profile_id) {
                    warn!("[notifications] failed to save active profile: {e}");
                }

                info!("[notifications] active profile set to {profile_id}");
            }

            (_, "create-group") => {
                let group: NotificationGroup = serde_json::from_value(params)?;
                let group_id = group.id.clone();

                {
                    let mut groups_guard = self.groups.lock_or_recover();
                    groups_guard.push(group);
                }

                self.rebuild_matchers();

                if let Err(e) = self.sync_config_to_toml() {
                    warn!("[notifications] failed to write config: {e}");
                }

                info!("[notifications] created group {group_id}");
            }

            ("notification-group", "update-group") => {
                let group: NotificationGroup = serde_json::from_value(params)?;

                {
                    let mut groups_guard = self.groups.lock_or_recover();

                    if let Some(existing) = groups_guard.iter_mut().find(|g| g.id == entity_id) {
                        *existing = group.clone();
                    } else {
                        anyhow::bail!("group not found");
                    }
                }

                self.rebuild_matchers();

                if let Err(e) = self.sync_config_to_toml() {
                    warn!("[notifications] failed to write config: {e}");
                }

                info!("[notifications] updated group {}", group.id);
            }

            ("notification-group", "delete-group") => {
                {
                    let mut groups_guard = self.groups.lock_or_recover();
                    groups_guard.retain(|g| g.id != entity_id);
                }

                {
                    let mut profiles_guard = self.profiles.lock_or_recover();

                    for profile in profiles_guard.iter_mut() {
                        profile.rules.remove(entity_id);
                    }
                }

                self.rebuild_matchers();

                if let Err(e) = self.sync_config_to_toml() {
                    warn!("[notifications] failed to write config: {e}");
                }

                info!("[notifications] deleted group {entity_id}");
            }

            (_, "create-profile") => {
                let profile: NotificationProfile = serde_json::from_value(params)?;
                let profile_id = profile.id.clone();

                {
                    let mut profiles_guard = self.profiles.lock_or_recover();
                    profiles_guard.push(profile);
                }

                if let Err(e) = self.sync_config_to_toml() {
                    warn!("[notifications] failed to write config: {e}");
                }

                info!("[notifications] created profile {profile_id}");
            }

            ("notification-profile", "update-profile") => {
                let profile: NotificationProfile = serde_json::from_value(params)?;

                {
                    let mut profiles_guard = self.profiles.lock_or_recover();

                    if let Some(existing) =
                        profiles_guard.iter_mut().find(|p| p.id == entity_id)
                    {
                        *existing = profile.clone();
                    } else {
                        anyhow::bail!("profile not found");
                    }
                }

                if let Err(e) = self.sync_config_to_toml() {
                    warn!("[notifications] failed to write config: {e}");
                }

                info!("[notifications] updated profile {}", profile.id);
            }

            ("notification-profile", "delete-profile") => {
                {
                    let mut profiles_guard = self.profiles.lock_or_recover();
                    profiles_guard.retain(|p| p.id != entity_id);
                }

                if let Err(e) = self.sync_config_to_toml() {
                    warn!("[notifications] failed to write config: {e}");
                }

                info!("[notifications] deleted profile {entity_id}");
            }

            // --- Sound gallery actions ---

            ("notification-sound", "add-sound") => {
                let filename = params
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing filename"))?
                    .to_string();
                let data_b64 = params
                    .get("data")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing data"))?;

                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD
                    .decode(data_b64)
                    .map_err(|e| anyhow::anyhow!("invalid base64: {e}"))?;

                {
                    let mut gallery = self.sound_gallery.lock_or_recover();
                    gallery.add_sound(&filename, &data)?;
                }

                info!("[notifications] added sound to gallery: {filename}");
            }

            ("notification-sound", "remove-sound") => {
                {
                    let mut gallery = self.sound_gallery.lock_or_recover();
                    gallery.remove_sound(entity_id)?;
                }

                info!("[notifications] removed sound from gallery: {entity_id}");
            }

            (_, "preview-sound") => {
                let reference = params
                    .get("reference")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing reference"))?;

                let resolved = sound::gallery::resolve_sound_reference(reference);
                let player = self.sound_player.clone();
                tokio::spawn(async move {
                    player.play(&resolved).await;
                });

                info!("[notifications] previewing sound: {reference}");
            }

            _ => {
                debug!(
                    "[notifications] Unknown action '{}' on entity type '{}'",
                    action, entity_type
                );
            }
        }

        Ok(serde_json::Value::Null)
    }

    fn can_stop(&self) -> bool {
        // Must keep running to receive D-Bus notifications
        false
    }

    fn set_claim_sender(&self, sender: waft_plugin::ClaimSender) {
        *self.claim_sender.lock_or_recover() = Some(sender);
    }

    async fn handle_claim_result(
        &self,
        urn: Urn,
        _claim_id: uuid::Uuid,
        claimed: bool,
    ) {
        if claimed {
            debug!("[notifications] ClaimResult: {urn} claimed by a subscriber, keeping");
            return;
        }

        // Parse notification ID from URN (format: notifications/notification/{id})
        let parts: Vec<&str> = urn.as_str().split('/').collect();
        let id: u64 = match parts.get(2).and_then(|s| s.parse().ok()) {
            Some(id) => id,
            None => {
                warn!(
                    "[notifications] ClaimResult: cannot parse notification id from {urn}"
                );
                return;
            }
        };

        info!("[notifications] ClaimResult: {urn} not claimed, removing");

        {
            let mut guard = self.state.lock_or_recover();
            process_op(&mut guard, NotificationOp::NotificationDismiss(id), self.i18n);
        }

        // Emit D-Bus NotificationClosed(EXPIRED) -- reason code 1
        if self
            .outbound_tx
            .send(OutboundEvent::NotificationClosed {
                id: id as u32,
                reason: close_reasons::EXPIRED,
            })
            .is_err()
        {
            warn!("[notifications] outbound channel closed in handle_claim_result");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::ingress::IngressedNotification;
    use std::sync::Arc;
    use std::time::SystemTime;

    fn make_notification(id: u64) -> IngressedNotification {
        IngressedNotification {
            app_name: Some(Arc::from("test-app")),
            actions: vec![Arc::from("default"), Arc::from("Open")],
            created_at: SystemTime::now(),
            description: Arc::from("Test body"),
            icon: Some(Arc::from("dialog-information")),
            id,
            hints: Default::default(),
            replaces_id: None,
            title: Arc::from("Test Title"),
            ttl: None,
        }
    }

    fn test_i18n() -> &'static waft_i18n::I18n {
        use std::sync::LazyLock;
        static TEST_I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| {
            waft_i18n::I18n::new(&[(
                "en-US",
                "device-group-devices = Devices\ndevice-group-network = Network Devices\ndevice-group-power = Power Devices\ndevice-group-audio = Audio Devices",
            )])
        });
        &TEST_I18N
    }

    fn make_plugin(state: Arc<StdMutex<State>>, tx: flume::Sender<OutboundEvent>) -> NotificationsPlugin {
        NotificationsPlugin::new(state, tx, Vec::new(), Vec::new(), String::new(), config::SoundConfig::default(), false, test_i18n())
    }

    #[test]
    fn get_entities_returns_dnd_when_empty() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = make_plugin(state, tx);
        let entities = plugin.get_entities();
        // DND + active-profile
        assert!(entities.iter().any(|e| e.entity_type == "dnd"));
        assert!(entities.iter().any(|e| e.entity_type == "active-profile"));
    }

    #[test]
    fn get_entities_returns_notification_entities() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = make_plugin(state.clone(), tx);

        // Ingest a notification
        plugin.process_ingress(make_notification(42));

        let entities = plugin.get_entities();

        let notif_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == "notification")
            .collect();
        assert_eq!(notif_entities.len(), 1);
        assert_eq!(
            notif_entities[0].urn.as_str(),
            "notifications/notification/42"
        );
    }

    #[test]
    fn can_stop_returns_false() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = make_plugin(state, tx);
        assert!(!plugin.can_stop());
    }

    #[tokio::test]
    async fn handle_action_dnd_toggle() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = make_plugin(state.clone(), tx);

        assert!(!state.lock().unwrap().dnd);

        plugin
            .handle_action(
                Urn::new("notifications", "dnd", "default"),
                "toggle".to_string(),
                serde_json::Value::Null,
            )
            .await
            .unwrap();

        assert!(state.lock().unwrap().dnd);

        // Toggle again
        plugin
            .handle_action(
                Urn::new("notifications", "dnd", "default"),
                "toggle".to_string(),
                serde_json::Value::Null,
            )
            .await
            .unwrap();

        assert!(!state.lock().unwrap().dnd);
    }

    #[tokio::test]
    async fn handle_action_dismiss() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, rx) = flume::unbounded();
        let plugin = make_plugin(state.clone(), tx);

        plugin.process_ingress(make_notification(10));
        assert!(state.lock().unwrap().notifications.contains_key(&10));

        plugin
            .handle_action(
                Urn::new("notifications", "notification", "10"),
                "dismiss".to_string(),
                serde_json::Value::Null,
            )
            .await
            .unwrap();

        assert!(!state.lock().unwrap().notifications.contains_key(&10));

        // Should have emitted NotificationClosed
        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OutboundEvent::NotificationClosed {
                id: 10,
                reason: close_reasons::DISMISSED_BY_USER
            }
        ));
    }

    #[tokio::test]
    async fn handle_action_invoke_action() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, rx) = flume::unbounded();
        let plugin = make_plugin(state.clone(), tx);

        plugin.process_ingress(make_notification(20));

        plugin
            .handle_action(
                Urn::new("notifications", "notification", "20"),
                "invoke-action".to_string(),
                serde_json::json!({"key": "default"}),
            )
            .await
            .unwrap();

        assert!(!state.lock().unwrap().notifications.contains_key(&20));

        // Should have emitted ActionInvoked then NotificationClosed
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(
            event1,
            OutboundEvent::ActionInvoked { id: 20, .. }
        ));
        let event2 = rx.try_recv().unwrap();
        assert!(matches!(
            event2,
            OutboundEvent::NotificationClosed { id: 20, .. }
        ));
    }

    #[test]
    fn process_close_removes_and_signals() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, rx) = flume::unbounded();
        let plugin = make_plugin(state.clone(), tx);

        plugin.process_ingress(make_notification(30));
        plugin.process_close(30);

        assert!(!state.lock().unwrap().notifications.contains_key(&30));

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OutboundEvent::NotificationClosed {
                id: 30,
                reason: close_reasons::CLOSED_BY_CALL
            }
        ));
    }

    #[test]
    fn match_notification_returns_none_without_groups() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = make_plugin(state, tx);

        let notif = make_notification(1);
        assert_eq!(plugin.match_notification(&notif), None);
    }

    #[test]
    fn match_notification_returns_group_id_on_match() {
        use waft_protocol::entity::notification_filter::*;

        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();

        let groups = vec![NotificationGroup {
            id: "test-apps".to_string(),
            name: "Test Apps".to_string(),
            order: 1,
            matcher: RuleCombinator {
                operator: CombinatorOperator::And,
                children: vec![RuleNode::Pattern(Pattern {
                    field: MatchField::AppName,
                    operator: MatchOperator::Contains,
                    value: "test".to_string(),
                })],
            },
        }];

        let plugin = NotificationsPlugin::new(
            state,
            tx,
            groups,
            Vec::new(),
            String::new(),
            config::SoundConfig::default(),
            false,
            test_i18n(),
        );

        let notif = make_notification(1); // app_name = "test-app"
        assert_eq!(
            plugin.match_notification(&notif),
            Some("test-apps".to_string())
        );
    }

    #[test]
    fn get_filter_actions_returns_defaults_without_profile() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = make_plugin(state, tx);

        let actions = plugin.get_filter_actions(Some("some-group"));
        assert!(!actions.hide);
        assert!(!actions.no_toast);
        assert!(!actions.no_sound);
    }

    #[test]
    fn get_filter_actions_applies_profile_rules() {
        use std::collections::HashMap;
        use waft_protocol::entity::notification_filter::*;

        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();

        let mut rules = HashMap::new();
        rules.insert(
            "test-group".to_string(),
            GroupRule {
                hide: RuleValue::On,
                no_toast: RuleValue::Off,
                no_sound: RuleValue::On,
                sound: None,
            },
        );

        let profiles = vec![NotificationProfile {
            id: "work".to_string(),
            name: "Work".to_string(),
            rules,
        }];

        let plugin = NotificationsPlugin::new(
            state,
            tx,
            Vec::new(),
            profiles,
            "work".to_string(),
            config::SoundConfig::default(),
            false,
            test_i18n(),
        );

        let actions = plugin.get_filter_actions(Some("test-group"));
        assert!(actions.hide);
        assert!(!actions.no_toast);
        assert!(actions.no_sound);
        assert_eq!(actions.sound, None);
    }
}
