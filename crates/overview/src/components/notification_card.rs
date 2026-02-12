//! Individual notification card widget.
//!
//! Renders a single notification with icon, title, description, close button,
//! and optional action buttons. Supports left-click (default action), right-click
//! (dismiss), and animated show/hide via a Revealer.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use waft_protocol::entity::notification::{NotificationAction, NotificationIconHint};
use waft_protocol::Urn;
use waft_ui_gtk::widgets::icon::{Icon, IconWidget};

use super::notification_markup;
use crate::ui::main_window::trigger_window_resize;

/// Output events from a notification card.
#[derive(Debug, Clone)]
pub enum NotificationCardOutput {
    ActionClick(Urn, String),
    Close(Urn),
}

/// A notification card for the panel.
pub struct NotificationCard {
    urn: Urn,
    pub root: gtk::Box,
    revealer: gtk::Revealer,
    title_label: gtk::Label,
    description_label: gtk::Label,
    on_output: Rc<RefCell<Option<Box<dyn Fn(NotificationCardOutput)>>>>,
    hidden: Rc<RefCell<bool>>,
}

impl NotificationCard {
    pub fn new(
        urn: Urn,
        title: &str,
        description: &str,
        icon_hints: &[NotificationIconHint],
        actions: &[NotificationAction],
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(false)
            .build();

        let on_output: Rc<RefCell<Option<Box<dyn Fn(NotificationCardOutput)>>>> =
            Rc::new(RefCell::new(None));
        let hidden = Rc::new(RefCell::new(false));

        // Card box
        let card_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["notification-card", "card"])
            .build();

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

        // Icon (32px)
        let icons = convert_icon_hints(icon_hints);
        let icon_widget = IconWidget::new(icons, 32);

