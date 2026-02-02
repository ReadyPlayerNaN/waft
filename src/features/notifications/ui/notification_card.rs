//! Notification card widget for the notifications panel.
//!
//! Similar to ToastWidget but styled for the panel and without timeout animations.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use super::notification_layout::{NotificationLayoutConfig, NotificationLayoutParts};
use crate::features::notifications::types::{NotificationAction, NotificationIcon};
use crate::ui::main_window::trigger_window_resize;

/// Output events from a notification card.
#[derive(Debug, Clone)]
pub enum NotificationCardOutput {
    ActionClick(u64, String),
    Close(u64),
}

/// A notification card for the panel.
pub struct NotificationCard {
    pub id: u64,
    pub root: gtk::Box,
    revealer: gtk::Revealer,
    layout: NotificationLayoutParts,
    on_output: Rc<RefCell<Option<Box<dyn Fn(NotificationCardOutput)>>>>,
    hidden: Rc<RefCell<bool>>,
}

impl NotificationCard {
    pub fn new(
        id: u64,
        title: &str,
        description: &str,
        icon_hints: Vec<NotificationIcon>,
        actions: Vec<NotificationAction>,
    ) -> Self {
        // Root container
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Revealer for animations
        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(false)
            .build();
        revealer.add_css_class("notification-card-revealer");

        let on_output: Rc<RefCell<Option<Box<dyn Fn(NotificationCardOutput)>>>> =
            Rc::new(RefCell::new(None));

        // Build shared layout
        let on_output_action = on_output.clone();
        let layout = NotificationLayoutParts::build(
            NotificationLayoutConfig {
                id,
                title: title.to_string(),
                description: description.to_string(),
                icon_hints,
                actions,
                css_classes: vec!["notification-card", "card"],
                show_close_button: true,
                toast_ttl: None,
            },
            move |action_id, action_key| {
                if let Some(ref callback) = *on_output_action.borrow() {
                    callback(NotificationCardOutput::ActionClick(action_id, action_key));
                }
            },
        );

        revealer.set_child(Some(&layout.card_box));
        root.append(&revealer);

        // Track hidden state for gesture handler guards
        let hidden = Rc::new(RefCell::new(false));

        // When revealer finishes animating, trigger window resize and cleanup
        let root_clone = root.clone();
        revealer.connect_child_revealed_notify(move |rev| {
            // Trigger window resize after animation completes
            trigger_window_resize();

            if !rev.is_child_revealed() {
                // Defer widget removal to after current event processing completes
                // This prevents GTK CRITICAL errors when gesture handlers are still active
                let root_for_removal = root_clone.clone();
                gtk::glib::idle_add_local_once(move || {
                    if let Some(parent) = root_for_removal.parent() {
                        if let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
                            parent_box.remove(&root_for_removal);
                        }
                    }
                });
            }
        });

        // Close button click handler
        let on_output_close = on_output.clone();
        let revealer_clone = revealer.clone();
        layout.close_btn.connect_clicked(move |_| {
            revealer_clone.set_reveal_child(false);
            if let Some(ref callback) = *on_output_close.borrow() {
                callback(NotificationCardOutput::Close(id));
            }
        });

        // Right-click to close
        let on_output_right = on_output.clone();
        let revealer_clone = revealer.clone();
        let hidden_clone = hidden.clone();
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.connect_pressed(move |_gesture, _n_press, _x, _y| {
            // Guard: ignore if already hidden
            if *hidden_clone.borrow() {
                return;
            }
            *hidden_clone.borrow_mut() = true;
            revealer_clone.set_reveal_child(false);
            if let Some(ref callback) = *on_output_right.borrow() {
                callback(NotificationCardOutput::Close(id));
            }
        });
        root.add_controller(right_click);

        // Left-click for default action
        let on_output_left = on_output.clone();
        let revealer_clone = revealer.clone();
        let hidden_clone = hidden.clone();
        let left_click = gtk::GestureClick::new();
        left_click.set_button(1);
        left_click.connect_pressed(move |gesture, _n_press, x, y| {
            // Guard: ignore if already hidden
            if *hidden_clone.borrow() {
                return;
            }

            // Check if click is on an interactive element
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

            *hidden_clone.borrow_mut() = true;
            if let Some(ref callback) = *on_output_left.borrow() {
                callback(NotificationCardOutput::ActionClick(
                    id,
                    "default".to_string(),
                ));
                callback(NotificationCardOutput::Close(id));
            }
            revealer_clone.set_reveal_child(false);
        });
        root.add_controller(left_click);

        Self {
            id,
            root,
            revealer,
            layout,
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

    /// Hide the card with animation.
    pub fn hide(&self) {
        self.revealer.set_reveal_child(false);
    }

    /// Check if currently revealed.
    pub fn is_revealed(&self) -> bool {
        self.revealer.reveals_child()
    }

    /// Update the card content.
    pub fn update(&self, title: &str, description: &str) {
        self.layout.update(title, description);
    }

    /// Get the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
