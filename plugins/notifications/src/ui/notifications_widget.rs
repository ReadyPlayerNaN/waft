//! Notifications widget for the overlay Info slot.
//!
//! Displays all notifications grouped by application with expand/collapse functionality.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use waft_plugin_api::ui::icon::IconWidget;
use waft_plugin_api::common::Callback;
use waft_core::menu_state::MenuStore;

use super::notification_group::{NotificationData, NotificationGroup, NotificationGroupOutput};
use crate::store::{ItemLifecycle, NotificationStore};

/// Output events from the notifications widget.
#[derive(Debug, Clone)]
pub enum NotificationsWidgetOutput {
    ActionClick(u64, String),
    Dismiss(u64),
    DismissAll(Vec<u64>),
}

/// The main notifications widget for the overlay.
pub struct NotificationsWidget {
    pub root: gtk::Box,
    groups_container: gtk::Box,
    empty_placeholder: gtk::Box,
    groups: Rc<RefCell<HashMap<Arc<str>, NotificationGroup>>>,
    on_output: Callback<NotificationsWidgetOutput>,
    store: Rc<NotificationStore>,
    menu_store: Rc<MenuStore>,
}

impl NotificationsWidget {
    pub fn new(store: Rc<NotificationStore>, menu_store: Rc<MenuStore>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["notifications-widget"])
            .build();

        // Header
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_start(0)
            .margin_end(0)
            .margin_top(16)
            .margin_bottom(8)
            .build();

        let header_label = gtk::Label::builder()
            .label(waft_plugin_api::i18n::t("notifications-title"))
            .css_classes(["title-3"])
            .hexpand(true)
            .xalign(0.0)
            .build();

        header.append(&header_label);
        root.append(&header);

