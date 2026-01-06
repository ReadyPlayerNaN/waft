use adw::prelude::*;
use gtk::gdk;

use super::types::{Notification, NotificationGroup, NotificationIcon, NotificationsSnapshot};

/// GTK view for notifications. Renders a `NotificationsModel` snapshot.
///
/// Notes:
/// - This view is intentionally "dumb": it accepts a snapshot + callbacks and rebuilds UI.
/// - A controller is expected to own the model and call `render_from_snapshot(...)` when needed.
pub struct NotificationsView {
    root: gtk::Widget,

    title_label: gtk::Label,
    clear_btn: gtk::Button,
    groups_list: gtk::Box,

    // Rendering settings
    icon_size: i32,
    // Default icon name for themed icon fallback in the view (if a themed name is missing).
    default_themed_icon: String,
}

impl NotificationsView {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .vexpand(true)
            .build();

        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_bottom(8)
            .build();

        let title_label = gtk::Label::builder()
            .css_classes(["heading"])
            .xalign(0.0)
            .hexpand(true)
            .label("Notifications")
            .build();

        let clear_btn = gtk::Button::builder()
            .label("Clear")
            .css_classes(["destructive-action"])
            .build();

        header_box.append(&title_label);
        header_box.append(&clear_btn);

        let scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .css_classes(["notification-scrollable"])
            .build();
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

        let groups_list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(8)
            .margin_end(16)
            .build();

        scrolled.set_child(Some(&groups_list));

        root.append(&header_box);
        root.append(&scrolled);

        Self {
            root: root.upcast::<gtk::Widget>(),
            title_label,
            clear_btn,
            groups_list,
            icon_size: 32,
            // No preference was specified; pick a reasonable symbolic default.
            default_themed_icon: "dialog-information-symbolic".to_string(),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }

