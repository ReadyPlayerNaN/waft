/*!
Notifications widget with grouped notifications and controls.

This module provides a reusable notifications widget that includes:
- Grouped notifications with app names, summaries, bodies, and actions
- Do Not Disturb toggle switch
- Clear all notifications button

The widget follows Adwaita design patterns and integrates with the main overlay UI.
*/

use adw::prelude::*;

use std::rc::Rc;

/// Represents a single notification with its data
pub struct Notification {
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub on_default_action: Option<Rc<dyn Fn() + 'static>>,
}

impl Notification {
    pub fn new(app_name: String, summary: String, body: String, actions: Vec<String>) -> Self {
        Self {
            app_name,
            summary,
            body,
            actions,
            on_default_action: None,
        }
    }

    pub fn with_default_action<F: Fn() + 'static>(mut self, action: F) -> Self {
        self.on_default_action = Some(Rc::new(action));
        self
    }
}

/// Build a complete notifications section with controls and custom data
pub fn build_notifications_section(notifications: Vec<Notification>) -> gtk::Widget {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .vexpand(true)
        .build();

    // Track visible notifications count
    let visible_count = std::rc::Rc::new(std::cell::RefCell::new(notifications.len()));

    // Create header box with title and clear button
    let header_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_bottom(8)
        .build();

    let title_label = gtk::Label::builder()
        .css_classes(["heading"])
        .xalign(0.0)
        .hexpand(true)
        .build();

    let clear_btn = gtk::Button::builder()
        .label("Clear")
        .css_classes(["destructive-action"])
        .build();

    header_box.append(&title_label);
    header_box.append(&clear_btn);

    // Set initial title
    let initial_title = if notifications.is_empty() {
        "Notifications".to_string()
    } else {
        format!("Notifications ({})", notifications.len())
    };
    title_label.set_label(&initial_title);

    // Create clear button for header
    let clear_btn = gtk::Button::builder()
        .label("Clear")
        .css_classes(["destructive-action"])
        .build();

    // Function to update title based on visible notifications
    let update_title = {
        let title_label = title_label.clone();
        let visible_count = visible_count.clone();
        move || {
            let count = *visible_count.borrow();
            let title = if count == 0 {
                "Notifications".to_string()
            } else {
                format!("Notifications ({})", count)
            };
            title_label.set_label(&title);
        }
    };

    // Create scrollable container for notifications with responsive height
    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .css_classes(["notification-scrollable"])
        .build();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

    let notifications_list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();

    scrolled.set_child(Some(&notifications_list));

    // Connect Clear button handler
    clear_btn.connect_clicked({
        let notifications_list = notifications_list.clone();
        let visible_count = visible_count.clone();
        let update_title = update_title.clone();
        move |_| {
            // Hide all notification cards and reset count
            let mut child = notifications_list.first_child();
            while let Some(widget) = child {
                if let Some(box_widget) = widget.downcast_ref::<gtk::Box>() {
                    box_widget.set_visible(false);
                }
                child = widget.next_sibling();
            }
            *visible_count.borrow_mut() = 0;
            update_title();
        }
    });

    // We'll update the height after all notifications are added

    // Helper to add a notification "card".
    let add_notif = {
        let visible_count = visible_count.clone();
        let update_title = update_title.clone();
        move |list: &gtk::Box, notification: &Notification| {
            // Create isolated notification card
            let card = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(["card", "notification-card"])
                .margin_top(0)
                .margin_bottom(0)
                .margin_end(16)
                .build();

            // Header with app name and close button
            let header = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .margin_top(12)
                .margin_start(12)
                .margin_end(12)
                .build();

            let app_badge = gtk::Label::builder()
                .label(&notification.app_name)
                .css_classes(["caption", "dim-label"])
                .xalign(0.0)
                .hexpand(true)
                .build();

            let close_btn = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .css_classes(["flat", "circular", "notification-close"])
                .valign(gtk::Align::Start)
                .build();

            header.append(&app_badge);
            header.append(&close_btn);

            // Main content area (clickable for default action)
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(8)
                .margin_start(12)
                .margin_end(12)
                .margin_bottom(12)
                .css_classes(["notification-content"])
                .build();

            let title = gtk::Label::builder()
                .label(&notification.summary)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["heading"])
                .build();

            let text = gtk::Label::builder()
                .label(&notification.body)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["dim-label"])
                .build();

            content.append(&title);
            content.append(&text);

            card.append(&header);
            card.append(&content);

            // Actions area with darker background
            if !notification.actions.is_empty() {
                let actions_container = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .css_classes(["notification-actions-container"])
                    .margin_top(8)
                    .build();

                let separator = gtk::Separator::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .css_classes(["notification-separator"])
                    .build();

                let actions_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(6)
                    .margin_top(8)
                    .margin_start(12)
                    .margin_end(12)
                    .margin_bottom(12)
                    .build();

                for a in &notification.actions {
                    let b = gtk::Button::builder()
                        .label(a)
                        .css_classes(["pill", "notif-action"])
                        .build();
                    actions_box.append(&b);
                }

                actions_container.append(&separator);
                actions_container.append(&actions_box);
                card.append(&actions_container);
            }

            // Connect click handler for default action
            if let Some(action) = &notification.on_default_action {
                let gesture = gtk::GestureClick::new();
                content.add_controller(gesture.clone());

                let action = Rc::clone(action);
                gesture.connect_pressed(move |_, _, _, _| {
                    action();
                });
            }

            // Connect close button handler
            close_btn.connect_clicked({
                let card = card.clone();
                let visible_count = visible_count.clone();
                let update_title = update_title.clone();
                move |_| {
                    // Remove notification card from the list and update count
                    card.set_visible(false);
                    *visible_count.borrow_mut() -= 1;
                    update_title();
                }
            });

            list.append(&card);
        }
    };

    for notification in notifications {
        add_notif(&notifications_list, &notification);
    }

    root.append(&header_box);
    root.append(&scrolled);

    root.upcast::<gtk::Widget>()
}