        // Scrolled window for notifications
        // Use propagate_natural_height so the widget sizes to content
        // The parent window will constrain the max height
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true)
            .build();

        // Container for groups
        let groups_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(8)
            .build();

        scrolled.set_child(Some(&groups_container));
        root.append(&scrolled);

        // Empty state placeholder
        let empty_placeholder = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .valign(gtk::Align::Center)
            .margin_top(32)
            .margin_bottom(32)
            .height_request(120)
            .css_classes(["empty-placeholder"])
            .build();

        let empty_icon = IconWidget::from_name("notifications-disabled-symbolic", 48);
        empty_icon.widget().add_css_class("dim-label");

        let empty_label = gtk::Label::builder()
            .label(waft_plugin_api::i18n::t("notifications-empty"))
            .css_classes(["dim-label"])
            .build();

        empty_placeholder.append(empty_icon.widget());
        empty_placeholder.append(&empty_label);
        groups_container.append(&empty_placeholder);

        let groups: Rc<RefCell<HashMap<Arc<str>, NotificationGroup>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let on_output: Callback<NotificationsWidgetOutput> = Rc::new(RefCell::new(None));

        let widget = Self {
            root,
            groups_container,
            empty_placeholder,
            groups,
            on_output,
            store,
            menu_store,
        };

        // Subscribe to store
        widget.setup_subscription();

        widget
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(NotificationsWidgetOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn setup_subscription(&self) {
        let groups = self.groups.clone();
        let groups_container = self.groups_container.clone();
        let empty_placeholder = self.empty_placeholder.clone();
        let on_output = self.on_output.clone();
        let store = self.store.clone();
        let menu_store = self.menu_store.clone();

        self.store.subscribe(move || {
            let state = store.get_state();
            let panel_count = state.panel_notifications.len();
            let notifications_count = state.notifications.len();
            let grouped = state.get_grouped_notifications();

            log::trace!(
                "[notifications_widget] Store update: {} panel_notifications, {} in notifications HashMap, {} groups",
                panel_count,
                notifications_count,
                grouped.len()
            );

            // Log panel_notification IDs for debugging
            if panel_count > 0 {
                let panel_ids: Vec<_> = state.panel_notifications.keys().collect();
                log::trace!("[notifications_widget] Panel notification IDs: {:?}", panel_ids);
            }

            // Log notifications HashMap IDs for debugging
            if notifications_count > 0 {
                let notif_ids: Vec<_> = state.notifications.keys().collect();
                log::trace!("[notifications_widget] Notifications HashMap IDs: {:?}", notif_ids);
            }

            Self::handle_state_changed(
                &grouped,
                &groups,
                &groups_container,
                &empty_placeholder,
                &on_output,
                menu_store.clone(),
            );
        });
    }

    fn handle_state_changed(
        grouped: &HashMap<
            Arc<str>,
            Vec<(
                &crate::store::Notification,
                &ItemLifecycle,
            )>,
        >,
        groups: &Rc<RefCell<HashMap<Arc<str>, NotificationGroup>>>,
        groups_container: &gtk::Box,
        empty_placeholder: &gtk::Box,
        on_output: &Callback<NotificationsWidgetOutput>,
        menu_store: Rc<MenuStore>,
    ) {
        log::debug!(
            "[notifications_widget] State changed, {} groups",
            grouped.len()
        );

        // Get current group IDs
        let current_app_ids: Vec<Arc<str>> = groups.borrow().keys().cloned().collect();
        let new_app_ids: Vec<Arc<str>> = grouped.keys().cloned().collect();

        // Remove groups that are no longer present
        for app_id in &current_app_ids {
            if !new_app_ids.contains(app_id)
                && let Some(group) = groups.borrow_mut().remove(app_id) {
                    groups_container.remove(&group.root);
                    log::debug!("[notifications_widget] Removed group: {}", app_id);
                }
        }

        // Create or update groups
        for (app_id, notifications) in grouped {
            let notification_data: Vec<NotificationData> = notifications
                .iter()
                .map(|(n, l)| NotificationData {
                    id: n.id,
                    title: n.title.clone(),
                    description: n.description.clone(),
                    icon_hints: n.icon_hints.clone(),
                    actions: n.actions.clone(),
                    lifecycle: (*l).clone(),
                })
                .collect();

            let mut groups_ref = groups.borrow_mut();

            if !groups_ref.contains_key(app_id) {
                // Create new group
                let app_title = notifications
                    .first()
                    .map(|(n, _)| n.app_title())
                    .unwrap_or_else(|| Arc::from("Unknown"));

                let icon_hints = notifications
                    .first()
                    .map(|(n, _)| {
                        crate::store::reorder_icon_hints_for_group(
                            &n.icon_hints,
                        )
                    })
                    .unwrap_or_default();

                let group = NotificationGroup::new(
                    app_id.clone(),
                    app_title,
                    icon_hints,
                    menu_store.clone(),
                );

                let on_output_clone = on_output.clone();
                group.connect_output(move |event| {
                    if let Some(ref callback) = *on_output_clone.borrow() {
                        match event {
                            NotificationGroupOutput::ActionClick(id, action_key) => {
                                callback(NotificationsWidgetOutput::ActionClick(id, action_key));
                            }
                            NotificationGroupOutput::ClearAll(ids) => {
                                callback(NotificationsWidgetOutput::DismissAll(ids));
                            }
                            NotificationGroupOutput::Close(id) => {
                                callback(NotificationsWidgetOutput::Dismiss(id));
                            }
                        }
                    }
                });

                group.update(&notification_data);
                groups_container.append(&group.root);
                groups_ref.insert(app_id.clone(), group);

                log::debug!("[notifications_widget] Created group: {}", app_id);
            } else if let Some(group) = groups_ref.get(app_id) {
                // Update existing group
                group.update(&notification_data);
            }
        }

        // Update empty state visibility
        let has_notifications = !grouped.is_empty();
        empty_placeholder.set_visible(!has_notifications);

        log::debug!(
            "[notifications_widget] State update complete, {} groups visible",
            groups.borrow().len()
        );
    }
}
