//! Notification list component.
//!
//! Subscribes to the `notification` entity type and renders notifications
//! sorted by creation time (newest first). Always visible, showing a
//! placeholder when no notifications exist.

use std::rc::Rc;

use gtk::prelude::*;

use waft_ipc::widget::{Action, ActionParams};
use waft_protocol::entity;
use waft_protocol::entity::notification::NotificationIconHint;
use waft_protocol::Urn;
use waft_ui_gtk::renderer::ActionCallback;
use waft_ui_gtk::widgets::info_card::InfoCardWidget;

use crate::entity_store::{EntityActionCallback, EntityStore};

/// Displays a list of desktop notifications sorted newest first.
///
/// Each notification is rendered as an `InfoCardWidget`. Notifications
/// with a "default" action are clickable. Shows "No notifications"
/// when the list is empty.
pub struct NotificationsComponent {
    container: gtk::Box,
    notifications_container: gtk::Box,
}

impl NotificationsComponent {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let header = gtk::Label::builder()
            .label("Notifications")
            .css_classes(["title-2"])
            .xalign(0.0)
            .build();
        container.append(&header);

        let notifications_container = gtk::Box::new(gtk::Orientation::Vertical, 4);
        container.append(&notifications_container);

        // Show placeholder initially
        let placeholder = gtk::Label::builder()
            .label("No notifications")
            .css_classes(["dim-label"])
            .xalign(0.0)
            .build();
        notifications_container.append(&placeholder);

        // Build an ActionCallback bridge from the EntityActionCallback.
        // InfoCardWidget expects Rc<dyn Fn(String, Action)> where the
        // String is a widget_id we set to the URN string.
        let entity_cb = action_callback.clone();
        let bridge_callback: ActionCallback = Rc::new(move |widget_id, action: Action| {
            match Urn::parse(&widget_id) {
                Ok(urn) => {
                    let params = match &action.params {
                        ActionParams::None => serde_json::Value::Null,
                        ActionParams::Value(v) => serde_json::json!(v),
                        ActionParams::String(s) => serde_json::json!(s),
                        ActionParams::Map(m) => serde_json::json!(m),
                    };
                    entity_cb(urn, action.id.clone(), params);
                }
                Err(e) => {
                    log::warn!(
                        "[notifications] failed to parse URN from widget_id '{}': {e}",
                        widget_id,
                    );
                }
            }
        });

        let store_ref = store.clone();
        let notifications_container_ref = notifications_container.clone();
        let bridge_cb = bridge_callback.clone();

        store.subscribe_type(entity::notification::NOTIFICATION_ENTITY_TYPE, move || {
            let mut entities: Vec<(Urn, entity::notification::Notification)> =
                store_ref.get_entities_typed(entity::notification::NOTIFICATION_ENTITY_TYPE);

            // Sort by created_at_ms descending (newest first)
            entities.sort_by(|a, b| b.1.created_at_ms.cmp(&a.1.created_at_ms));

            // Clear existing children
            while let Some(child) = notifications_container_ref.first_child() {
                notifications_container_ref.remove(&child);
            }

            if entities.is_empty() {
                let placeholder = gtk::Label::builder()
                    .label("No notifications")
                    .css_classes(["dim-label"])
                    .xalign(0.0)
                    .build();
                notifications_container_ref.append(&placeholder);
                return;
            }

            for (urn, notif) in &entities {
                let icon = resolve_notification_icon(&notif.icon_hints);
                let has_default_action = notif.actions.iter().any(|a| a.key == "default");

                let card = if has_default_action {
                    let on_click = Action {
                        id: "invoke-action".to_string(),
                        params: ActionParams::Map(
                            [("key".to_string(), serde_json::json!("default"))]
                                .into_iter()
                                .collect(),
                        ),
                    };
                    InfoCardWidget::new_clickable(
                        &icon,
                        &notif.title,
                        Some(notif.description.as_str()),
                        &bridge_cb,
                        &on_click,
                        urn.as_str(),
                    )
                } else {
                    InfoCardWidget::new(&icon, &notif.title, Some(notif.description.as_str()))
                };

                notifications_container_ref.append(&card.widget());
            }
        });

        Self {
            container,
            notifications_container,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

/// Resolve the best icon from notification icon hints.
///
/// Tries themed icons first (most efficient for GTK), falls back to
/// the default information icon.
fn resolve_notification_icon(hints: &[NotificationIconHint]) -> String {
    hints
        .iter()
        .find_map(|h| match h {
            NotificationIconHint::Themed(name) => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "dialog-information-symbolic".to_string())
}