    /// Connect Clear button to the provided handler (typically model.clear() + rerender).
    pub fn connect_clear<F: Fn() + 'static>(&self, f: F) {
        self.clear_btn.connect_clicked(move |_| f());
    }

    /// Render from a model snapshot.
    ///
    /// This rebuilds the list. It is structured so it can be debounced later by
    /// calling it less frequently from the controller.
    pub fn render_from_snapshot<F1, F2, F3>(
        &self,
        snapshot: NotificationsSnapshot,
        on_close_notification: F1,
        on_toggle_group: F2,
        on_close_all_groups: F3,
    ) where
        F1: Fn(u64) + Clone + 'static,
        F2: Fn(String) + Clone + 'static,
        F3: Fn() + Clone + 'static,
    {
        // Title
        if snapshot.total_count == 0 {
            self.title_label.set_label("Notifications");
        } else {
            self.title_label
                .set_label(&format!("Notifications ({})", snapshot.total_count));
        }

        // Clear existing children
        while let Some(child) = self.groups_list.first_child() {
            self.groups_list.remove(&child);
        }

        let open_group = snapshot.open_group.clone();

        for group in snapshot.groups {
            let is_open = open_group.as_deref() == Some(group.app_key.as_str());
            let group_widget = self.build_group_widget(
                &group,
                is_open,
                on_close_notification.clone(),
                on_toggle_group.clone(),
                on_close_all_groups.clone(),
            );
            self.groups_list.append(&group_widget);
        }

        // If list is empty, show an empty-state label.
        if snapshot.total_count == 0 {
            let empty = gtk::Label::builder()
                .label("No notifications")
                .css_classes(["dim-label"])
                .xalign(0.0)
                .margin_top(12)
                .build();
            self.groups_list.append(&empty);
        }
    }

    fn build_group_widget<F1, F2, F3>(
        &self,
        group: &NotificationGroup,
        is_open: bool,
        on_close_notification: F1,
        on_toggle_group: F2,
        _on_close_all_groups: F3,
    ) -> gtk::Widget
    where
        F1: Fn(u64) + Clone + 'static,
        F2: Fn(String) + Clone + 'static,
        F3: Fn() + Clone + 'static,
    {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        // Group header (app name)
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let app_label = gtk::Label::builder()
            .label(&group.display_app_name)
            .css_classes(["heading"])
            .xalign(0.0)
            .hexpand(true)
            .build();

        header.append(&app_label);
        container.append(&header);

        let count = group.notifications.len();
        if count == 0 {
            return container.upcast::<gtk::Widget>();
        }

        // Latest card always shown
        let latest = &group.notifications[0];
        let latest_card = self.build_notification_card(latest, on_close_notification.clone());
        container.append(&latest_card);

        // Expand button (only if more than one)
        let expand_btn = gtk::Button::builder()
            .css_classes(["flat"])
            .margin_bottom(0)
            .label(if is_open {
                "Show less".to_string()
            } else {
                format!("Show {} more", count.saturating_sub(1))
            })
            .halign(gtk::Align::Start)
            .build();

        if count <= 1 {
            expand_btn.set_visible(false);
        } else {
            let app_key = group.app_key.clone();
            expand_btn.connect_clicked(move |_| {
                on_toggle_group(app_key.clone());
            });
        }

        container.append(&expand_btn);

        // Remaining notifications in revealer
        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(is_open && count > 1)
            .build();

        let rest_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        for n in group.notifications.iter().skip(1) {
            let card = self.build_notification_card(n, on_close_notification.clone());
            rest_box.append(&card);
        }

        revealer.set_child(Some(&rest_box));
        container.append(&revealer);

        container.upcast::<gtk::Widget>()
    }

    fn build_notification_card<F>(&self, notification: &Notification, on_close: F) -> gtk::Widget
    where
        F: Fn(u64) + Clone + 'static,
    {
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["card", "notification-card"])
            .build();

        // Header with icon + content + close
        let layout = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(16)
            .margin_bottom(16)
            .build();

        let icon = self.build_icon_image(&notification.icon);
        let spacer = gtk::Box::builder().hexpand(true).build();

        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "notification-close"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .build();

        // Main content (clickable)
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
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

        layout.append(&icon);
        layout.append(&content);
        layout.append(&spacer);
        layout.append(&close_btn);

        content.append(&title);
        content.append(&text);

        card.append(&layout);

        // Actions
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
                .margin_top(6)
                .margin_start(12)
                .margin_end(12)
                .margin_bottom(8)
                .build();

            for a in &notification.actions {
                let b = gtk::Button::builder()
                    .label(&a.label)
                    .css_classes(["pill", "notif-action"])
                    .build();
                let on_invoke = a.on_invoke.clone();
                b.connect_clicked(move |_| (on_invoke)());
                actions_box.append(&b);
            }

            actions_container.append(&separator);
            actions_container.append(&actions_box);
            card.append(&actions_container);
        }

        // Default action click
        if let Some(action) = &notification.on_default_action {
            let gesture = gtk::GestureClick::new();
            content.add_controller(gesture.clone());
            let action = action.clone();
            gesture.connect_pressed(move |_, _, _, _| (action)());
        }

        // Close button
        let id = notification.id;
        close_btn.connect_clicked(move |_| on_close(id));

        card.upcast::<gtk::Widget>()
    }

    fn build_icon_image(&self, icon: &NotificationIcon) -> gtk::Image {
        let img = gtk::Image::builder()
            .pixel_size(self.icon_size)
            .valign(gtk::Align::Start)
            .build();

        match icon {
            NotificationIcon::Themed(name) => {
                let display = match gdk::Display::default() {
                    Some(d) => d,
                    None => {
                        img.set_icon_name(Some(&self.default_themed_icon));
                        return img;
                    }
                };

                let theme = gtk::IconTheme::for_display(&display);
                let mut candidates: Vec<String> = vec![];

                if name.ends_with("-symbolic") {
                    candidates.push(name.clone());
                } else {
                    candidates.push(format!("{name}-symbolic"));
                    candidates.push(name.clone());
                }

                let mut chosen = None;
                for c in candidates {
                    if theme.has_icon(&c) {
                        chosen = Some(c);
                        break;
                    }
                }

                if let Some(chosen) = chosen {
                    img.set_icon_name(Some(&chosen));
                } else {
                    img.set_icon_name(Some(&self.default_themed_icon));
                }
            }
            NotificationIcon::FilePath(path) => {
                // Load and scale-to-fit (distortion allowed per requirement).
                // Note: this is synchronous file IO/decoding.
                if let Ok(tex) = gdk::Texture::from_filename(path) {
                    img.set_paintable(Some(&tex));
                } else {
                    img.set_icon_name(Some(&self.default_themed_icon));
                }
            }
        }

        img
    }
}