        // Content box
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["notification-content"])
            .hexpand(true)
            .halign(gtk::Align::Fill)
            .build();

        // Title
        let prepared_title = notification_markup::prepare_title(title);
        let title_label = gtk::Label::builder()
            .css_classes(["heading"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        title_label.set_markup(&prepared_title);

        // Description
        let prepared_desc = notification_markup::prepare_description(description);
        let description_label = gtk::Label::builder()
            .css_classes(["dim-label"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        description_label.set_markup(&prepared_desc);

        content_box.append(&title_label);
        content_box.append(&description_label);

        // Spacer
        let spacer = gtk::Box::builder().hexpand(true).build();

        // Close button
        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "notification-close"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .build();

        header.append(icon_widget.widget());
        header.append(&content_box);
        header.append(&spacer);
        header.append(&close_btn);
        card_box.append(&header);

        // Action buttons
        let non_default_actions: Vec<_> =
            actions.iter().filter(|a| a.key != "default").collect();

        if !non_default_actions.is_empty() {
            let actions_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_start(16)
                .margin_end(16)
                .margin_bottom(12)
                .halign(gtk::Align::End)
                .build();

            for action in &non_default_actions {
                let action_btn = gtk::Button::builder()
                    .label(&action.label)
                    .css_classes(["notification-action"])
                    .build();

                let action_key = action.key.clone();
                let urn_clone = urn.clone();
                let on_output_ref = on_output.clone();
                let hidden_ref = hidden.clone();
                action_btn.connect_clicked(move |_| {
                    if *hidden_ref.borrow() {
                        return;
                    }
                    if let Some(ref cb) = *on_output_ref.borrow() {
                        cb(NotificationCardOutput::ActionClick(
                            urn_clone.clone(),
                            action_key.clone(),
                        ));
                    }
                });

                actions_box.append(&action_btn);
            }

            card_box.append(&actions_box);
        }

        revealer.set_child(Some(&card_box));
        root.append(&revealer);

        // Trigger window resize after reveal animation completes so the
        // layer-shell window recalculates its height. The Revealer itself
        // collapses to zero height when not revealing, so we don't need to
        // remove the card from the DOM here — that is handled by the group's
        // update() when the entity store confirms the removal.
        revealer.connect_child_revealed_notify(move |_rev| {
            trigger_window_resize();
        });

        // Close button handler
        {
            let on_output_ref = on_output.clone();
            let revealer_ref = revealer.clone();
            let urn_clone = urn.clone();
            let hidden_ref = hidden.clone();
            close_btn.connect_clicked(move |_| {
                if *hidden_ref.borrow() {
                    return;
                }
                *hidden_ref.borrow_mut() = true;
                revealer_ref.set_reveal_child(false);
                if let Some(ref cb) = *on_output_ref.borrow() {
                    cb(NotificationCardOutput::Close(urn_clone.clone()));
                }
            });
        }

        // Right-click to dismiss
        {
            let on_output_ref = on_output.clone();
            let revealer_ref = revealer.clone();
            let urn_clone = urn.clone();
            let hidden_ref = hidden.clone();
            let right_click = gtk::GestureClick::new();
            right_click.set_button(3);
            right_click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                if *hidden_ref.borrow() {
                    return;
                }
                *hidden_ref.borrow_mut() = true;
                revealer_ref.set_reveal_child(false);
                if let Some(ref cb) = *on_output_ref.borrow() {
                    cb(NotificationCardOutput::Close(urn_clone.clone()));
                }
            });
            root.add_controller(right_click);
        }

        // Left-click for default action
        {
            let on_output_ref = on_output.clone();
            let revealer_ref = revealer.clone();
            let urn_clone = urn.clone();
            let hidden_ref = hidden.clone();
            let left_click = gtk::GestureClick::new();
            left_click.set_button(1);
            left_click.connect_pressed(move |gesture, _n_press, x, y| {
                if *hidden_ref.borrow() {
                    return;
                }

                // Don't fire default action when clicking interactive elements
                if let Some(widget) = gesture.widget() {
                    if let Some(picked) = widget.pick(x, y, gtk::PickFlags::DEFAULT) {
                        let mut current: Option<gtk::Widget> = Some(picked);
                        while let Some(ref w) = current {
                            if w.downcast_ref::<gtk::Button>().is_some() {
                                return;
                            }
                            current = w.parent();
                        }
                    }
                }

                *hidden_ref.borrow_mut() = true;
                if let Some(ref cb) = *on_output_ref.borrow() {
                    cb(NotificationCardOutput::ActionClick(
                        urn_clone.clone(),
                        "default".to_string(),
                    ));
                    cb(NotificationCardOutput::Close(urn_clone.clone()));
                }
                revealer_ref.set_reveal_child(false);
            });
            root.add_controller(left_click);
        }

        Self {
            urn,
            root,
            revealer,
            title_label,
            description_label,
            on_output,
            hidden,
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(NotificationCardOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Show the card with animation.
    pub fn show(&self) {
        self.root.set_visible(true);
        self.revealer.set_reveal_child(true);
    }

    /// Update the card content.
    pub fn update(&self, title: &str, description: &str) {
        let prepared_title = notification_markup::prepare_title(title);
        let prepared_desc = notification_markup::prepare_description(description);
        self.title_label.set_markup(&prepared_title);
        self.description_label.set_markup(&prepared_desc);
    }

    pub fn urn(&self) -> &Urn {
        &self.urn
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

/// Convert protocol notification icon hints to the generic Icon type used by IconWidget.
fn convert_icon_hints(hints: &[NotificationIconHint]) -> Vec<Icon> {
    hints
        .iter()
        .map(|h| match h {
            NotificationIconHint::Themed(name) => Icon::Themed(Arc::from(name.as_str())),
            NotificationIconHint::FilePath(path) => Icon::FilePath(Arc::new(PathBuf::from(path))),
            NotificationIconHint::Bytes(bytes) => Icon::Bytes(bytes.clone()),
        })
        .collect()
}
