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

use self::dbus::client::{OutboundEvent, close_reasons};
use self::dbus::ingress::IngressedNotification;
use self::store::{NotificationOp, State, process_op};
use self::types::{NotificationIcon, NotificationUrgency};

pub mod dbus;
pub mod store;
pub mod ttl;
pub mod types;

/// Notifications plugin implementing the entity-based `Plugin` trait.
pub struct NotificationsPlugin {
    state: Arc<StdMutex<State>>,
    outbound_tx: flume::Sender<OutboundEvent>,
}

impl NotificationsPlugin {
    /// Create a new plugin instance.
    ///
    /// Returns the plugin and the receiver for outbound D-Bus events
    /// (to be consumed by the D-Bus server's signal loop).
    pub fn new(
        state: Arc<StdMutex<State>>,
        outbound_tx: flume::Sender<OutboundEvent>,
    ) -> Self {
        Self {
            state,
            outbound_tx,
        }
    }

    /// Ingest a notification from the D-Bus server into the store.
    ///
    /// Called from the ingress monitor task when a `Notify` D-Bus call arrives.
    pub fn process_ingress(&self, notification: IngressedNotification) {
        let mut guard = match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[notifications] mutex poisoned in process_ingress, recovering: {e}");
                e.into_inner()
            }
        };
        process_op(
            &mut guard,
            NotificationOp::Ingress(Box::new(notification)),
        );
    }

    /// Process a CloseNotification D-Bus call.
    pub fn process_close(&self, id: u32) {
        let mut guard = match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[notifications] mutex poisoned in process_close, recovering: {e}");
                e.into_inner()
            }
        };
        process_op(&mut guard, NotificationOp::NotificationRetract(id as u64));
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
}

#[async_trait::async_trait]
impl Plugin for NotificationsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let guard = match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[notifications] mutex poisoned in get_entities, recovering: {e}");
                e.into_inner()
            }
        };

        let mut entities = Vec::new();

        // DND entity
        let dnd = proto::Dnd { active: guard.dnd };
        entities.push(Entity::new(
            Urn::new("notifications", proto::DND_ENTITY_TYPE, "default"),
            proto::DND_ENTITY_TYPE,
            &dnd,
        ));

        // One entity per visible panel notification
        for (id, _lifecycle) in &guard.panel_notifications {
            let Some(notif) = guard.notifications.get(id) else {
                continue;
            };

            let proto_notif = proto::Notification {
                title: notif.title.to_string(),
                description: notif.description.to_string(),
                app_name: notif.app.as_ref().and_then(|a| a.title.as_ref()).map(|t| t.to_string()),
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
            };

            entities.push(Entity::new(
                Urn::new("notifications", proto::NOTIFICATION_ENTITY_TYPE, &id.to_string()),
                proto::NOTIFICATION_ENTITY_TYPE,
                &proto_notif,
            ));
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let parts: Vec<&str> = urn.as_str().split('/').collect();
        // URN format: notifications/{entity-type}/{id}
        let entity_type = parts.get(1).copied().unwrap_or("");
        let entity_id = parts.get(2).copied().unwrap_or("");

        match (entity_type, action.as_str()) {
            ("dnd", "toggle") => {
                let mut guard = match self.state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[notifications] mutex poisoned in handle_action, recovering: {e}");
                        e.into_inner()
                    }
                };
                let new_dnd = !guard.dnd;
                process_op(&mut guard, NotificationOp::SetDnd(new_dnd));
                info!("[notifications] DND toggled to {new_dnd}");
            }

            ("notification", "dismiss") => {
                let id: u64 = entity_id.parse().map_err(|e| {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("invalid notification id: {e}"),
                    )) as Box<dyn std::error::Error + Send + Sync>
                })?;

                {
                    let mut guard = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[notifications] mutex poisoned in handle_action, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    process_op(&mut guard, NotificationOp::NotificationDismiss(id));
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

            ("notification", "invoke-action") => {
                let id: u64 = entity_id.parse().map_err(|e| {
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("invalid notification id: {e}"),
                    )) as Box<dyn std::error::Error + Send + Sync>
                })?;

                let action_key = params
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();

                // Remove notification from store
                {
                    let mut guard = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[notifications] mutex poisoned in handle_action, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    process_op(&mut guard, NotificationOp::NotificationDismiss(id));
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

            _ => {
                debug!(
                    "[notifications] Unknown action '{}' on entity type '{}'",
                    action, entity_type
                );
            }
        }

        Ok(())
    }

    fn can_stop(&self) -> bool {
        // Must keep running to receive D-Bus notifications
        false
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

    #[test]
    fn get_entities_returns_dnd_when_empty() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = NotificationsPlugin::new(state, tx);
        let entities = plugin.get_entities();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "dnd");
    }

    #[test]
    fn get_entities_returns_notification_entities() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = NotificationsPlugin::new(state.clone(), tx);

        // Ingest a notification
        plugin.process_ingress(make_notification(42));

        let entities = plugin.get_entities();
        // 1 DND + 1 notification
        assert_eq!(entities.len(), 2);

        let notif_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == "notification")
            .collect();
        assert_eq!(notif_entities.len(), 1);
        assert_eq!(notif_entities[0].urn.as_str(), "notifications/notification/42");
    }

    #[test]
    fn can_stop_returns_false() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = NotificationsPlugin::new(state, tx);
        assert!(!plugin.can_stop());
    }

    #[tokio::test]
    async fn handle_action_dnd_toggle() {
        let state = Arc::new(StdMutex::new(State::new()));
        let (tx, _rx) = flume::unbounded();
        let plugin = NotificationsPlugin::new(state.clone(), tx);

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
        let plugin = NotificationsPlugin::new(state.clone(), tx);

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
        let plugin = NotificationsPlugin::new(state.clone(), tx);

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
        let plugin = NotificationsPlugin::new(state.clone(), tx);

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
}
