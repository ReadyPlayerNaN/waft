//! Shared notification layout builder.
//!
//! Extracts common layout code from ToastWidget for reuse in NotificationCard.

use std::rc::Rc;

use gtk::prelude::*;

use super::countdown_bar::CountdownBarWidget;
use super::icon::IconWidget;
use crate::features::notifications::types::{NotificationAction, NotificationIcon};

/// Parts of a notification layout that can be customized after building.
pub struct NotificationLayoutParts {
    pub card_box: gtk::Box,
    pub header: gtk::Box,
    pub content_box: gtk::Box,
    pub title_label: gtk::Label,
    pub description_label: gtk::Label,
    pub icon_widget: IconWidget,
    pub close_btn: gtk::Button,
    pub countdown_bar: Option<CountdownBarWidget>,
    pub actions_box: Option<gtk::Box>,
}

/// Configuration for building a notification layout.
pub struct NotificationLayoutConfig {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub icon_hints: Vec<NotificationIcon>,
    pub actions: Vec<NotificationAction>,
    pub css_classes: Vec<&'static str>,
    pub show_close_button: bool,
    /// TTL for countdown bar (None = no countdown bar)
    pub toast_ttl: Option<u64>,
}

impl NotificationLayoutParts {
    /// Build a notification layout with the given configuration.
    /// Returns the layout parts for attaching event handlers.
    pub fn build<A>(config: NotificationLayoutConfig, on_action: A) -> Self
    where
        A: Fn(u64, String) + 'static,
    {
        // Card box
        let card_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_top(8)
            .build();

        for class in &config.css_classes {
            card_box.add_css_class(class);
        }

        // Header
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(16)
            .margin_bottom(16)
            .build();

        // Content box (title + description)
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["notification-content"])
            .hexpand(true)
            .halign(gtk::Align::Fill)
            .build();

        // Title label
        let title_label = gtk::Label::builder()
            .css_classes(["heading"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        title_label.set_markup(&config.title);

        // Description label
        let description_label = gtk::Label::builder()
            .css_classes(["dim-label"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        description_label.set_markup(&config.description);

        content_box.append(&title_label);
        content_box.append(&description_label);

        // Icon widget
        let icon_widget = IconWidget::new(config.icon_hints);

        // Spacer
        let spacer = gtk::Box::builder().hexpand(true).build();

        // Close button
        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "notification-close"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .build();

        if !config.show_close_button {
            close_btn.set_visible(false);
        }

        // Append icon, then content
        header.append(icon_widget.widget());
        header.append(&content_box);
        header.append(&spacer);
        header.append(&close_btn);

        card_box.append(&header);

        // Countdown bar (if TTL is set)
        let countdown_bar = if let Some(ttl) = config.toast_ttl {
            let bar = CountdownBarWidget::new(ttl);
            card_box.append(bar.widget());
            Some(bar)
        } else {
            None
        };

        // Action buttons (if any)
        let on_action = Rc::new(on_action);
        let actions_box = if !config.actions.is_empty() {
            let actions_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_start(16)
                .margin_end(16)
                .margin_bottom(12)
                .halign(gtk::Align::End)
                .build();

            for action in config.actions {
                // Skip the "default" action (clicking the notification itself)
                if action.key.as_ref() == "default" {
                    continue;
                }

                let action_btn = gtk::Button::builder()
                    .label(action.label.as_ref())
                    .css_classes(["notification-action"])
                    .build();

                let action_key = action.key.to_string();
                let on_action_clone = on_action.clone();
                let id = config.id;
                action_btn.connect_clicked(move |_| {
                    on_action_clone(id, action_key.clone());
                });

                actions_box.append(&action_btn);
            }

            // Only append if we have visible buttons (non-default actions)
            if actions_box.first_child().is_some() {
                card_box.append(&actions_box);
                Some(actions_box)
            } else {
                None
            }
        } else {
            None
        };

        Self {
            card_box,
            header,
            content_box,
            title_label,
            description_label,
            icon_widget,
            close_btn,
            countdown_bar,
            actions_box,
        }
    }

    /// Update the title and description labels.
    pub fn update(&self, title: &str, description: &str) {
        self.title_label.set_markup(title);
        self.description_label.set_markup(description);
    }
}
