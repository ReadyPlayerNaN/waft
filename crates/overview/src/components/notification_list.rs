//! Notification list component.
//!
//! Smart container that subscribes to the `notification` entity type, groups
//! notifications by application, and routes actions back to the daemon.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::entity;
use waft_protocol::Urn;

use super::notification_group::{NotificationData, NotificationGroup, NotificationGroupOutput};
use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::menu_state::MenuStore;

/// Displays grouped desktop notifications sorted newest first.
///
/// Notifications are grouped by `app_id`. Each group shows the newest
/// notification and allows expanding to see older ones. Supports dismiss,
/// clear-all, and action invocation via entity callbacks.
pub struct NotificationsComponent {
    container: gtk::Box,
    groups_container: gtk::Box,
    empty_placeholder: gtk::Box,
    groups: Rc<RefCell<HashMap<String, NotificationGroup>>>,
}

impl NotificationsComponent {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<MenuStore>,
    ) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let header = gtk::Label::builder()
            .label("Notifications")
            .css_classes(["title-2"])
            .xalign(0.0)
            .build();
        container.append(&header);

        let groups_container = gtk::Box::new(gtk::Orientation::Vertical, 4);
        container.append(&groups_container);

        // Empty placeholder
        let empty_placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let placeholder_label = gtk::Label::builder()
            .label("No notifications")
            .css_classes(["dim-label"])
            .xalign(0.0)
            .build();
        empty_placeholder.append(&placeholder_label);
        container.append(&empty_placeholder);

        let groups: Rc<RefCell<HashMap<String, NotificationGroup>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Subscribe to notification entity changes
        let store_ref = store.clone();
        let entity_cb = action_callback.clone();
        let groups_ref = groups.clone();
        let groups_container_ref = groups_container.clone();
        let empty_placeholder_ref = empty_placeholder.clone();
        let menu_store_ref = menu_store.clone();

        store.subscribe_type(entity::notification::NOTIFICATION_ENTITY_TYPE, move || {
            let mut entities: Vec<(Urn, entity::notification::Notification)> =
                store_ref.get_entities_typed(entity::notification::NOTIFICATION_ENTITY_TYPE);

            // Sort by created_at_ms descending (newest first)
            entities.sort_by(|a, b| b.1.created_at_ms.cmp(&a.1.created_at_ms));

            // Group by app_id
            let mut grouped: HashMap<String, Vec<(Urn, entity::notification::Notification)>> =
                HashMap::new();
            for (urn, notif) in &entities {
                let group_key = notif
                    .app_id
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string();
                grouped
                    .entry(group_key)
                    .or_default()
                    .push((urn.clone(), notif.clone()));
            }

            // Remove groups no longer present
            let current_group_keys: Vec<String> =
                groups_ref.borrow().keys().cloned().collect();
            for key in &current_group_keys {
                if !grouped.contains_key(key) {
                    if let Some(group) = groups_ref.borrow_mut().remove(key) {
                        groups_container_ref.remove(group.widget());
                    }
                }
            }

            // Create or update groups
            for (group_key, notifs) in &grouped {
                let data: Vec<NotificationData> = notifs
                    .iter()
                    .map(|(urn, notif)| NotificationData {
                        urn: urn.clone(),
                        title: notif.title.clone(),
                        description: notif.description.clone(),
                        icon_hints: notif.icon_hints.clone(),
                        actions: notif.actions.clone(),
                    })
                    .collect();

                let mut groups_map = groups_ref.borrow_mut();

                if let Some(existing) = groups_map.get(group_key) {
                    existing.update(&data);
                } else {
                    // Determine app title and icon from first notification
                    let first = &notifs[0].1;
                    let app_title = first
                        .app_name
                        .as_deref()
                        .unwrap_or(group_key);

                    let group = NotificationGroup::new(
                        group_key,
                        app_title,
                        &first.icon_hints,
                        &menu_store_ref,
                    );

                    // Wire output to entity callbacks
                    let entity_cb_ref = entity_cb.clone();
                    group.connect_output(move |event| match event {
                        NotificationGroupOutput::ActionClick(urn, key) => {
                            entity_cb_ref(
                                urn,
                                "invoke-action".to_string(),
                                serde_json::json!({ "key": key }),
                            );
                        }
                        NotificationGroupOutput::Close(urn) => {
                            entity_cb_ref(urn, "dismiss".to_string(), serde_json::Value::Null);
                        }
                        NotificationGroupOutput::ClearAll(urns) => {
                            for urn in urns {
                                entity_cb_ref(
                                    urn,
                                    "dismiss".to_string(),
                                    serde_json::Value::Null,
                                );
                            }
                        }
                    });

                    group.update(&data);
                    groups_container_ref.append(group.widget());
                    groups_map.insert(group_key.clone(), group);
                }
            }

            // Toggle empty placeholder
            empty_placeholder_ref.set_visible(entities.is_empty());
            groups_container_ref.set_visible(!entities.is_empty());
        });

        Self {
            container,
            groups_container,
            empty_placeholder,
            groups,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}
